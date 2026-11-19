use super::*;

fn namespace_config(state: &V03State, namespace: AccountId) -> curve_core::Config {
    curve_core::Config::decode(
        state
            .get_account_by_id(curve_core::compute_config_pda(namespace, CURVE))
            .data
            .actor_state(CURVE),
    )
    .unwrap()
}

fn admin_call(
    state: &mut V03State,
    actors: &[Actor],
    signer: &PrivateKey,
    message: curve_core::Message,
) -> Result<(), lee::error::LeeError> {
    execute(
        state,
        Actor::new(id(signer), CURVE),
        actors.to_vec(),
        message,
        &[signer],
    )
}

fn emergency_key() -> PrivateKey {
    let emergency = key(52);
    assert_eq!(
        id(&emergency).value(),
        curve_core::INITIAL_EMERGENCY_AUTHORITY.value()
    );
    emergency
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn namespace_admin_updates_fee_and_treasury_with_validation_and_authority_checks() {
    let namespace = key(60);
    let outsider = key(61);
    let namespace_id = id(&namespace);
    let (mut state, actors) = base_state(&namespace, &[id(&outsider)]);
    for message in [
        curve_core::Message::UpdateFee {
            namespace: namespace_id,
            protocol_fee_bps: 250,
        },
        curve_core::Message::UpdateTreasury {
            namespace: namespace_id,
            treasury: id(&outsider),
        },
    ] {
        rejected_without_changes(
            &mut state,
            |state| admin_call(state, &actors, &outsider, message.clone()),
            "Unauthorized",
        );
        admin_call(&mut state, &actors, &namespace, message).unwrap();
    }
    let config = namespace_config(&state, namespace_id);
    assert_eq!(config.protocol_fee_bps, 250);
    assert_eq!(config.treasury, id(&outsider));
    for (message, reason) in [
        (
            curve_core::Message::UpdateFee {
                namespace: namespace_id,
                protocol_fee_bps: 10_000,
            },
            "InvalidFee",
        ),
        (
            curve_core::Message::UpdateTreasury {
                namespace: namespace_id,
                treasury: AccountId::default(),
            },
            "InvalidTreasury",
        ),
    ] {
        rejected_without_changes(
            &mut state,
            |state| admin_call(state, &actors, &namespace, message),
            reason,
        );
    }
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn signer_admin_transfer_requires_consent_and_renunciation_keeps_emergency_and_other_namespace() {
    let namespace = key(60);
    let successor = key(61);
    let other = key(62);
    let emergency = emergency_key();
    let namespace_id = id(&namespace);
    let (mut state, mut actors) =
        base_state(&namespace, &[id(&successor), id(&other), id(&emergency)]);
    actors.extend([
        Actor::new(id(&other), CURVE),
        Actor::new(id(&other), AUTHORITY),
        Actor::new(curve_core::compute_config_pda(id(&other), CURVE), CURVE),
        Actor::new(curve_core::compute_admin_role(id(&other), CURVE), AUTHORITY),
    ]);
    admin_call(
        &mut state,
        &actors,
        &other,
        curve_core::Message::InitializeNamespace {
            namespace: id(&other),
            protocol_fee_bps: 50,
            treasury: TREASURY,
        },
    )
    .unwrap();
    admin_call(
        &mut state,
        &actors,
        &emergency,
        curve_core::Message::SetFrozen {
            namespace: namespace_id,
            frozen: true,
        },
    )
    .unwrap();
    let transfer = curve_core::Message::TransferAdmin {
        namespace: namespace_id,
        new_admin: id(&successor),
        candidate: curve_core::AdminCandidate::Signer,
    };
    state = state.with_public_accounts([account(Actor::new(id(&successor), AUTHORITY), vec![])]);
    rejected_without_changes(
        &mut state,
        |state| admin_call(state, &actors, &namespace, transfer.clone()),
        "Unauthorized",
    );
    execute(
        &mut state,
        Actor::new(namespace_id, CURVE),
        actors.clone(),
        transfer,
        &[&namespace, &successor],
    )
    .unwrap();
    for signer in [&namespace, &other] {
        rejected_without_changes(
            &mut state,
            |state| {
                admin_call(
                    state,
                    &actors,
                    signer,
                    curve_core::Message::UpdateFee {
                        namespace: namespace_id,
                        protocol_fee_bps: 100,
                    },
                )
            },
            "Unauthorized",
        );
    }
    admin_call(
        &mut state,
        &actors,
        &successor,
        curve_core::Message::UpdateFee {
            namespace: namespace_id,
            protocol_fee_bps: 100,
        },
    )
    .unwrap();
    admin_call(
        &mut state,
        &actors,
        &successor,
        curve_core::Message::RenounceAdmin {
            namespace: namespace_id,
        },
    )
    .unwrap();
    for signer in [&namespace, &successor, &emergency] {
        rejected_without_changes(
            &mut state,
            |state| {
                admin_call(
                    state,
                    &actors,
                    signer,
                    curve_core::Message::UpdateTreasury {
                        namespace: namespace_id,
                        treasury: id(&successor),
                    },
                )
            },
            "Renounced",
        );
    }
    let config = namespace_config(&state, namespace_id);
    assert_eq!(config.protocol_fee_bps, 100);
    assert_eq!(config.treasury, TREASURY);
    assert!(config.emergency.is_frozen);
    assert_eq!(config.emergency.authority(), id(&emergency));
    let other_config = namespace_config(&state, id(&other));
    assert_eq!(other_config.protocol_fee_bps, 50);
    assert!(!other_config.emergency.is_frozen);
    admin_call(
        &mut state,
        &actors,
        &other,
        curve_core::Message::UpdateFee {
            namespace: id(&other),
            protocol_fee_bps: 200,
        },
    )
    .unwrap();
    admin_call(
        &mut state,
        &actors,
        &emergency,
        curve_core::Message::SetFrozen {
            namespace: namespace_id,
            frozen: false,
        },
    )
    .unwrap();
    assert!(!namespace_config(&state, namespace_id).emergency.is_frozen);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn emergency_transfer_preserves_freeze_and_all_noop_calls_require_current_authority() {
    let namespace = key(60);
    let successor = key(61);
    let emergency = emergency_key();
    let ns = id(&namespace);
    let (mut state, actors) = base_state(&namespace, &[id(&successor), id(&emergency)]);
    assert!(!namespace_config(&state, ns).emergency.is_frozen);
    for frozen in [false, true, true] {
        rejected_without_changes(
            &mut state,
            |state| {
                admin_call(
                    state,
                    &actors,
                    &namespace,
                    curve_core::Message::SetFrozen {
                        namespace: ns,
                        frozen,
                    },
                )
            },
            "Unauthorized",
        );
        admin_call(
            &mut state,
            &actors,
            &emergency,
            curve_core::Message::SetFrozen {
                namespace: ns,
                frozen,
            },
        )
        .unwrap();
    }
    for new_authority in [id(&emergency), id(&successor)] {
        rejected_without_changes(
            &mut state,
            |state| {
                admin_call(
                    state,
                    &actors,
                    &namespace,
                    curve_core::Message::TransferEmergencyAuthority {
                        namespace: ns,
                        new_authority,
                    },
                )
            },
            "Unauthorized",
        );
        admin_call(
            &mut state,
            &actors,
            &emergency,
            curve_core::Message::TransferEmergencyAuthority {
                namespace: ns,
                new_authority,
            },
        )
        .unwrap();
    }
    let config = namespace_config(&state, ns);
    assert!(config.emergency.is_frozen);
    assert_eq!(config.emergency.authority(), id(&successor));
    for message in [
        curve_core::Message::SetFrozen {
            namespace: ns,
            frozen: true,
        },
        curve_core::Message::TransferEmergencyAuthority {
            namespace: ns,
            new_authority: id(&successor),
        },
    ] {
        rejected_without_changes(
            &mut state,
            |state| admin_call(state, &actors, &emergency, message),
            "Unauthorized",
        );
    }
    rejected_without_changes(
        &mut state,
        |state| {
            admin_call(
                state,
                &actors,
                &successor,
                curve_core::Message::TransferEmergencyAuthority {
                    namespace: ns,
                    new_authority: AccountId::default(),
                },
            )
        },
        "Unauthorized",
    );
    admin_call(
        &mut state,
        &actors,
        &successor,
        curve_core::Message::TransferEmergencyAuthority {
            namespace: ns,
            new_authority: id(&successor),
        },
    )
    .unwrap();
    for frozen in [false, false] {
        admin_call(
            &mut state,
            &actors,
            &successor,
            curve_core::Message::SetFrozen {
                namespace: ns,
                frozen,
            },
        )
        .unwrap();
    }
    assert!(!namespace_config(&state, ns).emergency.is_frozen);
}

fn fresh_sale(trade_mode: u8, end_timestamp: Option<u64>) -> FactoryFixture {
    sale_with_pause(trade_mode, end_timestamp, curve_core::PausePolicy::Disabled)
}

fn sale_with_pause(
    trade_mode: u8,
    end_timestamp: Option<u64>,
    pause_policy: curve_core::PausePolicy,
) -> FactoryFixture {
    let mut fixture = factory_fixture();
    let creator = key(48);
    fixture.actors.extend([
        Actor::new(TREASURY, TOKEN),
        holding(TREASURY, COLLATERAL),
        Actor::new(id(&key(52)), CURVE),
    ]);
    fixture
        .call(
            &creator,
            factory_core::Message::CreateFactoryPool {
                buy_policy: curve_core::BuyPolicy::Open,
                pause_policy,
                ending_policy: if end_timestamp.is_some() {
                    curve_core::EndingPolicy::Either
                } else {
                    curve_core::EndingPolicy::Depletion
                },
                trade_mode,
                sell_fee_bps: 0,
                graduation_hook_program_id: None,
                graduation_hook_configuration: vec![],
                namespace: fixture.namespace,
                launch_salt: SALT,
                name: "Controls sale".into(),
                uri: "ipfs://controls".into(),
                graduation_reserve: 0,
                sale_reserve: 400,
                graduation_policy: launchpad_client::GraduationPolicy::default(),
                creator_allocation: 50,
                virtual_token_reserve: 1_000,
                virtual_collateral_reserve: 1_000,
                end_timestamp,
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
    execute(
        &mut fixture.state,
        Actor::new(fixture.definition, TOKEN),
        fixture.actors.clone(),
        token::Message::Initialize {
            owner: id(&key(49)),
        },
        &[],
    )
    .unwrap();
    execute(
        &mut fixture.state,
        Actor::new(COLLATERAL, TOKEN),
        fixture.actors.clone(),
        token::Message::Initialize { owner: TREASURY },
        &[],
    )
    .unwrap();
    fixture
}

fn saved_pool(fixture: &FactoryFixture) -> curve_core::PoolAccount {
    curve_core::PoolAccount::decode(
        fixture
            .state
            .get_account_by_id(fixture.pool)
            .data
            .actor_state(CURVE),
    )
    .unwrap()
}

fn trade(
    fixture: &mut FactoryFixture,
    message: curve_core::Message,
) -> Result<(), lee::error::LeeError> {
    curve_call_at(
        &mut fixture.state,
        &fixture.actors,
        fixture.pool,
        &key(49),
        message,
        fixture.inclusion_timestamp_ms,
    )
}

fn collect(fixture: &mut FactoryFixture) -> Result<(), lee::error::LeeError> {
    execute(
        &mut fixture.state,
        Actor::new(fixture.pool, CURVE),
        fixture.actors.clone(),
        curve_core::Message::CollectFees,
        &[],
    )
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn fresh_factory_sale_modes_apply_live_fees_and_collect_all_to_current_treasury_once() {
    for mode in [0, 1] {
        let mut fixture = fresh_sale(mode, None);
        admin_call(
            &mut fixture.state,
            &fixture.actors,
            &key(47),
            curve_core::Message::UpdateFee {
                namespace: fixture.namespace,
                protocol_fee_bps: 1_000,
            },
        )
        .unwrap();
        trade(
            &mut fixture,
            curve_core::Message::SwapExactInput {
                membership: None,
                amount_in: 100,
                min_amount_out: 82,
                token_in: COLLATERAL,
                now: 1,
            },
        )
        .unwrap();
        assert_eq!(balance(&fixture.state, id(&key(49)), COLLATERAL), 900);
        assert_eq!(
            balance(&fixture.state, id(&key(49)), fixture.definition),
            82
        );
        assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 100);
        assert_eq!(saved_pool(&fixture).pool.real_reserve0, 318);
        assert_eq!(saved_pool(&fixture).pool.real_reserve1, 90);
        assert_eq!(saved_pool(&fixture).pool.fees_accrued, 10);
        admin_call(
            &mut fixture.state,
            &fixture.actors,
            &key(47),
            curve_core::Message::UpdateFee {
                namespace: fixture.namespace,
                protocol_fee_bps: 2_000,
            },
        )
        .unwrap();
        let sell = curve_core::Message::SwapExactInput {
            membership: None,
            amount_in: 82,
            min_amount_out: 72,
            token_in: fixture.definition,
            now: 1,
        };
        if mode == 0 {
            rejected_without_changes(
                &mut fixture.state,
                |state| curve_call(state, &fixture.actors, fixture.pool, &key(49), sell),
                "Restricted",
            );
        } else {
            trade(&mut fixture, sell).unwrap();
            assert_eq!(balance(&fixture.state, id(&key(49)), COLLATERAL), 972);
            assert_eq!(balance(&fixture.state, id(&key(49)), fixture.definition), 0);
            assert_eq!(saved_pool(&fixture).pool.real_reserve0, 400);
            assert_eq!(saved_pool(&fixture).pool.real_reserve1, 0);
            assert_eq!(saved_pool(&fixture).pool.fees_accrued, 28);
        }
        admin_call(
            &mut fixture.state,
            &fixture.actors,
            &key(47),
            curve_core::Message::UpdateTreasury {
                namespace: fixture.namespace,
                treasury: id(&key(48)),
            },
        )
        .unwrap();
        for _ in 0..2 {
            collect(&mut fixture).unwrap();
            assert_eq!(saved_pool(&fixture).pool.fees_accrued, 0);
            assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 0);
            assert_eq!(
                balance(&fixture.state, id(&key(48)), COLLATERAL),
                if mode == 0 { 1_010 } else { 1_028 }
            );
            assert_eq!(
                balance(&fixture.state, fixture.pool, COLLATERAL),
                if mode == 0 { 90 } else { 0 }
            );
        }
    }
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn failed_swap_and_fee_payout_restore_full_runtime_state_before_successful_retry() {
    let mut fixture = fresh_sale(1, None);
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &key(47),
        curve_core::Message::UpdateFee {
            namespace: fixture.namespace,
            protocol_fee_bps: 1_000,
        },
    )
    .unwrap();
    let valid = fixture.state.clone();
    fixture.state = fixture.state.clone().with_public_accounts([account(
        holding(id(&key(49)), COLLATERAL),
        borsh::to_vec(&TokenState::Holding {
            definition: COLLATERAL,
            owner: id(&key(49)),
            kind: TokenKind::Fungible,
            balance: 0,
        })
        .unwrap(),
    )]);
    rejected_without_changes(
        &mut fixture.state,
        |state| {
            curve_call(
                state,
                &fixture.actors,
                fixture.pool,
                &key(49),
                curve_core::Message::SwapExactInput {
                    membership: None,
                    amount_in: 100,
                    min_amount_out: 82,
                    token_in: COLLATERAL,
                    now: 1,
                },
            )
        },
        "InsufficientBalance",
    );
    fixture.state = valid;
    trade(
        &mut fixture,
        curve_core::Message::SwapExactInput {
            membership: None,
            amount_in: 100,
            min_amount_out: 82,
            token_in: COLLATERAL,
            now: 1,
        },
    )
    .unwrap();
    let valid = fixture.state.clone();
    fixture.state = fixture.state.clone().with_public_accounts([account(
        holding(TREASURY, COLLATERAL),
        borsh::to_vec(&TokenState::Holding {
            definition: LAUNCH,
            owner: TREASURY,
            kind: TokenKind::Fungible,
            balance: 0,
        })
        .unwrap(),
    )]);
    rejected_without_changes(
        &mut fixture.state,
        |state| {
            execute(
                state,
                Actor::new(fixture.pool, CURVE),
                fixture.actors.clone(),
                curve_core::Message::CollectFees,
                &[],
            )
        },
        "WrongDefinition",
    );
    fixture.state = valid;
    collect(&mut fixture).unwrap();
    assert_eq!(balance(&fixture.state, id(&key(49)), COLLATERAL), 900);
    assert_eq!(
        balance(&fixture.state, id(&key(49)), fixture.definition),
        82
    );
    assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 90);
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 10);
    assert_eq!(saved_pool(&fixture).pool.fees_accrued, 0);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn frozen_factory_sales_block_buys_and_sells_but_keep_admin_roles_and_fee_collection() {
    for mode in [0, 1] {
        let mut fixture = fresh_sale(mode, None);
        let emergency = emergency_key();
        admin_call(
            &mut fixture.state,
            &fixture.actors,
            &key(47),
            curve_core::Message::UpdateFee {
                namespace: fixture.namespace,
                protocol_fee_bps: 1_000,
            },
        )
        .unwrap();
        trade(
            &mut fixture,
            curve_core::Message::SwapExactInput {
                membership: None,
                amount_in: 100,
                min_amount_out: 82,
                token_in: COLLATERAL,
                now: 1,
            },
        )
        .unwrap();
        admin_call(
            &mut fixture.state,
            &fixture.actors,
            &emergency,
            curve_core::Message::SetFrozen {
                namespace: fixture.namespace,
                frozen: true,
            },
        )
        .unwrap();
        for message in [
            curve_core::Message::SwapExactInput {
                membership: None,
                amount_in: 100,
                min_amount_out: 1,
                token_in: COLLATERAL,
                now: 1,
            },
            curve_core::Message::SwapExactInput {
                membership: None,
                amount_in: 82,
                min_amount_out: 1,
                token_in: fixture.definition,
                now: 1,
            },
            curve_core::Message::SwapExactOutput {
                membership: None,
                amount_out: 1,
                max_amount_in: 100,
                token_in: COLLATERAL,
                now: 1,
            },
        ] {
            rejected_without_changes(
                &mut fixture.state,
                |state| curve_call(state, &fixture.actors, fixture.pool, &key(49), message),
                "Frozen",
            );
        }
        for message in [
            curve_core::Message::UpdateFee {
                namespace: fixture.namespace,
                protocol_fee_bps: 2_000,
            },
            curve_core::Message::UpdateTreasury {
                namespace: fixture.namespace,
                treasury: id(&key(48)),
            },
        ] {
            admin_call(&mut fixture.state, &fixture.actors, &key(47), message).unwrap();
        }
        execute(
            &mut fixture.state,
            Actor::new(id(&key(48)), AUTHORITY),
            fixture.actors.clone(),
            authority::Message::Transfer {
                role: factory_core::compute_curve_creator_role_pda(
                    fixture.namespace,
                    FACTORY,
                    SALT,
                ),
                successor: id(&key(49)),
            },
            &[&key(48), &key(49)],
        )
        .unwrap();
        collect(&mut fixture).unwrap();
        assert_eq!(balance(&fixture.state, id(&key(48)), COLLATERAL), 1_010);
        assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 90);
        assert_eq!(saved_pool(&fixture).pool.fees_accrued, 0);
        rejected_without_changes(
            &mut fixture.state,
            |state| {
                curve_call(
                    state,
                    &fixture.actors,
                    fixture.pool,
                    &key(49),
                    curve_core::Message::ClosePool { now: 1 },
                )
            },
            "Unauthorized",
        );
        admin_call(
            &mut fixture.state,
            &fixture.actors,
            &emergency,
            curve_core::Message::SetFrozen {
                namespace: fixture.namespace,
                frozen: false,
            },
        )
        .unwrap();
        if mode == 0 {
            trade(
                &mut fixture,
                curve_core::Message::SwapExactInput {
                    membership: None,
                    amount_in: 100,
                    min_amount_out: 63,
                    token_in: COLLATERAL,
                    now: 1,
                },
            )
            .unwrap();
            assert_eq!(
                balance(&fixture.state, id(&key(49)), fixture.definition),
                145
            );
            assert_eq!(balance(&fixture.state, id(&key(49)), COLLATERAL), 800);
        } else {
            let definition = fixture.definition;
            trade(
                &mut fixture,
                curve_core::Message::SwapExactInput {
                    membership: None,
                    amount_in: 82,
                    min_amount_out: 72,
                    token_in: definition,
                    now: 1,
                },
            )
            .unwrap();
            assert_eq!(balance(&fixture.state, id(&key(49)), fixture.definition), 0);
            assert_eq!(balance(&fixture.state, id(&key(49)), COLLATERAL), 972);
        }
    }
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn buying_all_factory_d_closes_both_sale_modes_and_rejects_later_trades() {
    for mode in [0, 1] {
        let mut fixture = fresh_sale(mode, None);
        rejected_without_changes(
            &mut fixture.state,
            |state| {
                curve_call(
                    state,
                    &fixture.actors,
                    fixture.pool,
                    &key(48),
                    curve_core::Message::ClosePool { now: 1 },
                )
            },
            "Unauthorized",
        );
        trade(
            &mut fixture,
            curve_core::Message::SwapExactOutput {
                membership: None,
                amount_out: 400,
                max_amount_in: 667,
                token_in: COLLATERAL,
                now: 1,
            },
        )
        .unwrap();
        assert_eq!(
            saved_pool(&fixture).pool.lifecycle,
            pool::PoolLifecycle::Closed
        );
        assert_eq!(
            balance(&fixture.state, id(&key(49)), fixture.definition),
            400
        );
        assert_eq!(balance(&fixture.state, id(&key(49)), COLLATERAL), 333);
        assert_eq!(balance(&fixture.state, fixture.pool, fixture.definition), 0);
        assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 667);
        assert_eq!(
            balance(&fixture.state, fixture.factory, fixture.definition),
            0
        );
        rejected_without_changes(
            &mut fixture.state,
            |state| {
                curve_call(
                    state,
                    &fixture.actors,
                    fixture.pool,
                    &key(49),
                    curve_core::Message::SwapExactInput {
                        membership: None,
                        amount_in: 1,
                        min_amount_out: 0,
                        token_in: COLLATERAL,
                        now: 1,
                    },
                )
            },
            "InvalidPool",
        );
        if mode == 1 {
            rejected_without_changes(
                &mut fixture.state,
                |state| {
                    curve_call(
                        state,
                        &fixture.actors,
                        fixture.pool,
                        &key(49),
                        curve_core::Message::SwapExactInput {
                            membership: None,
                            amount_in: 1,
                            min_amount_out: 0,
                            token_in: fixture.definition,
                            now: 1,
                        },
                    )
                },
                "InvalidPool",
            );
        }
    }
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn namespace_freeze_and_emergency_transfer_do_not_control_another_namespace_pool() {
    let mut fixture = fresh_sale(1, None);
    let other = key(63);
    let owner = key(49);
    let emergency = emergency_key();
    let other_id = id(&other);
    let other_pool = curve_core::compute_pool_pda(other_id, CURVE, LAUNCH, COLLATERAL, id(&owner));
    fixture.actors.extend([
        Actor::new(other_id, CURVE),
        Actor::new(other_id, AUTHORITY),
        Actor::new(curve_core::compute_config_pda(other_id, CURVE), CURVE),
        Actor::new(curve_core::compute_admin_role(other_id, CURVE), AUTHORITY),
        Actor::new(other_pool, CURVE),
        Actor::new(other_pool, TOKEN),
        Actor::new(
            curve_core::compute_pool_owner_role(other_id, other_pool, CURVE),
            AUTHORITY,
        ),
        holding(other_pool, LAUNCH),
        holding(other_pool, COLLATERAL),
    ]);
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &other,
        curve_core::Message::InitializeNamespace {
            namespace: other_id,
            protocol_fee_bps: 0,
            treasury: TREASURY,
        },
    )
    .unwrap();
    curve_call(
        &mut fixture.state,
        &fixture.actors,
        other_pool,
        &owner,
        curve_core::Message::CreatePool {
            buy_policy: curve_core::BuyPolicy::Open,
            pause_policy: curve_core::PausePolicy::Disabled,
            ending_policy: curve_core::EndingPolicy::Manual,
            sell_fee_bps: 0,
            namespace: other_id,
            token0_definition_id: LAUNCH,
            token1_definition_id: COLLATERAL,
            allowed_input_token: None,
            defer_funding: false,
            token0_amount: 400,
            token1_amount: 100,
            virtual_reserve0: 1_000,
            virtual_reserve1: 1_000,
            close_timestamp: None,
            close_on_depletion: None,
            owner: id(&owner),
            owner_program_id: None,
            owner_program_seed: None,
            curve_program_id: CURVE,
            now: 1,
        },
    )
    .unwrap();
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &emergency,
        curve_core::Message::SetFrozen {
            namespace: fixture.namespace,
            frozen: true,
        },
    )
    .unwrap();
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &emergency,
        curve_core::Message::TransferEmergencyAuthority {
            namespace: fixture.namespace,
            new_authority: id(&owner),
        },
    )
    .unwrap();
    rejected_without_changes(
        &mut fixture.state,
        |state| {
            admin_call(
                state,
                &fixture.actors,
                &owner,
                curve_core::Message::SetFrozen {
                    namespace: other_id,
                    frozen: true,
                },
            )
        },
        "Unauthorized",
    );
    rejected_without_changes(
        &mut fixture.state,
        |state| {
            admin_call(
                state,
                &fixture.actors,
                &other,
                curve_core::Message::UpdateFee {
                    namespace: fixture.namespace,
                    protocol_fee_bps: 100,
                },
            )
        },
        "Unauthorized",
    );
    assert!(
        namespace_config(&fixture.state, fixture.namespace)
            .emergency
            .is_frozen
    );
    assert!(
        !namespace_config(&fixture.state, other_id)
            .emergency
            .is_frozen
    );
    assert_eq!(
        namespace_config(&fixture.state, other_id)
            .emergency
            .authority(),
        id(&emergency)
    );
    curve_call(
        &mut fixture.state,
        &fixture.actors,
        other_pool,
        &owner,
        curve_core::Message::SwapExactInput {
            membership: None,
            amount_in: 100,
            min_amount_out: 90,
            token_in: COLLATERAL,
            now: 1,
        },
    )
    .unwrap();
    assert_eq!(balance(&fixture.state, other_pool, LAUNCH), 310);
    assert_eq!(balance(&fixture.state, other_pool, COLLATERAL), 200);
    assert_eq!(balance(&fixture.state, id(&owner), LAUNCH), 690);
    assert_eq!(balance(&fixture.state, id(&owner), COLLATERAL), 800);
    assert_eq!(
        balance(&fixture.state, fixture.pool, fixture.definition),
        400
    );
    assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 0);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn pool_treasury_alias_collects_fees_without_self_transfer_or_reserve_change() {
    let mut fixture = fresh_sale(1, None);
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &key(47),
        curve_core::Message::UpdateFee {
            namespace: fixture.namespace,
            protocol_fee_bps: 1_000,
        },
    )
    .unwrap();
    trade(
        &mut fixture,
        curve_core::Message::SwapExactInput {
            membership: None,
            amount_in: 100,
            min_amount_out: 82,
            token_in: COLLATERAL,
            now: 1,
        },
    )
    .unwrap();
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &key(47),
        curve_core::Message::UpdateTreasury {
            namespace: fixture.namespace,
            treasury: fixture.pool,
        },
    )
    .unwrap();
    collect(&mut fixture).unwrap();
    assert_eq!(saved_pool(&fixture).pool.fees_accrued, 0);
    assert_eq!(saved_pool(&fixture).pool.real_reserve1, 90);
    assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 100);
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 0);
    let collected = fixture.state.clone();
    collect(&mut fixture).unwrap();
    assert!(
        fixture.state == collected,
        "zero-fee retry changed runtime state"
    );
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn frozen_direct_pool_keeps_owner_close_and_reserve_recovery_checks() {
    let namespace = key(60);
    let owner = key(61);
    let emergency = emergency_key();
    let ns = id(&namespace);
    let (mut state, mut actors) = base_state(&namespace, &[id(&owner), id(&emergency)]);
    let pool_id = curve_core::compute_pool_pda(ns, CURVE, LAUNCH, COLLATERAL, id(&owner));
    actors.extend([
        Actor::new(pool_id, CURVE),
        Actor::new(pool_id, TOKEN),
        Actor::new(
            curve_core::compute_pool_owner_role(ns, pool_id, CURVE),
            AUTHORITY,
        ),
        holding(pool_id, LAUNCH),
        holding(pool_id, COLLATERAL),
    ]);
    curve_call(
        &mut state,
        &actors,
        pool_id,
        &owner,
        curve_core::Message::CreatePool {
            buy_policy: curve_core::BuyPolicy::Open,
            pause_policy: curve_core::PausePolicy::Disabled,
            ending_policy: curve_core::EndingPolicy::Manual,
            sell_fee_bps: 0,
            namespace: ns,
            token0_definition_id: LAUNCH,
            token1_definition_id: COLLATERAL,
            allowed_input_token: None,
            defer_funding: false,
            token0_amount: 400,
            token1_amount: 100,
            virtual_reserve0: 1_000,
            virtual_reserve1: 1_000,
            close_timestamp: None,
            close_on_depletion: None,
            owner: id(&owner),
            owner_program_id: None,
            owner_program_seed: None,
            curve_program_id: CURVE,
            now: 1,
        },
    )
    .unwrap();
    admin_call(
        &mut state,
        &actors,
        &emergency,
        curve_core::Message::SetFrozen {
            namespace: ns,
            frozen: true,
        },
    )
    .unwrap();
    for instruction in [
        curve_core::Message::ClosePool { now: 1 },
        curve_core::Message::WithdrawReserves { now: 1 },
    ] {
        rejected_without_changes(
            &mut state,
            |state| curve_call(state, &actors, pool_id, &namespace, instruction.clone()),
            "Unauthorized",
        );
        curve_call(&mut state, &actors, pool_id, &owner, instruction).unwrap();
    }
    assert!(namespace_config(&state, ns).emergency.is_frozen);
    assert_eq!(balance(&state, id(&owner), LAUNCH), 1_000);
    assert_eq!(balance(&state, id(&owner), COLLATERAL), 1_000);
    assert_eq!(balance(&state, pool_id, LAUNCH), 0);
    assert_eq!(balance(&state, pool_id, COLLATERAL), 0);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn factory_expiry_rejects_current_and_stale_trades_and_can_close_while_frozen() {
    let mut fixture = fresh_sale(1, Some(2));
    trade(
        &mut fixture,
        curve_core::Message::SwapExactInput {
            membership: None,
            amount_in: 100,
            min_amount_out: 90,
            token_in: COLLATERAL,
            now: 1,
        },
    )
    .unwrap();
    fixture.inclusion_timestamp_ms = 2_000;
    for now in [1, 2] {
        for token_in in [COLLATERAL, fixture.definition] {
            let before = fixture.state.clone();
            let result = trade(
                &mut fixture,
                curve_core::Message::SwapExactInput {
                    membership: None,
                    amount_in: 10,
                    min_amount_out: 1,
                    token_in,
                    now,
                },
            );
            if now == 1 {
                assert!(
                    matches!(result, Err(lee::error::LeeError::OutOfValidityWindow)),
                    "{result:?}"
                );
            } else {
                assert!(
                    matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("InvalidPool")),
                    "{result:?}"
                );
            }
            assert!(
                fixture.state == before,
                "expired trade changed runtime state or nonce"
            );
        }
    }
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &emergency_key(),
        curve_core::Message::SetFrozen {
            namespace: fixture.namespace,
            frozen: true,
        },
    )
    .unwrap();
    for _ in 0..2 {
        fixture
            .call(
                &key(48),
                factory_core::Message::WithdrawFactoryProceeds {
                    creator: id(&key(48)),
                    now: 2,
                },
            )
            .unwrap();
    }
    assert_eq!(
        saved_pool(&fixture).pool.lifecycle,
        pool::PoolLifecycle::Withdrawn
    );
    assert_eq!(balance(&fixture.state, fixture.pool, fixture.definition), 0);
    assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 0);
    assert_eq!(
        balance(&fixture.state, fixture.factory, fixture.definition),
        310
    );
    assert_eq!(balance(&fixture.state, fixture.factory, COLLATERAL), 100);
    assert_eq!(
        balance(&fixture.state, id(&key(49)), fixture.definition),
        90
    );
    assert_eq!(balance(&fixture.state, id(&key(49)), COLLATERAL), 900);
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &emergency_key(),
        curve_core::Message::SetFrozen {
            namespace: fixture.namespace,
            frozen: false,
        },
    )
    .unwrap();
    for token_in in [COLLATERAL, fixture.definition] {
        rejected_without_changes(
            &mut fixture.state,
            |state| {
                curve_call(
                    state,
                    &fixture.actors,
                    fixture.pool,
                    &key(49),
                    curve_core::Message::SwapExactInput {
                        membership: None,
                        amount_in: 1,
                        min_amount_out: 0,
                        token_in,
                        now: 2,
                    },
                )
            },
            "InvalidPool",
        );
    }
}

fn set_sale_paused(
    fixture: &mut FactoryFixture,
    signer: &PrivateKey,
    paused: bool,
) -> Result<(), lee::error::LeeError> {
    fixture.call(
        signer,
        factory_core::Message::SetPaused {
            creator: id(signer),
            paused,
        },
    )
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn curve_pause_uses_current_creator_blocks_all_swaps_and_preserves_frozen_settlement() {
    let mut fixture = sale_with_pause(1, Some(2), curve_core::PausePolicy::AuthorityControlled);
    let creator = key(48);
    let successor = key(49);
    fixture.actors.extend([
        Actor::new(id(&emergency_key()), AUTHORITY),
        Actor::new(id(&emergency_key()), FACTORY),
    ]);
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &key(47),
        curve_core::Message::UpdateFee {
            namespace: fixture.namespace,
            protocol_fee_bps: 1_000,
        },
    )
    .unwrap();
    trade(
        &mut fixture,
        curve_core::Message::SwapExactInput {
            membership: None,
            amount_in: 100,
            min_amount_out: 82,
            token_in: COLLATERAL,
            now: 1,
        },
    )
    .unwrap();
    execute(
        &mut fixture.state,
        Actor::new(id(&creator), AUTHORITY),
        fixture.actors.clone(),
        authority::Message::Transfer {
            role: factory_core::compute_curve_creator_role_pda(fixture.namespace, FACTORY, SALT),
            successor: id(&successor),
        },
        &[&creator, &successor],
    )
    .unwrap();
    for paused in [false, true, true] {
        for signer in [&creator, &key(47), &emergency_key()] {
            let before = fixture.state.clone();
            let result = set_sale_paused(&mut fixture, signer, paused);
            assert!(
                matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("Unauthorized")),
                "{result:?}"
            );
            assert!(fixture.state == before);
        }
        rejected_without_changes(
            &mut fixture.state,
            |state| {
                curve_call(
                    state,
                    &fixture.actors,
                    fixture.pool,
                    &successor,
                    curve_core::Message::SetPaused { paused },
                )
            },
            "Unauthorized",
        );
        set_sale_paused(&mut fixture, &successor, paused).unwrap();
    }
    assert_eq!(
        saved_pool(&fixture).trade_control,
        curve_core::TradeControl::Paused
    );
    for token_in in [COLLATERAL, fixture.definition] {
        for instruction in [
            curve_core::Message::SwapExactInput {
                membership: None,
                amount_in: 1,
                min_amount_out: 0,
                token_in,
                now: 1,
            },
            curve_core::Message::SwapExactOutput {
                membership: None,
                amount_out: 1,
                max_amount_in: 100,
                token_in,
                now: 1,
            },
        ] {
            rejected_without_changes(
                &mut fixture.state,
                |state| {
                    curve_call(
                        state,
                        &fixture.actors,
                        fixture.pool,
                        &successor,
                        instruction,
                    )
                },
                "Paused",
            );
        }
    }
    collect(&mut fixture).unwrap();
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 10);
    assert_eq!(saved_pool(&fixture).pool.fees_accrued, 0);
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &emergency_key(),
        curve_core::Message::SetFrozen {
            namespace: fixture.namespace,
            frozen: true,
        },
    )
    .unwrap();
    set_sale_paused(&mut fixture, &successor, false).unwrap();
    assert!(
        namespace_config(&fixture.state, fixture.namespace)
            .emergency
            .is_frozen
    );
    rejected_without_changes(
        &mut fixture.state,
        |state| {
            curve_call(
                state,
                &fixture.actors,
                fixture.pool,
                &successor,
                curve_core::Message::SwapExactInput {
                    membership: None,
                    amount_in: 1,
                    min_amount_out: 0,
                    token_in: COLLATERAL,
                    now: 1,
                },
            )
        },
        "Frozen",
    );
    set_sale_paused(&mut fixture, &successor, true).unwrap();
    fixture.inclusion_timestamp_ms = 2_000;
    let before = fixture.state.clone();
    let result = set_sale_paused(&mut fixture, &successor, false);
    assert!(
        matches!(result, Err(lee::error::LeeError::OutOfValidityWindow)),
        "{result:?}"
    );
    assert!(fixture.state == before);
    for _ in 0..3 {
        fixture
            .call(
                &successor,
                factory_core::Message::WithdrawFactoryProceeds {
                    creator: id(&successor),
                    now: 2,
                },
            )
            .unwrap();
    }
    assert_eq!(
        saved_pool(&fixture).trade_control,
        curve_core::TradeControl::Paused
    );
    assert_eq!(
        saved_pool(&fixture).pool.lifecycle,
        pool::PoolLifecycle::Withdrawn
    );
    assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 0);
    assert_eq!(balance(&fixture.state, fixture.factory, COLLATERAL), 0);
    assert_eq!(balance(&fixture.state, id(&successor), COLLATERAL), 990);
    rejected_without_changes(
        &mut fixture.state,
        |state| {
            execute_at(
                state,
                Actor::new(fixture.factory, FACTORY),
                fixture.actors.clone(),
                factory_core::Message::SetPaused {
                    creator: id(&successor),
                    paused: false,
                },
                &[&successor],
                2_000,
            )
        },
        "Closed",
    );
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn disabled_curve_pause_and_depleted_sale_controls_reject_without_changes() {
    let mut disabled = fresh_sale(1, None);
    let before = disabled.state.clone();
    let result = set_sale_paused(&mut disabled, &key(48), true);
    assert!(
        matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("PauseDisabled")),
        "{result:?}"
    );
    assert!(disabled.state == before);
    let mut depleted = sale_with_pause(1, None, curve_core::PausePolicy::AuthorityControlled);
    trade(
        &mut depleted,
        curve_core::Message::SwapExactOutput {
            membership: None,
            amount_out: 400,
            max_amount_in: 667,
            token_in: COLLATERAL,
            now: 1,
        },
    )
    .unwrap();
    for paused in [false, true] {
        let before = depleted.state.clone();
        let result = set_sale_paused(&mut depleted, &key(48), paused);
        assert!(
            matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("Closed")),
            "{result:?}"
        );
        assert!(depleted.state == before);
    }
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn direct_curve_pause_follows_transferred_owner_and_keeps_reserve_recovery() {
    let namespace = key(60);
    let owner = key(61);
    let successor = key(62);
    let emergency = emergency_key();
    let ns = id(&namespace);
    let (mut state, mut actors) =
        base_state(&namespace, &[id(&owner), id(&successor), id(&emergency)]);
    let pool = curve_core::compute_pool_pda(ns, CURVE, LAUNCH, COLLATERAL, id(&owner));
    let role = curve_core::compute_pool_owner_role(ns, pool, CURVE);
    actors.extend([
        Actor::new(pool, CURVE),
        Actor::new(pool, TOKEN),
        Actor::new(role, AUTHORITY),
        holding(pool, LAUNCH),
        holding(pool, COLLATERAL),
    ]);
    curve_call(
        &mut state,
        &actors,
        pool,
        &owner,
        curve_core::Message::CreatePool {
            buy_policy: curve_core::BuyPolicy::Open,
            pause_policy: curve_core::PausePolicy::AuthorityControlled,
            ending_policy: curve_core::EndingPolicy::Manual,
            sell_fee_bps: 0,
            namespace: ns,
            token0_definition_id: LAUNCH,
            token1_definition_id: COLLATERAL,
            allowed_input_token: None,
            defer_funding: false,
            token0_amount: 400,
            token1_amount: 100,
            virtual_reserve0: 1_000,
            virtual_reserve1: 1_000,
            close_timestamp: None,
            close_on_depletion: None,
            owner: id(&owner),
            owner_program_id: None,
            owner_program_seed: None,
            curve_program_id: CURVE,
            now: 1,
        },
    )
    .unwrap();
    execute(
        &mut state,
        Actor::new(id(&owner), AUTHORITY),
        actors.clone(),
        authority::Message::Transfer {
            role,
            successor: id(&successor),
        },
        &[&owner, &successor],
    )
    .unwrap();
    for paused in [false, true, true] {
        for signer in [&owner, &namespace, &emergency] {
            rejected_without_changes(
                &mut state,
                |state| {
                    curve_call(
                        state,
                        &actors,
                        pool,
                        signer,
                        curve_core::Message::SetPaused { paused },
                    )
                },
                "Unauthorized",
            );
        }
        let call = launchpad_client::build_set_curve_paused_invocation(
            ns,
            CURVE,
            pool,
            id(&successor),
            paused,
        );
        execute(
            &mut state,
            call.destination,
            call.actors,
            call.instruction,
            &[&successor],
        )
        .unwrap();
    }
    for token_in in [LAUNCH, COLLATERAL] {
        for instruction in [
            curve_core::Message::SwapExactInput {
                membership: None,
                amount_in: 1,
                min_amount_out: 0,
                token_in,
                now: 1,
            },
            curve_core::Message::SwapExactOutput {
                membership: None,
                amount_out: 1,
                max_amount_in: 100,
                token_in,
                now: 1,
            },
        ] {
            rejected_without_changes(
                &mut state,
                |state| curve_call(state, &actors, pool, &successor, instruction),
                "Paused",
            );
        }
    }
    for instruction in [
        curve_core::Message::ClosePool { now: 1 },
        curve_core::Message::WithdrawReserves { now: 1 },
    ] {
        curve_call(&mut state, &actors, pool, &successor, instruction).unwrap();
    }
    assert_eq!(balance(&state, id(&successor), LAUNCH), 1_400);
    assert_eq!(balance(&state, id(&successor), COLLATERAL), 1_100);
    assert_eq!(balance(&state, pool, LAUNCH), 0);
    assert_eq!(balance(&state, pool, COLLATERAL), 0);
    rejected_without_changes(
        &mut state,
        |state| {
            curve_call(
                state,
                &actors,
                pool,
                &successor,
                curve_core::Message::SetPaused { paused: false },
            )
        },
        "Closed",
    );
}
