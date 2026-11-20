use super::*;
use lee_core::program::TransactionEvent;

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
    for _ in 0..3 {
        fixture
            .call(
                &creator,
                factory_core::Message::ContinueCreation {
                    creator: id(&creator),
                    now: 1,
                },
            )
            .unwrap();
    }
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
fn curve_late_activation_failure_restores_funding_and_continuation_activates_once() {
    let mut fixture = factory_fixture();
    let creator = key(48);
    fixture
        .call(
            &creator,
            factory_core::Message::CreateFactoryPool {
                buy_policy: curve_core::BuyPolicy::Open,
                pause_policy: curve_core::PausePolicy::Disabled,
                ending_policy: curve_core::EndingPolicy::Either,
                trade_mode: 0,
                sell_fee_bps: 0,
                graduation_hook_program_id: None,
                graduation_hook_configuration: vec![],
                namespace: fixture.namespace,
                launch_salt: SALT,
                name: "Recovery".into(),
                uri: "ipfs://recovery".into(),
                graduation_reserve: 0,
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
            },
        )
        .unwrap();
    let continuation = factory_core::Message::ContinueCreation {
        creator: id(&creator),
        now: 1,
    };
    for stage in [
        factory_core::CreationStage::Allocated,
        factory_core::CreationStage::PoolPrepared,
    ] {
        fixture.call(&creator, continuation.clone()).unwrap();
        assert_eq!(saved(&fixture).creation_stage, stage);
    }
    let pool = fixture.pool;
    set_holding(&mut fixture, pool, COLLATERAL, LAUNCH, 0);
    rejected(
        &mut fixture,
        |fixture| fixture.call(&creator, continuation.clone()),
        "InvalidToken",
    );
    assert_eq!(
        saved(&fixture).creation_stage,
        factory_core::CreationStage::PoolPrepared
    );
    assert!(!pool_state(&fixture).funded);
    assert_eq!(
        balance(&fixture.state, fixture.factory, fixture.definition),
        400
    );
    assert_eq!(balance(&fixture.state, pool, fixture.definition), 0);
    assert_eq!(
        balance(
            &fixture.state,
            saved(&fixture).creator_escrow_id,
            fixture.definition
        ),
        50
    );
    assert_eq!(supply(&fixture), 450);
    set_holding(&mut fixture, pool, COLLATERAL, COLLATERAL, 0);
    fixture.call(&creator, continuation.clone()).unwrap();
    assert_eq!(
        saved(&fixture).creation_stage,
        factory_core::CreationStage::Active
    );
    assert!(pool_state(&fixture).funded);
    assert_eq!(
        balance(&fixture.state, fixture.factory, fixture.definition),
        0
    );
    assert_eq!(balance(&fixture.state, pool, fixture.definition), 400);
    assert_eq!(
        balance(
            &fixture.state,
            saved(&fixture).creator_escrow_id,
            fixture.definition
        ),
        50
    );
    assert_eq!(supply(&fixture), 450);
    rejected(
        &mut fixture,
        |fixture| fixture.call(&creator, continuation),
        "StaleStage",
    );
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
