use crate::fixture::{
    AUTHORITY, COLLATERAL, CURVE, FACTORY, FactoryFixture, SALT, TOKEN, TREASURY, balance,
    curve_call, execute, factory_fixture, guest, holding, id, key,
};
use lee::{
    AccountId, Actor, PublicTransaction,
    public_transaction::{Message, WitnessSet},
};
use lee_core::program::TransactionEvent;
use lez_compat_core::{
    authority,
    token::{self, TokenState},
};
use std::collections::BTreeMap;

#[path = "settlement/faults.rs"]
mod faults;

const ADAPTER: AccountId = AccountId::new([19; 32]);

fn saved(fixture: &FactoryFixture) -> factory_core::FactoryState {
    factory_core::FactoryState::decode(
        fixture
            .state
            .get_account_by_id(fixture.factory)
            .data
            .actor_state(FACTORY),
    )
    .unwrap()
}

fn pool_state(fixture: &FactoryFixture) -> curve_core::PoolAccount {
    curve_core::PoolAccount::decode(
        fixture
            .state
            .get_account_by_id(fixture.pool)
            .data
            .actor_state(CURVE),
    )
    .unwrap()
}

fn closed_sale(
    hook: Option<AccountId>,
    configuration: Vec<u8>,
    policy: launchpad_client::GraduationPolicy,
    buy: bool,
) -> FactoryFixture {
    let mut fixture = factory_fixture();
    let creator = key(48);
    fixture
        .actors
        .extend([holding(TREASURY, COLLATERAL), Actor::new(TREASURY, TOKEN)]);
    execute(
        &mut fixture.state,
        Actor::new(id(&key(47)), CURVE),
        fixture.actors.clone(),
        curve_core::Message::UpdateFee {
            namespace: fixture.namespace,
            protocol_fee_bps: 1_000,
        },
        &[&key(47)],
    )
    .unwrap();
    fixture
        .call(
            &creator,
            factory_core::Message::CreateFactoryPool {
                buy_policy: curve_core::BuyPolicy::Open,
                pause_policy: curve_core::PausePolicy::Disabled,
                ending_policy: curve_core::EndingPolicy::Either,
                trade_mode: 0,
                sell_fee_bps: 0,
                per_block_token_limit: None,
                graduation_hook_program_id: hook,
                graduation_hook_configuration: configuration,
                namespace: fixture.namespace,
                launch_salt: SALT,
                name: "Settlement".into(),
                uri: "ipfs://settlement".into(),
                graduation_reserve: 0,
                sale_reserve: 400,
                graduation_policy: policy,
                creator_allocation: 50,
                virtual_token_reserve: 1_000,
                virtual_collateral_reserve: 1_000,
                end_timestamp: Some(301),
                curve_program_id: CURVE,
                collateral_definition_id: COLLATERAL,
                creator: id(&creator),
                now: 1,
            },
        )
        .unwrap();
    assert!(pool_state(&fixture).funded);
    assert_eq!(
        balance(&fixture.state, fixture.pool, fixture.definition),
        400
    );
    for owner in [id(&creator), id(&key(49))] {
        execute(
            &mut fixture.state,
            Actor::new(fixture.definition, TOKEN),
            fixture.actors.clone(),
            token::Message::Initialize { owner },
            &[],
        )
        .unwrap();
    }
    execute(
        &mut fixture.state,
        Actor::new(COLLATERAL, TOKEN),
        fixture.actors.clone(),
        token::Message::Initialize { owner: TREASURY },
        &[],
    )
    .unwrap();
    if buy {
        curve_call(
            &mut fixture.state,
            &fixture.actors,
            fixture.pool,
            &key(49),
            curve_core::Message::SwapExactOutput {
                membership: None,
                amount_out: 100,
                max_amount_in: 125,
                token_in: COLLATERAL,
                now: 1,
            },
        )
        .unwrap();
        assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 125);
        assert_eq!(pool_state(&fixture).pool.fees_accrued, 13);
    }
    fixture.inclusion_timestamp_ms = 301_000;
    fixture
}

fn definition_state(fixture: &FactoryFixture) -> TokenState {
    TokenState::decode(
        fixture
            .state
            .get_account_by_id(fixture.definition)
            .data
            .actor_state(TOKEN),
    )
    .unwrap()
}

fn supply(fixture: &FactoryFixture) -> u128 {
    match definition_state(fixture) {
        TokenState::Definition { supply, .. } => supply,
        _ => panic!("expected token definition"),
    }
}

fn custody(fixture: &FactoryFixture) -> AccountId {
    factory_core::compute_graduation_pda(ADAPTER, fixture.factory)
}

fn adapter_state(fixture: &FactoryFixture) -> factory_core::AdapterState {
    factory_core::AdapterState::decode(
        fixture
            .state
            .get_account_by_id(custody(fixture))
            .data
            .actor_state(ADAPTER),
    )
    .unwrap()
}

fn initialize_adapter(
    fixture: &mut FactoryFixture,
    kind: factory_core::GraduationIntentKind,
) -> Result<(), lee::error::LeeError> {
    let adapter = custody(fixture);
    fixture.actors.extend([
        Actor::new(adapter, ADAPTER),
        Actor::new(adapter, TOKEN),
        holding(adapter, fixture.definition),
        holding(adapter, COLLATERAL),
    ]);
    let signer = key(49);
    let mut message = Message::try_new(
        Actor::new(adapter, ADAPTER),
        fixture.actors.clone(),
        BTreeMap::from([(
            id(&signer),
            fixture.state.get_account_by_id(id(&signer)).nonce,
        )]),
        factory_core::AdapterMessage::Initialize {
            factory_program_id: FACTORY,
            namespace: fixture.namespace,
            launch_salt: SALT,
            accepted_intent: kind,
        },
    )
    .unwrap();
    message
        .admission_evidence
        .push(lee::PublicAccountEvidence::Pda {
            program: ADAPTER,
            seed: factory_core::compute_graduation_seed(fixture.factory),
        });
    let witnesses = WitnessSet::for_message(&message, &[&signer]);
    fixture
        .state
        .transition_from_public_transaction(
            &PublicTransaction::new(message, witnesses),
            1,
            fixture.inclusion_timestamp_ms,
        )
        .map(|_| ())
}

fn manual(fixture: &mut FactoryFixture) -> Result<(), lee::error::LeeError> {
    manual_events(fixture).map(|_| ())
}

fn manual_events(
    fixture: &mut FactoryFixture,
) -> Result<Vec<TransactionEvent>, lee::error::LeeError> {
    fixture.call_with_events(
        &key(48),
        factory_core::Message::WithdrawFactoryProceeds {
            creator: id(&key(48)),
            now: 301,
        },
    )
}

fn claim(fixture: &mut FactoryFixture) -> Result<(), lee::error::LeeError> {
    claim_events(fixture).map(|_| ())
}

fn claim_events(
    fixture: &mut FactoryFixture,
) -> Result<Vec<TransactionEvent>, lee::error::LeeError> {
    fixture.call_with_events(
        &key(48),
        factory_core::Message::ClaimCreatorAllocation {
            creator: id(&key(48)),
            now: 301,
        },
    )
}

fn hook(fixture: &mut FactoryFixture) -> Result<(), lee::error::LeeError> {
    hook_events(fixture).map(|_| ())
}

fn hook_events(
    fixture: &mut FactoryFixture,
) -> Result<Vec<TransactionEvent>, lee::error::LeeError> {
    fixture.call_with_events(
        &key(49),
        factory_core::Message::ExecuteGraduationHook {
            creator_hint: Some(id(&key(48))),
        },
    )
}

fn rejected<T: std::fmt::Debug>(
    fixture: &mut FactoryFixture,
    operation: impl FnOnce(&mut FactoryFixture) -> Result<T, lee::error::LeeError>,
    reason: &str,
) {
    let before = fixture.state.clone();
    let result = operation(fixture);
    assert!(
        matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains(reason)),
        "expected {reason}, got {result:?}"
    );
    assert!(
        fixture.state == before,
        "rejection changed accounts or signer nonce"
    );
}

fn finish_manual(fixture: &mut FactoryFixture) {
    for _ in 0..4 {
        if saved(fixture).settlement_stage == factory_core::SettlementStage::Complete {
            return;
        }
        manual(fixture).unwrap();
    }
    assert_eq!(
        saved(fixture).settlement_stage,
        factory_core::SettlementStage::Complete
    );
}

fn prepare_hook(fixture: &mut FactoryFixture) {
    for _ in 0..4 {
        if matches!(
            saved(fixture).settlement_stage,
            factory_core::SettlementStage::HookReady { .. }
        ) {
            return;
        }
        hook(fixture).unwrap();
    }
    assert!(matches!(
        saved(fixture).settlement_stage,
        factory_core::SettlementStage::HookReady { .. }
    ));
}

fn hook_fixture(buy: bool, policy: launchpad_client::GraduationPolicy) -> FactoryFixture {
    let mut fixture = closed_sale(Some(ADAPTER), b"saved-route".to_vec(), policy, buy);
    fixture.state = fixture.state.with_programs_at(
        ([(ADAPTER, guest("LAUNCHPAD_ADAPTER_ELF"))])
            .map(|(address, program)| (address, program, true)),
    );
    initialize_adapter(&mut fixture, factory_core::GraduationIntentKind::Curve).unwrap();
    prepare_hook(&mut fixture);
    fixture
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn settlement_manual_pays_unsold_and_recorded_collateral_without_burning() {
    for buy in [false, true] {
        let mut fixture = closed_sale(
            None,
            vec![],
            launchpad_client::GraduationPolicy::default(),
            buy,
        );
        finish_manual(&mut fixture);
        let creator = id(&key(48));
        assert_eq!(
            balance(&fixture.state, creator, fixture.definition),
            if buy { 300 } else { 400 }
        );
        assert_eq!(
            balance(&fixture.state, creator, COLLATERAL),
            if buy { 1112 } else { 1000 }
        );
        assert_eq!(
            balance(&fixture.state, TREASURY, COLLATERAL),
            if buy { 13 } else { 0 }
        );
        assert_eq!(supply(&fixture), 450);
        assert_eq!(
            balance(
                &fixture.state,
                saved(&fixture).creator_escrow_id,
                fixture.definition
            ),
            50
        );
        rejected(&mut fixture, manual, "AlreadySettled");
        claim(&mut fixture).unwrap();
        assert_eq!(
            balance(&fixture.state, creator, fixture.definition),
            if buy { 350 } else { 450 }
        );
    }
}

#[test]
#[ignore = "requires fresh application, adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_graduation_splits_both_assets_and_rejects_manual_override() {
    for buy in [false, true] {
        for policy in [
            launchpad_client::GraduationPolicy {
                launch_bps: 0,
                collateral_bps: 0,
            },
            launchpad_client::GraduationPolicy {
                launch_bps: 2500,
                collateral_bps: 7500,
            },
            launchpad_client::GraduationPolicy {
                launch_bps: 10000,
                collateral_bps: 10000,
            },
        ] {
            let mut fixture = hook_fixture(buy, policy);
            rejected(&mut fixture, manual, "InvalidTerms");
            let inventory = if buy { 300 } else { 400 };
            let collateral = if buy { 112 } else { 0 };
            let launch_adapter = inventory * u128::from(policy.launch_bps) / 10000;
            let collateral_adapter = collateral * u128::from(policy.collateral_bps) / 10000;
            hook(&mut fixture).unwrap();
            let adapter = custody(&fixture);
            let creator = id(&key(48));
            assert_eq!(
                balance(&fixture.state, adapter, fixture.definition),
                launch_adapter
            );
            assert_eq!(
                balance(&fixture.state, adapter, COLLATERAL),
                collateral_adapter
            );
            assert_eq!(
                balance(&fixture.state, creator, fixture.definition),
                inventory - launch_adapter
            );
            assert_eq!(
                balance(&fixture.state, creator, COLLATERAL),
                1000 + collateral - collateral_adapter
            );
            assert_eq!(supply(&fixture), 450);
            assert_eq!(
                adapter_state(&fixture).receipt.unwrap().intent,
                factory_core::GraduationIntent::Curve {
                    launch_tokens: launch_adapter,
                    collateral: collateral_adapter
                }
            );
            assert_eq!(
                saved(&fixture).settlement_stage,
                factory_core::SettlementStage::Graduated
            );
            rejected(&mut fixture, hook, "AlreadySettled");
            rejected(&mut fixture, manual, "InvalidTerms");
        }
    }
}

#[test]
#[ignore = "requires fresh application, adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_adapter_initializes_once_with_required_kind() {
    let mut fixture = hook_fixture(
        true,
        launchpad_client::GraduationPolicy {
            launch_bps: 2500,
            collateral_bps: 7500,
        },
    );
    assert_eq!(
        adapter_state(&fixture).accepted_intent,
        factory_core::GraduationIntentKind::Curve
    );
    let adapter = custody(&fixture);
    let initialize = factory_core::AdapterMessage::Initialize {
        factory_program_id: FACTORY,
        namespace: fixture.namespace,
        launch_salt: SALT,
        accepted_intent: factory_core::GraduationIntentKind::Weighted,
    };
    for completed in [false, true] {
        if completed {
            hook(&mut fixture).unwrap();
        }
        let before = fixture.state.clone();
        let result = execute(
            &mut fixture.state,
            Actor::new(adapter, ADAPTER),
            fixture.actors.clone(),
            initialize.clone(),
            &[&key(49)],
        );
        assert!(format!("{result:?}").contains("AlreadyInitialized"));
        assert!(fixture.state == before);
    }
}

#[test]
#[ignore = "requires fresh application, adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_wrong_adapter_kind_cannot_enable_manual_fallback() {
    let mut fixture = closed_sale(
        Some(ADAPTER),
        vec![],
        launchpad_client::GraduationPolicy {
            launch_bps: 2500,
            collateral_bps: 7500,
        },
        true,
    );
    fixture.state = fixture.state.with_programs_at(
        ([(ADAPTER, guest("LAUNCHPAD_ADAPTER_ELF"))])
            .map(|(address, program)| (address, program, true)),
    );
    let before = fixture.state.clone();
    let result = initialize_adapter(&mut fixture, factory_core::GraduationIntentKind::Weighted);
    assert!(format!("{result:?}").contains("AdapterIntentMismatch"));
    assert!(fixture.state == before);
    initialize_adapter(&mut fixture, factory_core::GraduationIntentKind::Curve).unwrap();
    prepare_hook(&mut fixture);
    hook(&mut fixture).unwrap();
    rejected(&mut fixture, manual, "InvalidTerms");
}

#[test]
#[ignore = "requires fresh application, adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_graduation_resolves_transferred_creator_and_rejects_old_hint() {
    let mut fixture = hook_fixture(
        true,
        launchpad_client::GraduationPolicy {
            launch_bps: 2500,
            collateral_bps: 7500,
        },
    );
    let former = key(48);
    let successor = key(49);
    execute(
        &mut fixture.state,
        Actor::new(id(&former), AUTHORITY),
        fixture.actors.clone(),
        authority::Message::Transfer {
            role: factory_core::compute_curve_creator_role_pda(fixture.namespace, FACTORY, SALT),
            successor: id(&successor),
        },
        &[&former, &successor],
    )
    .unwrap();
    rejected(&mut fixture, hook, "Unauthorized");
    fixture
        .call(
            &key(48),
            factory_core::Message::ExecuteGraduationHook {
                creator_hint: Some(id(&successor)),
            },
        )
        .unwrap();
    assert_eq!(balance(&fixture.state, id(&former), fixture.definition), 0);
    assert_eq!(balance(&fixture.state, id(&former), COLLATERAL), 1000);
    assert_eq!(
        balance(&fixture.state, id(&successor), fixture.definition),
        325
    );
    assert_eq!(balance(&fixture.state, id(&successor), COLLATERAL), 903);
    assert_eq!(
        balance(&fixture.state, custody(&fixture), fixture.definition),
        75
    );
    assert_eq!(balance(&fixture.state, custody(&fixture), COLLATERAL), 84);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn settlement_while_frozen_keeps_creator_checks_and_completes_fees_and_payouts() {
    let mut fixture = closed_sale(
        None,
        vec![],
        launchpad_client::GraduationPolicy::default(),
        true,
    );
    let emergency = key(52);
    fixture.actors.push(Actor::new(id(&emergency), CURVE));
    execute(
        &mut fixture.state,
        Actor::new(id(&emergency), CURVE),
        fixture.actors.clone(),
        curve_core::Message::SetFrozen {
            namespace: fixture.namespace,
            frozen: true,
        },
        &[&emergency],
    )
    .unwrap();
    let config = curve_core::Config::decode(
        fixture
            .state
            .get_account_by_id(curve_core::compute_config_pda(fixture.namespace, CURVE))
            .data
            .actor_state(CURVE),
    )
    .unwrap();
    assert!(config.emergency.is_frozen);
    claim(&mut fixture).unwrap();
    for _ in 0..3 {
        rejected(
            &mut fixture,
            |fixture| {
                fixture.call(
                    &key(49),
                    factory_core::Message::WithdrawFactoryProceeds {
                        creator: id(&key(49)),
                        now: 301,
                    },
                )
            },
            "Unauthorized",
        );
        manual(&mut fixture).unwrap();
    }
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::Complete
    );
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 13);
    assert_eq!(
        balance(&fixture.state, id(&key(48)), fixture.definition),
        350
    );
    assert_eq!(balance(&fixture.state, id(&key(48)), COLLATERAL), 1_112);
    assert_eq!(balance(&fixture.state, fixture.pool, fixture.definition), 0);
    assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 0);
    assert_eq!(supply(&fixture), 450);
}
