use super::*;

#[test]
fn weighted_settlement_before_end_rejected_by_inclusion_window() {
    let (mut state, factory, pool, launch, _escrow, key, actors, salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));

    // Settlement at 299_999 ms (before the 300s end = 300_000 ms) must be rejected
    state = clock_state(state, 299_999);
    let before = state.clone();
    let result = factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::WithdrawWeightedProceeds { creator },
        299_999,
    );
    assert!(
        matches!(result, Err(lee::error::LeeError::OutOfValidityWindow)),
        "early settlement should be rejected: {result:?}"
    );
    assert!(state == before, "rejected settlement changed state");
}

#[test]
fn weighted_allocation_before_settlement_then_settlement() {
    let (mut state, factory, pool, launch, escrow, key, actors, salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));

    state = clock_state(state, 300_000);

    let creator_tokens_before = balance(&state, creator, launch);
    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::ClaimWeightedAllocation { creator },
        300_000,
    )
    .unwrap();
    assert_eq!(
        balance(&state, creator, launch),
        creator_tokens_before + 200
    );
    assert_eq!(balance(&state, escrow, launch), 0);

    let creator_collateral_before = balance(&state, creator, COLLATERAL);
    let sale = fixed_pool(&state, pool);
    let recorded = sale.recorded_collateral;
    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::WithdrawWeightedProceeds { creator },
        300_000,
    )
    .unwrap();
    assert_eq!(
        fixed_pool(&state, pool).settlement_progress,
        lbp_core::SettlementProgress::ManualComplete
    );
    assert_eq!(
        balance(&state, creator, COLLATERAL),
        creator_collateral_before + recorded
    );
}

#[test]
fn weighted_settlement_failures_roll_back_all_effects_and_retry_once() {
    let (state, factory, pool, launch, escrow, key, actors, salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
    let state = clock_state(state, 300_000);
    let sale = fixed_pool(&state, pool);
    assert_eq!(
        sale.settlement_progress,
        lbp_core::SettlementProgress::Unselected
    );
    let mut expected = sale.clone();
    expected.closure_observed = true;
    expected.recorded_collateral = 0;
    expected.accrued_fees = 0;
    expected.settlement_progress = lbp_core::SettlementProgress::ManualComplete;
    assert!(sale.accrued_fees > 0);
    assert!(sale.remaining_inventory > 0);
    let definition_bytes = state
        .get_account_by_id(launch)
        .data
        .actor_state(TOKEN_PROGRAM_ACCOUNT_ID)
        .to_vec();
    let definition = TokenState::decode(&definition_bytes).unwrap();
    let TokenState::Definition { supply, .. } = definition.clone() else {
        panic!("expected definition")
    };

    // Each failed credit must restore the complete transaction.
    for fault in ["fee", "launch", "proceeds"] {
        let owner = if fault == "fee" { TREASURY } else { creator };
        let asset = if fault == "launch" {
            launch
        } else {
            COLLATERAL
        };
        let actor = holding(owner, asset);
        let original = state
            .get_account_by_id(actor.account_id)
            .data
            .actor_state(TOKEN_PROGRAM_ACCOUNT_ID)
            .to_vec();
        let broken = borsh::to_vec(&TokenState::Holding {
            owner,
            definition: asset,
            kind: TokenKind::Fungible,
            balance: u128::MAX,
        })
        .unwrap();
        let reason = "Overflow";
        let mut rejected = state.clone().with_public_accounts([account(actor, broken)]);
        let before = rejected.clone();
        let nonce = rejected.get_account_by_id(creator).nonce;
        let result = factory_execute_at(
            &mut rejected,
            factory,
            &actors,
            &key,
            salt,
            launch,
            pool,
            factory_core::Message::WithdrawWeightedProceeds { creator },
            300_000,
        );
        assert!(
            matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains(reason)),
            "{fault}: expected {reason}, got {result:?}"
        );
        assert!(
            rejected == before,
            "{fault}: failed settlement changed runtime state"
        );
        assert_eq!(rejected.get_account_by_id(creator).nonce, nonce);
        assert_eq!(
            fixed_pool(&rejected, pool).settlement_progress,
            lbp_core::SettlementProgress::Unselected
        );

        let mut repaired = rejected.with_public_accounts([account(actor, original)]);
        let creator_before = balance(&repaired, creator, COLLATERAL);
        let treasury_before = balance(&repaired, TREASURY, COLLATERAL);
        let tokens_before = balance(&repaired, pool, launch);
        for attempt in 0..2 {
            let events = factory_execute_events_at(
                &mut repaired,
                factory,
                &actors,
                &key,
                salt,
                launch,
                pool,
                factory_core::Message::WithdrawWeightedProceeds { creator },
                300_000,
            )
            .unwrap();
            if attempt == 0 {
                assert_lbp_settlement_events(&events, pool, &sale, creator, TREASURY, None);
            } else {
                assert!(events.is_empty(), "settlement retry emitted an event");
            }
            assert_eq!(
                fixed_pool(&repaired, pool).settlement_progress,
                lbp_core::SettlementProgress::ManualComplete
            );
            assert_eq!(fixed_pool(&repaired, pool), expected);
            assert_eq!(
                balance(&repaired, creator, COLLATERAL),
                creator_before + sale.recorded_collateral
            );
            assert_eq!(
                balance(&repaired, TREASURY, COLLATERAL),
                treasury_before + sale.accrued_fees
            );
            assert_eq!(
                balance(&repaired, pool, launch),
                tokens_before - sale.remaining_inventory
            );
            let current = TokenState::decode(
                repaired
                    .get_account_by_id(launch)
                    .data
                    .actor_state(TOKEN_PROGRAM_ACCOUNT_ID),
            )
            .unwrap();
            let TokenState::Definition {
                supply: current_supply,
                ..
            } = current
            else {
                panic!("expected definition")
            };
            assert_eq!(current_supply, supply);
            assert_eq!(balance(&repaired, escrow, launch), sale.creator_allocation);
        }
    }
}

#[test]
fn weighted_allocation_credit_failure_rolls_back_and_retry_pays_once() {
    let (state, factory, pool, launch, escrow, key, actors, salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
    let state = clock_state(state, 300_000);
    let recipient = holding(creator, launch);
    let original = state
        .get_account_by_id(recipient.account_id)
        .data
        .actor_state(TOKEN_PROGRAM_ACCOUNT_ID)
        .to_vec();
    let mut rejected = state.with_public_accounts([account(
        recipient,
        borsh::to_vec(&TokenState::Holding {
            owner: creator,
            definition: launch,
            kind: TokenKind::Fungible,
            balance: u128::MAX,
        })
        .unwrap(),
    )]);
    let before = rejected.clone();
    let nonce = rejected.get_account_by_id(creator).nonce;
    let result = factory_execute_at(
        &mut rejected,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::ClaimWeightedAllocation { creator },
        300_000,
    );
    assert!(
        matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains("Overflow")),
        "expected allocation credit overflow, got {result:?}"
    );
    assert!(
        rejected == before,
        "failed allocation changed runtime state"
    );
    assert_eq!(rejected.get_account_by_id(creator).nonce, nonce);
    let mut repaired = rejected.with_public_accounts([account(recipient, original)]);
    let balance_before = balance(&repaired, creator, launch);
    let allocation = balance(&repaired, escrow, launch);
    assert!(allocation > 0);
    for _ in 0..2 {
        factory_execute_at(
            &mut repaired,
            factory,
            &actors,
            &key,
            salt,
            launch,
            pool,
            factory_core::Message::ClaimWeightedAllocation { creator },
            300_000,
        )
        .unwrap();
        assert_eq!(
            balance(&repaired, creator, launch),
            balance_before + allocation
        );
        assert_eq!(balance(&repaired, escrow, launch), 0);
    }
}

#[test]
fn fixed_expiry_settlement_and_allocation_in_both_orders_keep_surplus_while_frozen() {
    for allocation_first in [false, true] {
        let mut env = fixed_settlement_env(Some(200));
        let creator = env.creator();
        env.state = fixed_config(env.state, 333, NEXT_TREASURY, true);
        let sale = fixed_pool(&env.state, env.pool);
        assert_eq!(
            (
                sale.remaining_inventory,
                sale.recorded_collateral,
                sale.accrued_fees
            ),
            (700, 150, 6)
        );
        let creator_collateral = balance(&env.state, creator, COLLATERAL);
        let creator_tokens = balance(&env.state, creator, env.launch);
        let supply = token_supply(&env.state, env.launch);
        let settle = factory_core::Message::WithdrawFactoryProceeds { creator, now: 200 };
        let claim = factory_core::Message::ClaimCreatorAllocation { creator, now: 200 };
        let operations = if allocation_first {
            [claim.clone(), settle.clone()]
        } else {
            [settle.clone(), claim.clone()]
        };
        for operation in operations {
            env.execute(operation, 200_000).unwrap();
        }
        for _ in 0..2 {
            assert_eq!(
                balance(&env.state, creator, COLLATERAL),
                creator_collateral + 150
            );
            assert_eq!(
                balance(&env.state, creator, env.launch),
                creator_tokens + 900
            );
            assert_eq!(balance(&env.state, env.escrow, env.launch), 0);
            assert_eq!(balance(&env.state, NEXT_TREASURY, COLLATERAL), 6);
            assert_eq!(balance(&env.state, TREASURY, COLLATERAL), 0);
            assert_eq!(balance(&env.state, env.pool, COLLATERAL), 41);
            assert_eq!(balance(&env.state, env.pool, env.launch), 7);
            assert_eq!(token_supply(&env.state, env.launch), supply);
            let settled = fixed_pool(&env.state, env.pool);
            assert_eq!(
                settled.settlement_progress,
                lbp_core::SettlementProgress::ManualComplete
            );
            assert_eq!(
                settled.remaining_inventory, 700,
                "payout must not become sellout evidence"
            );
            assert_eq!((settled.recorded_collateral, settled.accrued_fees), (0, 0));
            env.execute(settle.clone(), 200_000).unwrap();
            env.execute(claim.clone(), 200_000).unwrap();
        }
    }
}

#[test]
fn fixed_no_end_rejects_early_payouts_then_sellout_pays_without_burning_surplus() {
    let mut env = fixed_settlement_env(None);
    let creator = env.creator();
    for operation in [
        factory_core::Message::WithdrawFactoryProceeds { creator, now: 100 },
        factory_core::Message::ClaimCreatorAllocation { creator, now: 100 },
    ] {
        let before = env.state.clone();
        let nonce = env.state.get_account_by_id(creator).nonce;
        let result = env.execute(operation, 100_000);
        assert!(
            matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("NotClosed")),
            "{result:?}"
        );
        assert!(env.state == before);
        assert_eq!(env.state.get_account_by_id(creator).nonce, nonce);
    }
    // The buyer needs more collateral than the setup fixture's balance to sell out.
    env.state = env
        .state
        .with_public_accounts([token_holding(creator, COLLATERAL, 2_000)]);
    env.buy(700, 1_087);
    let supply = token_supply(&env.state, env.launch);
    let collateral = balance(&env.state, creator, COLLATERAL);
    let tokens = balance(&env.state, creator, env.launch);
    let sale = fixed_pool(&env.state, env.pool);
    assert_eq!(sale.remaining_inventory, 0);
    for _ in 0..2 {
        env.execute(
            factory_core::Message::ClaimCreatorAllocation { creator, now: 100 },
            100_000,
        )
        .unwrap();
        env.execute(
            factory_core::Message::WithdrawFactoryProceeds { creator, now: 100 },
            100_000,
        )
        .unwrap();
        assert_eq!(
            balance(&env.state, creator, COLLATERAL),
            collateral + sale.recorded_collateral
        );
        assert_eq!(balance(&env.state, creator, env.launch), tokens + 200);
        assert_eq!(balance(&env.state, TREASURY, COLLATERAL), sale.accrued_fees);
        assert_eq!(balance(&env.state, env.pool, COLLATERAL), 41);
        assert_eq!(balance(&env.state, env.pool, env.launch), 7);
        assert_eq!(token_supply(&env.state, env.launch), supply);
    }
}

#[test]
fn fixed_expiry_payouts_reject_inclusion_before_end_even_with_future_hint() {
    let mut env = fixed_settlement_env(Some(200));
    let creator = env.creator();
    for operation in [
        factory_core::Message::WithdrawFactoryProceeds { creator, now: 200 },
        factory_core::Message::ClaimCreatorAllocation { creator, now: 200 },
    ] {
        let before = env.state.clone();
        let nonce = env.state.get_account_by_id(creator).nonce;
        let result = env.execute(operation, 199_999);
        assert!(
            matches!(result, Err(lee::error::LeeError::OutOfValidityWindow)),
            "{result:?}"
        );
        assert!(env.state == before);
        assert_eq!(env.state.get_account_by_id(creator).nonce, nonce);
    }
}

#[test]
fn fixed_settlement_token_faults_restore_full_runtime_then_repair_and_pay_once() {
    let env = fixed_settlement_env(Some(200));
    let creator = env.creator();
    let sale = fixed_pool(&env.state, env.pool);
    assert_eq!(
        sale.settlement_progress,
        lbp_core::SettlementProgress::Unselected
    );
    let mut expected = sale.clone();
    expected.closure_observed = true;
    expected.recorded_collateral = 0;
    expected.accrued_fees = 0;
    expected.settlement_progress = lbp_core::SettlementProgress::ManualComplete;
    let supply = token_supply(&env.state, env.launch);
    for fault in ["fee", "launch", "proceeds", "allocation"] {
        let actor = match fault {
            "fee" => holding(TREASURY, COLLATERAL),
            "launch" => holding(creator, env.launch),
            "proceeds" => holding(creator, COLLATERAL),
            _ => holding(creator, env.launch),
        };
        let original = env
            .state
            .get_account_by_id(actor.account_id)
            .data
            .actor_state(TOKEN_PROGRAM_ACCOUNT_ID)
            .to_vec();
        let mut broken = TokenState::decode(&original).unwrap();
        match &mut broken {
            TokenState::Definition { supply, .. } => *supply = 0,
            TokenState::Holding { balance, .. } => *balance = u128::MAX,
        }
        let mut state = env
            .state
            .clone()
            .with_public_accounts([account(actor, borsh::to_vec(&broken).unwrap())]);
        let before = state.clone();
        let nonce = state.get_account_by_id(creator).nonce;
        let operation = if fault == "allocation" {
            factory_core::Message::ClaimCreatorAllocation { creator, now: 200 }
        } else {
            factory_core::Message::WithdrawFactoryProceeds { creator, now: 200 }
        };
        let result = factory_execute_at(
            &mut state,
            env.factory,
            &env.actors,
            &env.key,
            env.salt,
            env.launch,
            env.pool,
            operation.clone(),
            200_000,
        );
        let reason = "Overflow";
        assert!(
            matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains(reason)),
            "{fault}: {result:?}"
        );
        assert!(
            state == before,
            "{fault}: failed payout changed runtime state"
        );
        assert_eq!(state.get_account_by_id(creator).nonce, nonce);
        assert_eq!(
            fixed_pool(&state, env.pool).settlement_progress,
            lbp_core::SettlementProgress::Unselected
        );
        state = state.with_public_accounts([account(actor, original)]);
        let collateral = balance(&state, creator, COLLATERAL);
        let tokens = balance(&state, creator, env.launch);
        for attempt in 0..2 {
            let events = factory_execute_events_at(
                &mut state,
                env.factory,
                &env.actors,
                &env.key,
                env.salt,
                env.launch,
                env.pool,
                operation.clone(),
                200_000,
            )
            .unwrap();
            if attempt == 0 {
                if fault == "allocation" {
                    assert_eq!(
                        sale_event_kinds(
                            &events,
                            LBP_FACTORY_PROGRAM,
                            launch_subject(env.factory, env.pool)
                        ),
                        vec![SaleEventKindV1::AllocationReleased {
                            definition: *env.launch.value(),
                            recipient: *creator.value(),
                            amount: 200,
                            release: AllocationReleaseV1::AfterEnd,
                        }]
                    );
                    assert!(sale_event_kinds(&events, LBP, pool_subject(env.pool)).is_empty());
                } else {
                    assert_lbp_settlement_events(&events, env.pool, &sale, creator, TREASURY, None);
                }
            } else {
                assert!(events.is_empty(), "payout retry emitted an event");
            }
            if fault == "allocation" {
                assert_eq!(balance(&state, creator, env.launch), tokens + 200);
                assert_eq!(balance(&state, env.escrow, env.launch), 0);
                assert_eq!(balance(&state, creator, COLLATERAL), collateral);
                assert_eq!(token_supply(&state, env.launch), supply);
                assert_eq!(fixed_pool(&state, env.pool), sale);
            } else {
                assert_eq!(
                    balance(&state, creator, COLLATERAL),
                    collateral + sale.recorded_collateral
                );
                assert_eq!(balance(&state, TREASURY, COLLATERAL), sale.accrued_fees);
                assert_eq!(balance(&state, env.pool, env.launch), 7);
                assert_eq!(balance(&state, env.pool, COLLATERAL), 41);
                assert_eq!(token_supply(&state, env.launch), supply);
                assert_eq!(balance(&state, env.escrow, env.launch), 200);
                let settled = fixed_pool(&state, env.pool);
                assert_eq!(
                    settled.settlement_progress,
                    lbp_core::SettlementProgress::ManualComplete
                );
                assert_eq!(settled, expected);
                assert_eq!((settled.recorded_collateral, settled.accrued_fees), (0, 0));
            }
        }
    }
}

fn reject_payout_roles(
    state: V03State,
    factory: AccountId,
    pool: AccountId,
    launch: AccountId,
    actors: &[Actor],
    salt: [u8; 32],
    weighted: bool,
) {
    const FACTORY: AccountId = AccountId::new([17; 32]);
    for (seed, role) in [
        (97, None),
        (
            98,
            Some(factory_core::compute_creator_role_pda(
                NAMESPACE, FACTORY, [25; 32],
            )),
        ),
        (99, Some(curve_core::compute_admin_role(NAMESPACE, CURVE))),
    ] {
        let key = PrivateKey::try_new([seed; 32]).unwrap();
        let caller = AccountId::from(&PublicKey::new_from_private_key(&key));
        let mut state = state.clone();
        let mut actors = actors.to_vec();
        if let Some(role) = role {
            // Seed unrelated roles through the authority API, without copying its private layout.
            state = state.with_public_accounts([seeded_role(role, caller)]);
            actors.push(Actor::new(role, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID));
        }
        actors.extend([
            Actor::new(caller, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
            Actor::new(caller, TOKEN_PROGRAM_ACCOUNT_ID),
            Actor::new(caller, FACTORY),
            holding(caller, launch),
            holding(caller, COLLATERAL),
        ]);
        for allocation in [false, true] {
            let operation = match (weighted, allocation) {
                (true, false) => {
                    factory_core::Message::WithdrawWeightedProceeds { creator: caller }
                }
                (true, true) => factory_core::Message::ClaimWeightedAllocation { creator: caller },
                (false, false) => factory_core::Message::WithdrawFactoryProceeds {
                    creator: caller,
                    now: 200,
                },
                (false, true) => factory_core::Message::ClaimCreatorAllocation {
                    creator: caller,
                    now: 200,
                },
            };
            let before = state.clone();
            let nonce = state.get_account_by_id(caller).nonce;
            let result = factory_execute_at(
                &mut state, factory, &actors, &key, salt, launch, pool, operation, 300_000,
            );
            assert!(
                matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains("Unauthorized")),
                "role {seed}, allocation {allocation}: {result:?}"
            );
            assert!(state == before, "unauthorized payout changed runtime state");
            assert_eq!(state.get_account_by_id(caller).nonce, nonce);
        }
    }
}

#[test]
fn fixed_payouts_reject_wrong_caller_other_sale_creator_and_namespace_admin() {
    let env = fixed_settlement_env(Some(200));
    reject_payout_roles(
        env.state,
        env.factory,
        env.pool,
        env.launch,
        &env.actors,
        env.salt,
        false,
    );
}

#[test]
fn weighted_payouts_reject_wrong_caller_other_sale_creator_and_namespace_admin() {
    let (state, factory, pool, launch, _escrow, _key, actors, salt) = weighted_factory_env();
    reject_payout_roles(state, factory, pool, launch, &actors, salt, true);
}

#[test]
fn fixed_selected_ending_controls_sellout_settlement() {
    use lbp_core::EndingPolicy;
    for ending in [
        EndingPolicy::Scheduled,
        EndingPolicy::Depletion,
        EndingPolicy::Either,
    ] {
        let end = if ending == EndingPolicy::Depletion {
            None
        } else {
            Some(200)
        };
        let mut env = fixed_factory_env_with_ending(
            end,
            factory_core::CreatorAllocationRelease::AfterEnd,
            curve_core::PausePolicy::Disabled,
            ending,
        );
        let creator = env.creator();
        env.state = env
            .state
            .with_public_accounts([token_holding(creator, COLLATERAL, 2_000)]);
        assert_eq!(fixed_pool(&env.state, env.pool).ending_policy, ending);
        env.buy(800, 1_242);
        let claim = factory_core::Message::ClaimCreatorAllocation { creator, now: 100 };
        let settle = factory_core::Message::WithdrawFactoryProceeds { creator, now: 100 };
        if ending == EndingPolicy::Scheduled {
            for operation in [claim.clone(), settle.clone()] {
                let before = env.state.clone();
                assert!(matches!(
                    env.execute(operation, 100_000),
                    Err(lee::error::LeeError::OutOfValidityWindow)
                ));
                assert!(env.state == before);
            }
        }
        let timestamp = if ending == EndingPolicy::Scheduled {
            200_000
        } else {
            100_000
        };
        env.execute(claim, timestamp).unwrap();
        env.execute(settle, timestamp).unwrap();
        assert_eq!(
            fixed_pool(&env.state, env.pool).settlement_progress,
            lbp_core::SettlementProgress::ManualComplete
        );
        assert_eq!(balance(&env.state, env.escrow, env.launch), 0);
        assert_eq!(balance(&env.state, creator, env.launch), 1_000);
        assert_eq!(balance(&env.state, TREASURY, COLLATERAL), 42);
    }
}

#[test]
fn fixed_zero_collateral_settlement_pays_unsold_once_and_authenticates_retry() {
    let mut env = fixed_factory_env(Some(200), factory_core::CreatorAllocationRelease::AfterEnd);
    let creator = env.creator();
    let initial = fixed_pool(&env.state, env.pool);
    assert_eq!(
        initial.settlement_progress,
        lbp_core::SettlementProgress::Unselected
    );
    assert_eq!((initial.recorded_collateral, initial.accrued_fees), (0, 0));
    assert_eq!(initial.remaining_inventory, 800);
    let creator_collateral = balance(&env.state, creator, COLLATERAL);
    let supply = token_supply(&env.state, env.launch);
    let mut expected = initial.clone();
    expected.closure_observed = true;
    expected.settlement_progress = lbp_core::SettlementProgress::ManualComplete;
    for _ in 0..2 {
        env.execute(
            factory_core::Message::WithdrawFactoryProceeds { creator, now: 200 },
            200_000,
        )
        .unwrap();
        assert_eq!(fixed_pool(&env.state, env.pool), expected);
        assert_eq!(balance(&env.state, creator, COLLATERAL), creator_collateral);
        assert_eq!(balance(&env.state, env.pool, COLLATERAL), 0);
        assert_eq!(balance(&env.state, env.pool, env.launch), 0);
        assert_eq!(balance(&env.state, TREASURY, COLLATERAL), 0);
        assert_eq!(balance(&env.state, env.escrow, env.launch), 200);
        assert_eq!(token_supply(&env.state, env.launch), supply);
    }
    reject_payout_roles(
        env.state,
        env.factory,
        env.pool,
        env.launch,
        &env.actors,
        env.salt,
        false,
    );
}

#[test]
fn weighted_seed_only_settlement_completes_manual_route_and_retries_keep_the_end_bound() {
    let (mut state, factory, pool, launch, escrow, key, actors, salt) =
        weighted_factory_env_with_buy(false);
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
    let initial = fixed_pool(&state, pool);
    assert_eq!(
        initial.settlement_progress,
        lbp_core::SettlementProgress::Unselected
    );
    assert_eq!(
        (initial.recorded_collateral, initial.accrued_fees),
        (100, 0)
    );
    assert_eq!(initial.remaining_inventory, 800);
    let creator_collateral = balance(&state, creator, COLLATERAL);
    let supply = token_supply(&state, launch);
    let mut expected = initial.clone();
    expected.closure_observed = true;
    expected.recorded_collateral = 0;
    expected.settlement_progress = lbp_core::SettlementProgress::ManualComplete;
    state = clock_state(state, 300_000);
    for _ in 0..2 {
        let before = state.clone();
        let nonce = state.get_account_by_id(creator).nonce;
        let early = factory_execute_at(
            &mut state,
            factory,
            &actors,
            &key,
            salt,
            launch,
            pool,
            factory_core::Message::WithdrawWeightedProceeds { creator },
            299_999,
        );
        assert!(
            matches!(early, Err(lee::error::LeeError::OutOfValidityWindow)),
            "seed-only settlement or retry lost its scheduled-end bound: {early:?}",
        );
        assert!(
            state == before,
            "early seed-only settlement changed runtime state"
        );
        assert_eq!(state.get_account_by_id(creator).nonce, nonce);
        factory_execute_at(
            &mut state,
            factory,
            &actors,
            &key,
            salt,
            launch,
            pool,
            factory_core::Message::WithdrawWeightedProceeds { creator },
            300_000,
        )
        .unwrap();
        assert_eq!(fixed_pool(&state, pool), expected);
        assert_eq!(
            balance(&state, creator, COLLATERAL),
            creator_collateral + 100
        );
        assert_eq!(balance(&state, pool, COLLATERAL), 0);
        assert_eq!(balance(&state, pool, launch), 0);
        assert_eq!(balance(&state, TREASURY, COLLATERAL), 0);
        assert_eq!(balance(&state, escrow, launch), 200);
        assert_eq!(token_supply(&state, launch), supply);
    }
    reject_payout_roles(state, factory, pool, launch, &actors, salt, true);
}
