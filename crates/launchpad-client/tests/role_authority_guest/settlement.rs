use super::*;

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
    reserve: u128,
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
                graduation_hook_program_id: hook,
                graduation_hook_configuration: configuration,
                namespace: fixture.namespace,
                launch_salt: SALT,
                name: "Settlement".into(),
                uri: "ipfs://settlement".into(),
                sale_reserve: 400,
                dex_seed_reserve: reserve,
                creator_allocation: 50,
                virtual_token_reserve: 1_000,
                virtual_collateral_reserve: 1_000,
                end_timestamp: Some(2),
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
    fixture.inclusion_timestamp_ms = 2_000;
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

fn set_definition(fixture: &mut FactoryFixture, value: TokenState) {
    fixture.state = fixture.state.clone().with_public_accounts([account(
        Actor::new(fixture.definition, TOKEN),
        borsh::to_vec(&value).unwrap(),
    )]);
}

fn supply(fixture: &FactoryFixture) -> u128 {
    match definition_state(fixture) {
        TokenState::Definition { supply, .. } => supply,
        _ => panic!("expected token definition"),
    }
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
                graduation_hook_program_id: None,
                graduation_hook_configuration: vec![],
                namespace: fixture.namespace,
                launch_salt: SALT,
                name: "Recovery".into(),
                uri: "ipfs://recovery".into(),
                sale_reserve: 400,
                dex_seed_reserve: 100,
                creator_allocation: 50,
                virtual_token_reserve: 1_000,
                virtual_collateral_reserve: 1_000,
                end_timestamp: Some(2),
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
        500
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
    assert_eq!(supply(&fixture), 550);
    set_holding(&mut fixture, pool, COLLATERAL, COLLATERAL, 0);
    fixture.call(&creator, continuation.clone()).unwrap();
    assert_eq!(
        saved(&fixture).creation_stage,
        factory_core::CreationStage::Active
    );
    assert!(pool_state(&fixture).funded);
    assert_eq!(
        balance(&fixture.state, fixture.factory, fixture.definition),
        100
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
    assert_eq!(supply(&fixture), 550);
    rejected(
        &mut fixture,
        |fixture| fixture.call(&creator, continuation),
        "StaleStage",
    );
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn settlement_burn_supply_failure_restores_debited_inventory_and_retry_burns_once() {
    let mut fixture = closed_sale(None, vec![], 100, true);
    manual(&mut fixture).unwrap();
    manual(&mut fixture).unwrap();
    let valid = definition_state(&fixture);
    let mut invalid = valid.clone();
    let TokenState::Definition {
        supply: stored_supply,
        ..
    } = &mut invalid
    else {
        unreachable!()
    };
    *stored_supply = 299;
    set_definition(&mut fixture, invalid);
    rejected(&mut fixture, manual, "InsufficientBalance");
    assert_eq!(
        balance(&fixture.state, fixture.factory, fixture.definition),
        400
    );
    set_definition(&mut fixture, valid);
    manual(&mut fixture).unwrap();
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::Burned {
            collateral_due: 112
        }
    );
    assert_eq!(supply(&fixture), 250);
    assert_eq!(
        balance(&fixture.state, fixture.factory, fixture.definition),
        100
    );
    manual(&mut fixture).unwrap();
    manual(&mut fixture).unwrap();
    assert_eq!(supply(&fixture), 250);
    assert_eq!(
        balance(&fixture.state, id(&key(48)), fixture.definition),
        100
    );
    assert_eq!(balance(&fixture.state, id(&key(48)), COLLATERAL), 1_112);
    rejected(&mut fixture, manual, "AlreadySettled");
}

fn claim(fixture: &mut FactoryFixture) -> Result<(), lee::error::LeeError> {
    fixture.call(
        &key(48),
        factory_core::Message::ClaimCreatorAllocation {
            creator: id(&key(48)),
            now: 2,
        },
    )
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn settlement_allocation_credit_failure_preserves_escrow_and_retry_pays_once() {
    let mut fixture = closed_sale(None, vec![], 100, true);
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
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::Unstarted
    );
    rejected(&mut fixture, claim, "AlreadyClaimed");
}

fn hook(fixture: &mut FactoryFixture) -> Result<(), lee::error::LeeError> {
    fixture.call(
        &key(49),
        factory_core::Message::ExecuteGraduationHook { now: 2 },
    )
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn settlement_manual_reserve_credit_retry_selects_route_only_after_payment() {
    let mut fixture = closed_sale(Some(ADAPTER), vec![], 100, true);
    for _ in 0..3 {
        manual(&mut fixture).unwrap();
    }
    let creator = id(&key(48));
    let definition = fixture.definition;
    set_holding(&mut fixture, creator, definition, COLLATERAL, 0);
    rejected(&mut fixture, manual, "WrongDefinition");
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::Burned {
            collateral_due: 112
        }
    );
    set_holding(&mut fixture, creator, definition, definition, 0);
    manual(&mut fixture).unwrap();
    assert_eq!(balance(&fixture.state, creator, definition), 100);
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::TokenPaid {
            collateral_due: 112
        }
    );
    rejected(&mut fixture, hook, "ManualPayoutStarted");
    manual(&mut fixture).unwrap();
    assert_eq!(balance(&fixture.state, creator, definition), 100);
    assert_eq!(balance(&fixture.state, creator, COLLATERAL), 1_112);
    assert_eq!(supply(&fixture), 250);
    rejected(&mut fixture, manual, "AlreadySettled");
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn settlement_manual_collateral_credit_failure_preserves_paid_reserve_without_double_payment() {
    let mut fixture = closed_sale(None, vec![], 100, true);
    for _ in 0..4 {
        manual(&mut fixture).unwrap();
    }
    let creator = id(&key(48));
    set_holding(&mut fixture, creator, COLLATERAL, LAUNCH, 1_000);
    rejected(&mut fixture, manual, "WrongDefinition");
    assert_eq!(balance(&fixture.state, creator, fixture.definition), 100);
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::TokenPaid {
            collateral_due: 112
        }
    );
    set_holding(&mut fixture, creator, COLLATERAL, COLLATERAL, 1_000);
    manual(&mut fixture).unwrap();
    assert_eq!(balance(&fixture.state, creator, fixture.definition), 100);
    assert_eq!(balance(&fixture.state, creator, COLLATERAL), 1_112);
    assert_eq!(balance(&fixture.state, fixture.factory, COLLATERAL), 0);
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::Complete
    );
    rejected(&mut fixture, manual, "AlreadySettled");
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

fn initialize_adapter(fixture: &mut FactoryFixture, ready: bool) {
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
            ready,
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
        .unwrap();
}

fn hook_ready(configuration: Vec<u8>, reserve: u128, buy: bool, elf: &str) -> FactoryFixture {
    let mut fixture = closed_sale(Some(ADAPTER), configuration, reserve, buy);
    fixture.state = fixture.state.with_named_programs([(ADAPTER, guest(elf))]);
    initialize_adapter(&mut fixture, true);
    for _ in 0..4 {
        hook(&mut fixture).unwrap();
    }
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::HookReady {
            collateral_due: if buy { 112 } else { 0 }
        }
    );
    fixture
}

#[test]
#[ignore = "requires fresh application, adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_permissionless_callback_records_graduated_without_dex_claim_and_rejects_replays() {
    let mut fixture = hook_ready(b"saved-route".to_vec(), 100, true, "LAUNCHPAD_ADAPTER_ELF");
    assert_eq!(
        balance(&fixture.state, custody(&fixture), fixture.definition),
        0
    );
    assert!(adapter_state(&fixture).receipt.is_none());
    hook(&mut fixture).unwrap();
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::Graduated
    );
    assert_eq!(
        balance(&fixture.state, custody(&fixture), fixture.definition),
        100
    );
    assert_eq!(balance(&fixture.state, custody(&fixture), COLLATERAL), 112);
    assert_eq!(balance(&fixture.state, id(&key(48)), fixture.definition), 0);
    assert_eq!(
        balance(
            &fixture.state,
            saved(&fixture).creator_escrow_id,
            fixture.definition
        ),
        50
    );
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 13);
    assert_eq!(supply(&fixture), 250);
    assert_eq!(
        adapter_state(&fixture).receipt,
        Some(factory_core::GraduationRequest {
            factory_program_id: FACTORY,
            namespace: fixture.namespace,
            launch_salt: SALT,
            token_definition_id: fixture.definition,
            collateral_definition_id: COLLATERAL,
            dex_seed_reserve: 100,
            collateral_amount: 112,
            configuration: b"saved-route".to_vec(),
        })
    );
    rejected(&mut fixture, hook, "AlreadySettled");
    rejected(&mut fixture, manual, "AlreadySettled");
    claim(&mut fixture).unwrap();
    assert_eq!(
        balance(&fixture.state, id(&key(48)), fixture.definition),
        50
    );
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::Graduated
    );
}

#[test]
#[ignore = "requires fresh application, adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_hook_reserve_credit_failure_rolls_back_and_retry_pays_once() {
    let mut fixture = hook_ready(vec![], 100, true, "LAUNCHPAD_ADAPTER_ELF");
    let adapter = custody(&fixture);
    let definition = fixture.definition;
    set_holding(&mut fixture, adapter, definition, COLLATERAL, 0);
    rejected(&mut fixture, hook, "WrongDefinition");
    assert!(adapter_state(&fixture).receipt.is_none());
    set_holding(&mut fixture, adapter, definition, definition, 0);
    hook(&mut fixture).unwrap();
    assert_eq!(balance(&fixture.state, adapter, definition), 100);
    assert_eq!(balance(&fixture.state, adapter, COLLATERAL), 112);
    assert_eq!(supply(&fixture), 250);
    rejected(&mut fixture, hook, "AlreadySettled");
}

#[test]
#[ignore = "requires fresh application, adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_hook_collateral_credit_failure_restores_prior_reserve_transfer() {
    let mut fixture = hook_ready(vec![], 100, true, "LAUNCHPAD_ADAPTER_ELF");
    let adapter = custody(&fixture);
    set_holding(&mut fixture, adapter, COLLATERAL, LAUNCH, 0);
    rejected(&mut fixture, hook, "WrongDefinition");
    assert_eq!(balance(&fixture.state, adapter, fixture.definition), 0);
    assert_eq!(
        balance(&fixture.state, fixture.factory, fixture.definition),
        100
    );
    assert!(adapter_state(&fixture).receipt.is_none());
    set_holding(&mut fixture, adapter, COLLATERAL, COLLATERAL, 0);
    hook(&mut fixture).unwrap();
    assert_eq!(balance(&fixture.state, adapter, fixture.definition), 100);
    assert_eq!(balance(&fixture.state, adapter, COLLATERAL), 112);
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::Graduated
    );
    assert_eq!(supply(&fixture), 250);
}

#[test]
#[ignore = "requires fresh application, adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_real_callback_failure_restores_both_payouts_and_retries_same_adapter() {
    let mut fixture = hook_ready(
        b"unchanged-route".to_vec(),
        100,
        true,
        "LAUNCHPAD_ADAPTER_ELF",
    );
    let valid = definition_state(&fixture);
    let mut invalid = valid.clone();
    let TokenState::Definition { kind, .. } = &mut invalid else {
        unreachable!()
    };
    *kind = TokenKind::NftMaster;
    set_definition(&mut fixture, invalid);
    rejected(&mut fixture, hook, "InvalidCustody");
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::HookReady {
            collateral_due: 112
        }
    );
    assert_eq!(
        balance(&fixture.state, custody(&fixture), fixture.definition),
        0
    );
    assert_eq!(balance(&fixture.state, custody(&fixture), COLLATERAL), 0);
    assert_eq!(
        balance(&fixture.state, fixture.factory, fixture.definition),
        100
    );
    assert_eq!(balance(&fixture.state, fixture.factory, COLLATERAL), 112);
    assert!(adapter_state(&fixture).receipt.is_none());
    set_definition(&mut fixture, valid);
    hook(&mut fixture).unwrap();
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::Graduated
    );
    assert_eq!(
        balance(&fixture.state, custody(&fixture), fixture.definition),
        100
    );
    assert_eq!(balance(&fixture.state, custody(&fixture), COLLATERAL), 112);
    assert_eq!(supply(&fixture), 250);
    assert_eq!(
        adapter_state(&fixture).receipt.unwrap().configuration,
        b"unchanged-route"
    );
}

#[test]
#[ignore = "requires fresh application, fault adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_final_ack_failure_rolls_back_adapter_receipt_payouts_and_graduated_marker() {
    let mut fixture = hook_ready(
        b"corrupt-final-ack".to_vec(),
        100,
        true,
        "LAUNCHPAD_GRADUATION_FAULT_ELF",
    );
    rejected(&mut fixture, hook, "InvalidState");
    assert!(adapter_state(&fixture).receipt.is_none());
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::HookReady {
            collateral_due: 112
        }
    );
    assert_eq!(
        balance(&fixture.state, custody(&fixture), fixture.definition),
        0
    );
    assert_eq!(balance(&fixture.state, custody(&fixture), COLLATERAL), 0);
    assert_eq!(
        balance(&fixture.state, fixture.factory, fixture.definition),
        100
    );
    assert_eq!(balance(&fixture.state, fixture.factory, COLLATERAL), 112);
    // Remove the test-only fault; this does not prove a production adapter upgrade route.
    fixture.state = fixture
        .state
        .with_named_programs([(ADAPTER, guest("LAUNCHPAD_ADAPTER_ELF"))]);
    hook(&mut fixture).unwrap();
    assert!(adapter_state(&fixture).receipt.is_some());
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::Graduated
    );
    assert_eq!(
        balance(&fixture.state, custody(&fixture), fixture.definition),
        100
    );
    assert_eq!(balance(&fixture.state, custody(&fixture), COLLATERAL), 112);
    assert_eq!(supply(&fixture), 250);
    rejected(&mut fixture, hook, "AlreadySettled");
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
        factory_core::Message::ExecuteGraduationHook { now: 2 },
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
                collateral_due: 112,
            }
        }
    );
    assert_eq!(adapter_state(fixture).receipt.is_some(), graduated);
    assert_eq!(
        balance(&fixture.state, adapter, fixture.definition),
        if graduated { 100 } else { 0 }
    );
    assert_eq!(
        balance(&fixture.state, adapter, COLLATERAL),
        if graduated { 112 } else { 0 }
    );
    assert_eq!(
        balance(&fixture.state, fixture.factory, fixture.definition),
        if graduated { 0 } else { 100 }
    );
    assert_eq!(
        balance(&fixture.state, fixture.factory, COLLATERAL),
        if graduated { 0 } else { 112 }
    );
    assert_eq!(balance(&fixture.state, id(&key(48)), fixture.definition), 0);
    assert_eq!(balance(&fixture.state, id(&key(48)), COLLATERAL), 1_000);
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
    assert_eq!(supply(fixture), 250);
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
        factory_core::Message::ExecuteGraduationHook { now: 2 },
        factory_core::Message::WithdrawFactoryProceeds {
            creator: id(&key(48)),
            now: 2,
        },
        factory_core::Message::ClaimCreatorAllocation {
            creator: id(&key(48)),
            now: 2,
        },
        factory_core::Message::ContinueCreation {
            creator: id(&key(48)),
            now: 2,
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
        nested_configuration(factory_actor, factory_core::GraduationResult { factory }),
        "InvalidMessage",
    ));
    for message in [
        curve_core::Message::ClosePool { now: 2 },
        curve_core::Message::WithdrawReserves { now: 2 },
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
        let mut fixture = hook_ready(
            configuration.clone(),
            100,
            true,
            "LAUNCHPAD_GRADUATION_FAULT_ELF",
        );
        // The creator signs so ambient signer access cannot explain the transaction failure.
        rejected(&mut fixture, creator_hook, reason);
        assert_hook_custody(&fixture, false);
        // Replace only test code, not the saved adapter identity or configuration.
        fixture.state = fixture
            .state
            .with_named_programs([(ADAPTER, guest("LAUNCHPAD_ADAPTER_ELF"))]);
        creator_hook(&mut fixture).unwrap();
        assert_hook_custody(&fixture, true);
        assert_eq!(
            adapter_state(&fixture).receipt.unwrap().configuration,
            configuration
        );
        rejected(&mut fixture, creator_hook, "AlreadySettled");
        rejected(&mut fixture, manual, "AlreadySettled");
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
    let mut fixture = hook_ready(configuration, 100, true, "LAUNCHPAD_GRADUATION_FAULT_ELF");
    assert_hook_custody(&fixture, false);
    creator_hook(&mut fixture).unwrap();
    assert_hook_custody(&fixture, true);
    assert_eq!(pool_state(&fixture).pool.fees_accrued, 0);
    rejected(&mut fixture, creator_hook, "AlreadySettled");
    assert_hook_custody(&fixture, true);
}

#[test]
#[ignore = "requires fresh application, fault adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_failed_callback_allows_manual_recovery_without_replacing_adapter() {
    let mut fixture = hook_ready(
        b"reject-callback".to_vec(),
        100,
        true,
        "LAUNCHPAD_GRADUATION_FAULT_ELF",
    );
    rejected(&mut fixture, hook, "callback rejected after transfers");
    assert!(adapter_state(&fixture).receipt.is_none());
    assert_eq!(
        balance(&fixture.state, custody(&fixture), fixture.definition),
        0
    );
    assert_eq!(balance(&fixture.state, custody(&fixture), COLLATERAL), 0);
    manual(&mut fixture).unwrap();
    assert_eq!(
        balance(&fixture.state, id(&key(48)), fixture.definition),
        100
    );
    rejected(&mut fixture, hook, "ManualPayoutStarted");
    manual(&mut fixture).unwrap();
    assert_eq!(balance(&fixture.state, id(&key(48)), COLLATERAL), 1_112);
    assert_eq!(
        balance(&fixture.state, custody(&fixture), fixture.definition),
        0
    );
    assert_eq!(balance(&fixture.state, custody(&fixture), COLLATERAL), 0);
    assert!(adapter_state(&fixture).receipt.is_none());
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::Complete
    );
    assert_eq!(supply(&fixture), 250);
    rejected(&mut fixture, manual, "AlreadySettled");
    rejected(&mut fixture, hook, "ManualPayoutStarted");
}

#[test]
#[ignore = "requires fresh application, adapter and compatibility ELFs and the pinned clock ELF"]
fn settlement_zero_manual_payout_and_allocation_claim_leave_one_permissionless_hook() {
    let mut fixture = closed_sale(Some(ADAPTER), b"zero-payout".to_vec(), 0, false);
    fixture.state = fixture
        .state
        .with_named_programs([(ADAPTER, guest("LAUNCHPAD_ADAPTER_ELF"))]);
    initialize_adapter(&mut fixture, true);
    for _ in 0..4 {
        manual(&mut fixture).unwrap();
    }
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::NoPayout
    );
    assert_eq!(supply(&fixture), 50);
    claim(&mut fixture).unwrap();
    assert_eq!(
        balance(&fixture.state, id(&key(48)), fixture.definition),
        50
    );
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::NoPayout
    );
    hook(&mut fixture).unwrap();
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::HookReady { collateral_due: 0 }
    );
    hook(&mut fixture).unwrap();
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::Graduated
    );
    let receipt = adapter_state(&fixture).receipt.unwrap();
    assert_eq!(receipt.dex_seed_reserve, 0);
    assert_eq!(receipt.collateral_amount, 0);
    assert_eq!(receipt.configuration, b"zero-payout");
    assert_eq!(
        balance(&fixture.state, custody(&fixture), fixture.definition),
        0
    );
    assert_eq!(balance(&fixture.state, custody(&fixture), COLLATERAL), 0);
    assert_eq!(
        balance(&fixture.state, id(&key(48)), fixture.definition),
        50
    );
    assert_eq!(balance(&fixture.state, id(&key(48)), COLLATERAL), 1_000);
    assert_eq!(supply(&fixture), 50);
    rejected(&mut fixture, hook, "AlreadySettled");
    rejected(&mut fixture, claim, "AlreadyClaimed");
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn settlement_while_frozen_keeps_creator_checks_and_completes_fees_burn_and_payouts() {
    let mut fixture = closed_sale(None, vec![], 100, true);
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
    for _ in 0..5 {
        rejected(
            &mut fixture,
            |fixture| {
                fixture.call(
                    &key(49),
                    factory_core::Message::WithdrawFactoryProceeds {
                        creator: id(&key(49)),
                        now: 2,
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
        150
    );
    assert_eq!(balance(&fixture.state, id(&key(48)), COLLATERAL), 1_112);
    assert_eq!(balance(&fixture.state, fixture.pool, fixture.definition), 0);
    assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 0);
    assert_eq!(supply(&fixture), 250);
}

fn manual(fixture: &mut FactoryFixture) -> Result<(), lee::error::LeeError> {
    fixture.call(
        &key(48),
        factory_core::Message::WithdrawFactoryProceeds {
            creator: id(&key(48)),
            now: 2,
        },
    )
}

fn rejected(
    fixture: &mut FactoryFixture,
    operation: impl FnOnce(&mut FactoryFixture) -> Result<(), lee::error::LeeError>,
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

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn settlement_reserve0_credit_failure_restores_fees_and_reserves() {
    let mut fixture = closed_sale(None, vec![], 100, true);
    manual(&mut fixture).unwrap();
    let factory = fixture.factory;
    let definition = fixture.definition;
    set_holding(&mut fixture, factory, definition, definition, u128::MAX);
    rejected(&mut fixture, manual, "Overflow");
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 0);
    assert_eq!(balance(&fixture.state, fixture.pool, definition), 300);
    set_holding(&mut fixture, factory, definition, definition, 100);
    manual(&mut fixture).unwrap();
    assert_eq!(balance(&fixture.state, factory, definition), 400);
    assert_eq!(balance(&fixture.state, factory, COLLATERAL), 112);
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 13);
    assert_eq!(balance(&fixture.state, fixture.pool, definition), 0);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn settlement_reserve1_credit_failure_restores_prior_reserve0_and_fee_payments() {
    let mut fixture = closed_sale(None, vec![], 100, true);
    manual(&mut fixture).unwrap();
    let factory = fixture.factory;
    set_holding(&mut fixture, factory, COLLATERAL, COLLATERAL, u128::MAX);
    rejected(&mut fixture, manual, "Overflow");
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 0);
    assert_eq!(
        balance(&fixture.state, fixture.pool, fixture.definition),
        300
    );
    assert_eq!(balance(&fixture.state, factory, fixture.definition), 100);
    set_holding(&mut fixture, factory, COLLATERAL, COLLATERAL, 0);
    manual(&mut fixture).unwrap();
    assert_eq!(balance(&fixture.state, factory, fixture.definition), 400);
    assert_eq!(balance(&fixture.state, factory, COLLATERAL), 112);
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 13);
    assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 0);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn settlement_fee_transfer_failure_rolls_back_and_retry_collects_once() {
    let mut fixture = closed_sale(None, vec![], 100, true);
    manual(&mut fixture).unwrap();
    assert_eq!(
        saved(&fixture).settlement_stage,
        factory_core::SettlementStage::Ready
    );
    set_holding(&mut fixture, TREASURY, COLLATERAL, LAUNCH, 0);
    rejected(&mut fixture, manual, "WrongDefinition");
    assert_eq!(pool_state(&fixture).pool.fees_accrued, 13);
    set_holding(&mut fixture, TREASURY, COLLATERAL, COLLATERAL, 0);
    manual(&mut fixture).unwrap();
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
