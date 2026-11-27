use super::*;

#[test]
fn weighted_factory_controls_and_settlement_full_flow() {
    let (mut state, factory, pool, launch, escrow, key, actors, salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));

    let sale = fixed_pool(&state, pool);
    assert!(sale.funded);
    assert!(sale.accrued_fees > 0 || sale.recorded_collateral > 100);

    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::SetWeightedPaused {
            creator,
            paused: true,
        },
        150_000,
    )
    .unwrap();

    let buy_result = fixed_execute_at(
        &mut state,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        150_000,
    );
    assert!(buy_result.is_err());

    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::SetWeightedPaused {
            creator,
            paused: true,
        },
        150_000,
    )
    .unwrap();

    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::SetWeightedPaused {
            creator,
            paused: false,
        },
        150_000,
    )
    .unwrap();

    fixed_execute_at(
        &mut state,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        150_000,
    )
    .unwrap();

    fixed_execute_at(
        &mut state,
        pool,
        &key,
        lbp_core::Message::CollectFees,
        150_000,
    )
    .unwrap();
    let fee_collected = balance(&state, TREASURY, COLLATERAL);
    assert!(fee_collected > 0, "fees should have been collected");

    // Move clock past end (300 seconds → 300_000 ms)
    state = clock_state(state, 300_000);

    let creator_collateral_before = balance(&state, creator, COLLATERAL);
    let creator_tokens_before = balance(&state, creator, launch);
    let supply_before = token_supply(&state, launch);
    let pool_token_before = balance(&state, pool, launch);
    let sale_before = fixed_pool(&state, pool);
    let recorded = sale_before.recorded_collateral;
    let fees = sale_before.accrued_fees;
    let unsold = sale_before.remaining_inventory;

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

    let sale_after = fixed_pool(&state, pool);
    assert_eq!(
        sale_after.settlement_progress,
        lbp_core::SettlementProgress::ManualComplete
    );
    assert_eq!(sale_after.recorded_collateral, 0);
    assert_eq!(sale_after.accrued_fees, 0);

    let creator_collateral_after = balance(&state, creator, COLLATERAL);
    assert_eq!(
        creator_collateral_after,
        creator_collateral_before + recorded
    );

    let treasury_after = balance(&state, TREASURY, COLLATERAL);
    assert_eq!(treasury_after, fee_collected + fees);

    let pool_token_after = balance(&state, pool, launch);
    assert_eq!(pool_token_after, pool_token_before - unsold);

    assert_eq!(
        balance(&state, creator, launch),
        creator_tokens_before + unsold
    );
    assert_eq!(token_supply(&state, launch), supply_before);

    let before_retry = state.clone();
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
        balance(&state, creator, COLLATERAL),
        creator_collateral_after
    );

    let escrow_tokens = balance(&state, escrow, launch);
    assert_eq!(escrow_tokens, 200);
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
        balance(&before_retry, creator, launch) + 200
    );
    assert_eq!(balance(&state, escrow, launch), 0);

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
}

#[test]
fn weighted_controls_after_end_rejected_by_inclusion_window() {
    let (mut state, factory, pool, launch, _escrow, key, actors, salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));

    // Pause after end must be rejected (pool enforces time bound before end)
    state = clock_state(state, 300_000);
    let before = state.clone();
    let result = factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::SetWeightedPaused {
            creator,
            paused: true,
        },
        300_000,
    );
    assert!(
        matches!(result, Err(lee::error::LeeError::OutOfValidityWindow)),
        "late pause should be rejected: {result:?}"
    );
    assert!(state == before, "rejected pause changed state");
}

#[test]
fn weighted_unauthorized_factory_controls_rejected() {
    let (mut state, factory, pool, launch, _escrow, _key, actors, salt) = weighted_factory_env();

    let wrong_key = PrivateKey::try_new([99; 32]).unwrap();
    let wrong_caller = AccountId::from(&PublicKey::new_from_private_key(&wrong_key));

    // The caller signs but does not hold this sale's creator role.
    let mut actors_with_wrong = actors.clone();
    actors_with_wrong.push(Actor::new(
        wrong_caller,
        factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
    ));
    actors_with_wrong.push(Actor::new(wrong_caller, TOKEN_PROGRAM_ACCOUNT_ID));
    actors_with_wrong.push(holding(wrong_caller, launch));
    actors_with_wrong.push(holding(wrong_caller, COLLATERAL));

    let result = factory_execute_at(
        &mut state,
        factory,
        &actors_with_wrong,
        &wrong_key,
        salt,
        launch,
        pool,
        factory_core::Message::SetWeightedPaused {
            creator: wrong_caller,
            paused: true,
        },
        150_000,
    );
    assert!(result.is_err(), "unauthorized pause should fail");
}

#[test]
fn weighted_frozen_buys_blocked_but_settlement_allowed() {
    let (mut state, factory, pool, launch, _escrow, key, actors, salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));

    state = fixed_config(state, 100, TREASURY, true);

    let buy_result = fixed_execute_at(
        &mut state,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        150_000,
    );
    assert!(buy_result.is_err());

    fixed_execute_at(
        &mut state,
        pool,
        &key,
        lbp_core::Message::CollectFees,
        150_000,
    )
    .unwrap();

    state = clock_state(fixed_config(state, 100, TREASURY, true), 300_000);
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
}

#[test]
fn fixed_shared_pause_blocks_both_buys_but_keeps_frozen_collection_and_settlement() {
    let mut env = fixed_factory_env_with_policy(
        Some(200),
        factory_core::CreatorAllocationRelease::AfterEnd,
        curve_core::PausePolicy::AuthorityControlled,
    );
    let creator = env.creator();
    let initial = fixed_pool(&env.state, env.pool);
    for paused in [false, true, true] {
        env.execute(factory_core::Message::SetPaused { creator, paused }, 99_999)
            .unwrap();
        assert_eq!(
            fixed_pool(&env.state, env.pool).trade_control.is_paused(),
            paused
        );
    }
    for operation in [
        lbp_core::Message::BuyExactInput {
            membership: Some(env.path.clone()),
            amount_in: 50,
            min_amount_out: 1,
            now: 100,
        },
        lbp_core::Message::BuyExactOutput {
            membership: Some(env.path.clone()),
            amount_out: 10,
            max_amount_in: 100,
            now: 100,
        },
    ] {
        fixed_rejected(&mut env.state, env.pool, &env.key, operation, "Paused");
    }
    env.execute(
        factory_core::Message::SetPaused {
            creator,
            paused: false,
        },
        100_000,
    )
    .unwrap();
    env.buy(100, 156);
    env.execute(
        factory_core::Message::SetPaused {
            creator,
            paused: true,
        },
        100_000,
    )
    .unwrap();
    env.state = fixed_config(env.state, 333, TREASURY, true);
    env.execute(
        factory_core::Message::SetPaused {
            creator,
            paused: false,
        },
        100_000,
    )
    .unwrap();
    fixed_rejected(
        &mut env.state,
        env.pool,
        &env.key,
        lbp_core::Message::BuyExactOutput {
            membership: Some(env.path.clone()),
            amount_out: 10,
            max_amount_in: 100,
            now: 100,
        },
        "Frozen",
    );
    env.execute(
        factory_core::Message::SetPaused {
            creator,
            paused: true,
        },
        199_999,
    )
    .unwrap();
    let sale = fixed_pool(&env.state, env.pool);
    assert_eq!(sale.start_timestamp, initial.start_timestamp);
    assert_eq!(sale.end_timestamp, initial.end_timestamp);
    for paused in [false, true] {
        let before = env.state.clone();
        let result = env.execute(
            factory_core::Message::SetPaused { creator, paused },
            200_000,
        );
        assert!(
            matches!(result, Err(lee::error::LeeError::OutOfValidityWindow)),
            "{result:?}"
        );
        assert!(env.state == before);
    }
    fixed_execute_at(
        &mut env.state,
        env.pool,
        &env.key,
        lbp_core::Message::CollectFees,
        200_000,
    )
    .unwrap();
    assert_eq!(balance(&env.state, TREASURY, COLLATERAL), 6);
    env.execute(
        factory_core::Message::WithdrawFactoryProceeds { creator, now: 200 },
        200_000,
    )
    .unwrap();
    env.execute(
        factory_core::Message::ClaimCreatorAllocation { creator, now: 200 },
        200_000,
    )
    .unwrap();
    let sale = fixed_pool(&env.state, env.pool);
    assert_eq!(
        sale.settlement_progress,
        lbp_core::SettlementProgress::ManualComplete
    );
    assert!(sale.trade_control.is_paused());
    assert_eq!(balance(&env.state, env.escrow, env.launch), 0);
}

#[test]
fn fixed_shared_controls_reject_disabled_policy_and_no_end_sellout() {
    for enabled in [false, true] {
        let mut env = fixed_factory_env_with_policy(
            None,
            factory_core::CreatorAllocationRelease::AfterEnd,
            if enabled {
                curve_core::PausePolicy::AuthorityControlled
            } else {
                curve_core::PausePolicy::Disabled
            },
        );
        let creator = env.creator();
        if enabled {
            env.state = env
                .state
                .with_public_accounts([token_holding(creator, COLLATERAL, 2_000)]);
            env.buy(800, 1_242);
        }
        for paused in [false, true] {
            let before = env.state.clone();
            let result = env.execute(
                factory_core::Message::SetPaused { creator, paused },
                100_000,
            );
            let reason = if enabled { "Closed" } else { "PauseDisabled" };
            assert!(
                matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains(reason)),
                "{result:?}"
            );
            assert!(env.state == before);
        }
    }
}

#[test]
fn shared_lbp_controls_authenticate_no_ops_and_reject_direct_pool_bypass() {
    const FACTORY: AccountId = AccountId::new([17; 32]);
    for weighted in [false, true] {
        let (mut state, factory, pool, launch, _escrow, key, actors, salt) = if weighted {
            weighted_factory_env()
        } else {
            let env = fixed_factory_env_with_policy(
                Some(300),
                factory_core::CreatorAllocationRelease::AfterEnd,
                curve_core::PausePolicy::AuthorityControlled,
            );
            (
                env.state,
                env.factory,
                env.pool,
                env.launch,
                env.escrow,
                env.key,
                env.actors,
                env.salt,
            )
        };
        let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
        // The participant Execute wrapper rejects controls before calling the pool.
        let before = state.clone();
        let nonce = state.get_account_by_id(creator).nonce;
        let result = fixed_execute_at(
            &mut state,
            pool,
            &key,
            lbp_core::Message::SetPaused { paused: true },
            150_000,
        );
        assert!(
            matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains("Unauthorized")),
            "participant-wrapper rejection: {result:?}"
        );
        assert!(state == before, "participant wrapper changed runtime state");
        assert_eq!(state.get_account_by_id(creator).nonce, nonce);
        for paused in [false, true] {
            factory_execute_at(
                &mut state,
                factory,
                &actors,
                &key,
                salt,
                launch,
                pool,
                factory_core::Message::SetPaused { creator, paused },
                150_000,
            )
            .unwrap();
            assert_eq!(fixed_pool(&state, pool).trade_control.is_paused(), paused);
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
                let wrong_key = PrivateKey::try_new([seed; 32]).unwrap();
                let caller = AccountId::from(&PublicKey::new_from_private_key(&wrong_key));
                let mut rejected = state.clone();
                let mut actors = actors.clone();
                if let Some(role) = role {
                    rejected = rejected.with_public_accounts([seeded_role(role, caller)]);
                    actors.push(Actor::new(role, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID));
                }
                actors.extend([
                    Actor::new(caller, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
                    Actor::new(caller, FACTORY),
                    Actor::new(caller, LBP),
                ]);
                let before = rejected.clone();
                let result = factory_execute_at(
                    &mut rejected,
                    factory,
                    &actors,
                    &wrong_key,
                    salt,
                    launch,
                    pool,
                    factory_core::Message::SetPaused {
                        creator: caller,
                        paused,
                    },
                    150_000,
                );
                assert!(
                    matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains("Unauthorized")),
                    "{result:?}"
                );
                assert!(rejected == before, "rejected no-op changed runtime state");
            }
            for requested_pause in [false, true] {
                let instruction = lbp_core::Message::SetPaused {
                    paused: requested_pause,
                };
                let mut direct = fixed_transaction(&state, pool, &key, instruction.clone()).message;
                direct.execution.root.to = Actor::new(pool, LBP);
                direct.execution.root.message = borsh::to_vec(&instruction).unwrap();
                let witnesses = WitnessSet::for_message(&direct, &[&key]);
                let before = state.clone();
                let nonce = state.get_account_by_id(creator).nonce;
                let result = state.transition_from_public_transaction(
                    &PublicTransaction::new(direct, witnesses),
                    1,
                    150_000,
                );
                assert!(
                    matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains("Unauthorized")),
                    "pool bypass with paused={paused}, requested_pause={requested_pause}: {result:?}"
                );
                assert!(state == before, "pool bypass changed runtime state");
                assert_eq!(state.get_account_by_id(creator).nonce, nonce);
            }
        }
    }
}

#[test]
fn weighted_shared_controls_keep_pause_and_frozen_resume_independent() {
    let (mut state, factory, pool, launch, _escrow, key, actors, salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
    for paused in [false, true, true, false, true] {
        factory_execute_at(
            &mut state,
            factory,
            &actors,
            &key,
            salt,
            launch,
            pool,
            factory_core::Message::SetPaused { creator, paused },
            150_000,
        )
        .unwrap();
        assert_eq!(fixed_pool(&state, pool).trade_control.is_paused(), paused);
    }
    assert!(fixed_pool(&state, pool).trade_control.is_paused());
    for operation in [
        lbp_core::Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        lbp_core::Message::BuyWeightedExactOutput {
            membership: None,
            amount_out: 1,
            max_amount_in: 100,
        },
    ] {
        let before = state.clone();
        let result = fixed_execute_at(&mut state, pool, &key, operation, 150_000);
        assert!(
            matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains("Paused")),
            "{result:?}"
        );
        assert!(state == before);
    }
    state = fixed_config(state, 100, TREASURY, true);
    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::SetPaused {
            creator,
            paused: false,
        },
        150_000,
    )
    .unwrap();
    let before = state.clone();
    let result = fixed_execute_at(
        &mut state,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        150_000,
    );
    assert!(
        matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains("Frozen")),
        "{result:?}"
    );
    assert!(state == before);
    for paused in [false, true] {
        let before = state.clone();
        let result = factory_execute_at(
            &mut state,
            factory,
            &actors,
            &key,
            salt,
            launch,
            pool,
            factory_core::Message::SetPaused { creator, paused },
            300_000,
        );
        assert!(
            matches!(result, Err(lee::error::LeeError::OutOfValidityWindow)),
            "{result:?}"
        );
        assert!(state == before);
    }
}
