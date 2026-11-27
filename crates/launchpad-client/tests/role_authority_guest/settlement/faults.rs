use super::{
    ADAPTER, adapter_state, claim, closed_sale, custody, finish_manual, hook, hook_fixture,
    initialize_adapter, manual, manual_events, pool_state, prepare_hook, rejected, saved, supply,
};
use crate::fixture::{
    COLLATERAL, CURVE, FACTORY, FactoryFixture, LAUNCH, SALT, TREASURY, account, balance,
    factory_fixture, guest, holding, id, key,
};
use lee::{AccountId, Actor};
use lez_compat_core::token::{TokenKind, TokenState};

fn set_holding(
    fixture: &mut FactoryFixture,
    owner: AccountId,
    definition: AccountId,
    stored_definition: AccountId,
    amount: u128,
) {
    fixture.state = fixture.state.clone().with_public_accounts([account(
        holding(owner, definition),
        borsh::to_vec(&TokenState::Holding {
            definition: stored_definition,
            owner,
            kind: TokenKind::Fungible,
            balance: amount,
        })
        .unwrap(),
    )]);
}

#[test]
#[ignore = "requires fresh application, adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_creator_credit_failure_rolls_back_both_destinations_and_retries() {
    let mut fixture = hook_fixture(
        true,
        launchpad_client::GraduationPolicy {
            launch_bps: 2500,
            collateral_bps: 7500,
        },
    );
    let creator = id(&key(48));
    set_holding(&mut fixture, creator, COLLATERAL, LAUNCH, 1000);
    rejected(&mut fixture, hook, "WrongDefinition");
    assert_eq!(
        balance(&fixture.state, custody(&fixture), fixture.definition),
        0
    );
    assert_eq!(balance(&fixture.state, custody(&fixture), COLLATERAL), 0);
    assert_eq!(balance(&fixture.state, creator, fixture.definition), 0);
    assert!(adapter_state(&fixture).receipt.is_none());
    rejected(&mut fixture, manual, "InvalidTerms");
    set_holding(&mut fixture, creator, COLLATERAL, COLLATERAL, 1000);
    hook(&mut fixture).unwrap();
    assert_eq!(balance(&fixture.state, creator, fixture.definition), 225);
    assert_eq!(balance(&fixture.state, creator, COLLATERAL), 1028);
    assert_eq!(
        balance(&fixture.state, custody(&fixture), fixture.definition),
        75
    );
    assert_eq!(balance(&fixture.state, custody(&fixture), COLLATERAL), 84);
}

#[test]
#[ignore = "requires fresh application, adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_callback_failure_restores_payouts_and_preserves_required_route() {
    let mut fixture = fault_hook_fixture(b"reject-callback".to_vec());
    rejected(&mut fixture, hook, "callback rejected after transfers");
    assert_eq!(
        balance(&fixture.state, custody(&fixture), fixture.definition),
        0
    );
    assert_eq!(balance(&fixture.state, id(&key(48)), fixture.definition), 0);
    assert!(adapter_state(&fixture).receipt.is_none());
    rejected(&mut fixture, manual, "InvalidTerms");
    fixture.state = fixture.state.with_programs_at(
        ([(ADAPTER, guest("LAUNCHPAD_ADAPTER_ELF"))])
            .map(|(address, program)| (address, program, true)),
    );
    hook(&mut fixture).unwrap();
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::Graduated
    );
    assert_eq!(supply(&fixture), 450);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn settlement_manual_credit_failure_is_atomic_and_retry_keeps_surplus() {
    let mut fixture = closed_sale(
        None,
        vec![],
        launchpad_client::GraduationPolicy::default(),
        true,
    );
    manual(&mut fixture).unwrap();
    manual(&mut fixture).unwrap();
    let factory = fixture.factory;
    set_holding(&mut fixture, factory, COLLATERAL, COLLATERAL, 129);
    let creator = id(&key(48));
    set_holding(&mut fixture, creator, COLLATERAL, LAUNCH, 1000);
    rejected(&mut fixture, manual, "WrongDefinition");
    assert_eq!(balance(&fixture.state, creator, fixture.definition), 0);
    set_holding(&mut fixture, creator, COLLATERAL, COLLATERAL, 1000);
    finish_manual(&mut fixture);
    assert_eq!(balance(&fixture.state, creator, fixture.definition), 300);
    assert_eq!(balance(&fixture.state, creator, COLLATERAL), 1112);
    assert_eq!(balance(&fixture.state, factory, COLLATERAL), 17);
    assert_eq!(supply(&fixture), 450);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn curve_funding_failure_rolls_back_creation_and_allows_same_create_retry() {
    let mut fixture = factory_fixture();
    let creator = key(48);
    let create = factory_core::Message::CreateFactoryPool {
        buy_policy: curve_core::BuyPolicy::Open,
        pause_policy: curve_core::PausePolicy::Disabled,
        ending_policy: curve_core::EndingPolicy::Either,
        trade_mode: 0,
        sell_fee_bps: 0,
        per_block_token_limit: None,
        graduation_hook_program_id: None,
        graduation_hook_configuration: vec![],
        namespace: fixture.namespace,
        launch_salt: SALT,
        name: "Recovery".into(),
        uri: "ipfs://recovery".into(),
        graduation_reserve: 25,
        sale_reserve: 400,
        graduation_policy: launchpad_client::GraduationPolicy::default(),
        creator_allocation: 50,
        virtual_token_reserve: 1_000,
        virtual_collateral_reserve: 1_000,
        end_timestamp: Some(301),
        curve_program_id: CURVE,
        collateral_definition_id: COLLATERAL,
        creator: id(&creator),
        now: 1,
    };
    let pool = fixture.pool;
    let definition = fixture.definition;
    // The pool ATA can initialize, but funding requires an empty launch-token balance.
    set_holding(&mut fixture, pool, definition, definition, 1);
    rejected(
        &mut fixture,
        |fixture| fixture.call(&creator, create.clone()),
        "InvalidCustody",
    );
    rejected(
        &mut fixture,
        |fixture| {
            fixture.call(
                &creator,
                factory_core::Message::ContinueCreation {
                    creator: id(&creator),
                    now: 1,
                },
            )
        },
        "Unauthorized",
    );

    set_holding(&mut fixture, pool, definition, definition, 0);
    fixture.call(&creator, create).unwrap();
    let sale = saved(&fixture);
    assert_eq!(sale.creation_stage, factory_core::CreationStage::Active);
    assert!(pool_state(&fixture).funded);
    assert_eq!(balance(&fixture.state, fixture.factory, definition), 0);
    assert_eq!(balance(&fixture.state, pool, definition), 400);
    assert_eq!(balance(&fixture.state, pool, COLLATERAL), 0);
    assert_eq!(
        balance(&fixture.state, sale.creator_escrow_id, definition),
        50
    );
    assert_eq!(
        balance(
            &fixture.state,
            factory_core::compute_mint_pda(fixture.namespace, FACTORY, SALT),
            definition
        ),
        25
    );
    assert_eq!(supply(&fixture), 475);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn settlement_reserve0_credit_failure_restores_fees_and_reserves() {
    let mut fixture = closed_sale(
        None,
        vec![],
        launchpad_client::GraduationPolicy::default(),
        true,
    );
    manual(&mut fixture).unwrap();
    let factory = fixture.factory;
    let definition = fixture.definition;
    set_holding(&mut fixture, factory, definition, definition, u128::MAX);
    rejected(&mut fixture, manual, "Overflow");
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 0);
    assert_eq!(balance(&fixture.state, fixture.pool, definition), 300);
    set_holding(&mut fixture, factory, definition, definition, 0);
    manual(&mut fixture).unwrap();
    assert_eq!(balance(&fixture.state, factory, definition), 300);
    assert_eq!(balance(&fixture.state, factory, COLLATERAL), 112);
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 13);
    assert_eq!(balance(&fixture.state, fixture.pool, definition), 0);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn settlement_reserve1_credit_failure_restores_prior_reserve0_and_fee_payments() {
    let mut fixture = closed_sale(
        None,
        vec![],
        launchpad_client::GraduationPolicy::default(),
        true,
    );
    manual(&mut fixture).unwrap();
    let factory = fixture.factory;
    set_holding(&mut fixture, factory, COLLATERAL, COLLATERAL, u128::MAX);
    rejected(&mut fixture, manual, "Overflow");
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 0);
    assert_eq!(
        balance(&fixture.state, fixture.pool, fixture.definition),
        300
    );
    assert_eq!(balance(&fixture.state, factory, fixture.definition), 0);
    set_holding(&mut fixture, factory, COLLATERAL, COLLATERAL, 0);
    manual(&mut fixture).unwrap();
    assert_eq!(balance(&fixture.state, factory, fixture.definition), 300);
    assert_eq!(balance(&fixture.state, factory, COLLATERAL), 112);
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 13);
    assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 0);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn settlement_fee_transfer_failure_rolls_back_and_retry_collects_once() {
    let mut fixture = closed_sale(
        None,
        vec![],
        launchpad_client::GraduationPolicy::default(),
        true,
    );
    let mut events = manual_events(&mut fixture).unwrap();
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::Ready
    );
    set_holding(&mut fixture, TREASURY, COLLATERAL, LAUNCH, 0);
    rejected(&mut fixture, manual_events, "WrongDefinition");
    assert_eq!(pool_state(&fixture).pool.fees_accrued, 13);
    set_holding(&mut fixture, TREASURY, COLLATERAL, COLLATERAL, 0);
    events.extend(manual_events(&mut fixture).unwrap());
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 13);
    assert_eq!(balance(&fixture.state, fixture.factory, COLLATERAL), 112);
    assert_eq!(pool_state(&fixture).pool.fees_accrued, 0);
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::Withdrawn {
            unsold: 300,
            collateral_due: 112
        }
    );
}

fn fault_hook_fixture(configuration: Vec<u8>) -> FactoryFixture {
    let mut fixture = closed_sale(
        Some(ADAPTER),
        configuration,
        launchpad_client::GraduationPolicy {
            launch_bps: 2500,
            collateral_bps: 7500,
        },
        true,
    );
    fixture.state = fixture.state.with_programs_at(
        ([(ADAPTER, guest("LAUNCHPAD_GRADUATION_FAULT_ELF"))])
            .map(|(address, program)| (address, program, true)),
    );
    initialize_adapter(&mut fixture, factory_core::GraduationIntentKind::Curve).unwrap();
    prepare_hook(&mut fixture);
    fixture
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn settlement_allocation_credit_failure_preserves_escrow_and_retry_pays_once() {
    let mut fixture = closed_sale(
        None,
        vec![],
        launchpad_client::GraduationPolicy::default(),
        true,
    );
    let creator = id(&key(48));
    let definition = fixture.definition;
    set_holding(&mut fixture, creator, definition, COLLATERAL, 0);
    rejected(&mut fixture, claim, "WrongDefinition");
    assert!(!saved(&fixture).creator_allocation_claimed);
    assert_eq!(
        balance(
            &fixture.state,
            saved(&fixture).creator_escrow_id,
            definition
        ),
        50
    );
    set_holding(&mut fixture, creator, definition, definition, 0);
    claim(&mut fixture).unwrap();
    assert_eq!(balance(&fixture.state, creator, definition), 50);
    assert_eq!(
        balance(
            &fixture.state,
            saved(&fixture).creator_escrow_id,
            definition
        ),
        0
    );
    rejected(&mut fixture, claim, "AlreadyClaimed");
    finish_manual(&mut fixture);
    assert_eq!(balance(&fixture.state, creator, definition), 350);
}

#[test]
#[ignore = "requires fresh application, fault adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_final_ack_failure_rolls_back_adapter_receipt_payouts_and_graduated_marker() {
    let mut fixture = fault_hook_fixture(b"corrupt-final-ack".to_vec());
    rejected(&mut fixture, hook, "InvalidState");
    assert_hook_custody(&fixture, false);
    fixture.state = fixture.state.with_programs_at(
        ([(ADAPTER, guest("LAUNCHPAD_ADAPTER_ELF"))])
            .map(|(address, program)| (address, program, true)),
    );
    hook(&mut fixture).unwrap();
    assert_hook_custody(&fixture, true);
    rejected(&mut fixture, hook, "AlreadySettled");
}

#[test]
#[ignore = "requires fresh application, adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_adapter_credit_failures_restore_both_assets_and_retry_once() {
    for collateral in [false, true] {
        let mut fixture = hook_fixture(
            true,
            launchpad_client::GraduationPolicy {
                launch_bps: 2500,
                collateral_bps: 7500,
            },
        );
        let adapter = custody(&fixture);
        let definition = if collateral {
            COLLATERAL
        } else {
            fixture.definition
        };
        let wrong = if collateral { LAUNCH } else { COLLATERAL };
        set_holding(&mut fixture, adapter, definition, wrong, 0);
        rejected(&mut fixture, hook, "WrongDefinition");
        assert_hook_custody(&fixture, false);
        set_holding(&mut fixture, adapter, definition, definition, 0);
        hook(&mut fixture).unwrap();
        assert_hook_custody(&fixture, true);
    }
}

fn nested_configuration(target: Actor, message: impl borsh::BorshSerialize) -> Vec<u8> {
    let mut configuration = b"nested-call:".to_vec();
    configuration.extend_from_slice(target.account_id.value());
    configuration.extend_from_slice(target.program_account_id.value());
    configuration.extend(borsh::to_vec(&message).unwrap());
    configuration
}

fn creator_hook(fixture: &mut FactoryFixture) -> Result<(), lee::error::LeeError> {
    fixture.call(
        &key(48),
        factory_core::Message::ExecuteGraduationHook {
            creator_hint: Some(id(&key(48))),
        },
    )
}

fn assert_hook_custody(fixture: &FactoryFixture, graduated: bool) {
    let adapter = custody(fixture);
    assert_eq!(
        saved(fixture).settlement_stage,
        if graduated {
            factory_core::SettlementStage::Graduated
        } else {
            factory_core::SettlementStage::HookReady {
                unsold: 300,
                collateral_due: 112,
            }
        }
    );
    assert_eq!(adapter_state(fixture).receipt.is_some(), graduated);
    assert_eq!(
        balance(&fixture.state, adapter, fixture.definition),
        if graduated { 75 } else { 0 }
    );
    assert_eq!(
        balance(&fixture.state, adapter, COLLATERAL),
        if graduated { 84 } else { 0 }
    );
    assert_eq!(
        balance(&fixture.state, fixture.factory, fixture.definition),
        if graduated { 0 } else { 300 }
    );
    assert_eq!(
        balance(&fixture.state, fixture.factory, COLLATERAL),
        if graduated { 0 } else { 112 }
    );
    assert_eq!(
        balance(&fixture.state, id(&key(48)), fixture.definition),
        if graduated { 225 } else { 0 }
    );
    assert_eq!(
        balance(&fixture.state, id(&key(48)), COLLATERAL),
        if graduated { 1_028 } else { 1_000 }
    );
    assert_eq!(
        balance(
            &fixture.state,
            saved(fixture).creator_escrow_id,
            fixture.definition
        ),
        50
    );
    assert!(!saved(fixture).creator_allocation_claimed);
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 13);
    assert_eq!(balance(&fixture.state, fixture.pool, fixture.definition), 0);
    assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 0);
    assert_eq!(supply(fixture), 450);
}

#[test]
#[ignore = "requires fresh application, fault adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_nested_adapter_calls_roll_back_and_repaired_retry_pays_once() {
    let namespace = id(&key(47));
    let factory = factory_core::compute_factory_pda(namespace, FACTORY, SALT);
    let definition = factory_core::compute_definition_pda(namespace, FACTORY, SALT);
    let pool = curve_core::compute_pool_pda(namespace, CURVE, definition, COLLATERAL, factory);
    let factory_actor = Actor::new(factory, FACTORY);
    let mut attempts = vec![];
    for message in [
        factory_core::Message::ExecuteGraduationHook {
            creator_hint: Some(id(&key(48))),
        },
        factory_core::Message::WithdrawFactoryProceeds {
            creator: id(&key(48)),
            now: 301,
        },
        factory_core::Message::ClaimCreatorAllocation {
            creator: id(&key(48)),
            now: 301,
        },
        factory_core::Message::ContinueCreation {
            creator: id(&key(48)),
            now: 301,
        },
    ] {
        attempts.push((
            nested_configuration(factory_actor, message),
            "InvalidMessage",
        ));
    }
    // The valid nested acknowledgment is accepted first. The queued normal acknowledgment
    // then fails as a duplicate and rolls back the transaction.
    attempts.push((
        nested_configuration(
            factory_actor,
            factory_core::GraduationResult {
                factory,
                request_hash: [0; 32],
            },
        ),
        "InvalidState",
    ));
    for message in [
        curve_core::Message::ClosePool { now: 301 },
        curve_core::Message::WithdrawReserves { now: 301 },
    ] {
        attempts.push((
            nested_configuration(Actor::new(pool, CURVE), message),
            "Unauthorized",
        ));
    }
    attempts.push((
        nested_configuration(
            Actor::new(curve_core::compute_config_pda(namespace, CURVE), CURVE),
            curve_core::Message::UpdateTreasury {
                namespace,
                treasury: factory_core::compute_graduation_pda(ADAPTER, factory),
            },
        ),
        "Unauthorized",
    ));
    for (configuration, reason) in attempts {
        let mut fixture = fault_hook_fixture(configuration.clone());
        // The creator signs so ambient signer access cannot explain the transaction failure.
        rejected(&mut fixture, creator_hook, reason);
        assert_hook_custody(&fixture, false);
        // Replace only test code, not the saved adapter identity or configuration.
        fixture.state = fixture.state.with_programs_at(
            ([(ADAPTER, guest("LAUNCHPAD_ADAPTER_ELF"))])
                .map(|(address, program)| (address, program, true)),
        );
        creator_hook(&mut fixture).unwrap();
        assert_hook_custody(&fixture, true);
        assert_eq!(
            adapter_state(&fixture).receipt.unwrap().configuration,
            configuration
        );
        rejected(&mut fixture, creator_hook, "AlreadySettled");
        rejected(&mut fixture, manual, "InvalidTerms");
        assert_hook_custody(&fixture, true);
    }
}

#[test]
#[ignore = "requires fresh application, fault adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_nested_zero_fee_collection_permits_graduation() {
    let namespace = id(&key(47));
    let factory = factory_core::compute_factory_pda(namespace, FACTORY, SALT);
    let definition = factory_core::compute_definition_pda(namespace, FACTORY, SALT);
    let pool = curve_core::compute_pool_pda(namespace, CURVE, definition, COLLATERAL, factory);
    let configuration =
        nested_configuration(Actor::new(pool, CURVE), curve_core::Message::CollectFees);
    let mut fixture = fault_hook_fixture(configuration);
    assert_hook_custody(&fixture, false);
    creator_hook(&mut fixture).unwrap();
    assert_hook_custody(&fixture, true);
    assert_eq!(pool_state(&fixture).pool.fees_accrued, 0);
    rejected(&mut fixture, creator_hook, "AlreadySettled");
    assert_hook_custody(&fixture, true);
}
