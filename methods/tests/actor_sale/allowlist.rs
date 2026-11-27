use super::*;

#[test]
fn curve_allowlist_signed_buy_checks_actual_trader_and_reuses_membership() {
    let key = PrivateKey::try_new([21; 32]).unwrap();
    let other_key = PrivateKey::try_new([22; 32]).unwrap();
    let trader = AccountId::from(&PublicKey::new_from_private_key(&key));
    let other = AccountId::from(&PublicKey::new_from_private_key(&other_key));
    let (state, pool) = seeded_state(trader, other, None);
    let mut sale =
        PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(CURVE)).unwrap();
    let (policy, path) = allowlist_for(pool, trader);
    sale.buy_policy = policy.clone();
    let mut state = state.with_public_accounts([account(
        Actor::new(pool, CURVE),
        borsh::to_vec(&sale).unwrap(),
    )]);
    let transaction = |state: &V03State, key: &PrivateKey, membership| {
        let mut transaction = buy_transaction(state, pool, key, 200, 1);
        transaction.message.execution.root.message = borsh::to_vec(&CurveMessage::Execute {
            pool,
            instruction: Box::new(CurveMessage::SwapExactInput {
                amount_in: 50,
                min_amount_out: 1,
                token_in: COLLATERAL,
                now: 1,
                membership,
            }),
        })
        .unwrap();
        transaction.witness_set = WitnessSet::for_message(&transaction.message, &[key]);
        transaction
    };
    for (key, membership) in [(&key, None), (&other_key, Some(path.clone()))] {
        let before = state.clone();
        assert!(
            state
                .transition_from_public_transaction(&transaction(&state, key, membership), 1, 0)
                .is_err()
        );
        assert!(state == before);
    }
    let original = transaction(&state, &key, Some(path.clone()));
    let (diff, charge) = lee::ValidatedStateDiff::from_public_transaction_with_cycle_budget(
        &original, &state, 1, 0, 10_000_000,
    )
    .unwrap();
    eprintln!("curve allowlist exact-input cycles: {}", charge.cycles);
    assert!(charge.cycles < 10_000_000);
    state.apply_state_diff(diff).unwrap();
    let first_output = balance(&state, trader, LAUNCH);
    assert!(first_output > 0);
    let before = state.clone();
    assert!(
        state
            .transition_from_public_transaction(&original, 1, 0)
            .is_err()
    );
    assert!(state == before);
    let mut failed = state.clone().with_public_accounts([account(
        holding(trader, LAUNCH),
        borsh::to_vec(&TokenState::Holding {
            owner: trader,
            definition: LAUNCH,
            kind: TokenKind::Fungible,
            balance: u128::MAX,
        })
        .unwrap(),
    )]);
    let before = failed.clone();
    let late = transaction(&failed, &key, Some(path.clone()));
    let rejected = failed.transition_from_public_transaction(&late, 1, 0);
    assert!(
        matches!(&rejected, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("Overflow")),
        "expected late output overflow, got {rejected:?}"
    );
    assert!(
        failed == before,
        "late output failure changed pool, balances or nonce"
    );
    let mut fresh = buy_transaction(&state, pool, &key, 200, 1);
    fresh.message.execution.root.message = borsh::to_vec(&CurveMessage::Execute {
        pool,
        instruction: Box::new(CurveMessage::SwapExactOutput {
            amount_out: 1_000,
            max_amount_in: 200,
            token_in: COLLATERAL,
            now: 1,
            membership: Some(path),
        }),
    })
    .unwrap();
    fresh.witness_set = WitnessSet::for_message(&fresh.message, &[&key]);
    let (diff, charge) = lee::ValidatedStateDiff::from_public_transaction_with_cycle_budget(
        &fresh, &state, 1, 0, 10_000_000,
    )
    .unwrap();
    eprintln!("curve allowlist exact-output cycles: {}", charge.cycles);
    state.apply_state_diff(diff).unwrap();
    assert!(balance(&state, trader, LAUNCH) > first_output);
    assert!(balance(&state, trader, COLLATERAL) < 150);
    assert_eq!(balance(&state, other, LAUNCH), 0);
    let stored =
        PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(CURVE)).unwrap();
    assert_eq!(stored.buy_policy, policy);
}

#[test]
fn curve_factory_locks_allowlist_before_setup_and_buys_with_the_same_policy() {
    for (ending_policy, sell_fee_bps) in [
        (curve_core::EndingPolicy::Scheduled, 0),
        (curve_core::EndingPolicy::Depletion, 0),
        (curve_core::EndingPolicy::Either, 0),
        (curve_core::EndingPolicy::Either, 777),
    ] {
        curve_factory_setup_case(ending_policy, sell_fee_bps, true);
    }
}

#[test]
fn fixed_allowlist_signed_buy_requires_trader_control_and_keeps_repeat_permission() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let other_key = PrivateKey::try_new([22; 32]).unwrap();
    let trader = AccountId::from(&PublicKey::new_from_private_key(&key));
    let other = AccountId::from(&PublicKey::new_from_private_key(&other_key));
    let (state, pool) = fixed_state(trader, Some(200));
    let state = state.with_public_accounts([COLLATERAL, LAUNCH].map(|definition| {
        account(
            holding(other, definition),
            borsh::to_vec(&TokenState::Holding {
                owner: other,
                definition,
                kind: TokenKind::Fungible,
                balance: if definition == COLLATERAL { 2_000 } else { 0 },
            })
            .unwrap(),
        )
    }));
    let mut sale = fixed_pool(&state, pool);
    let (policy, path) = allowlist_for(pool, trader);
    sale.buy_policy = policy.clone();
    let mut state = state.with_public_accounts([account(
        Actor::new(pool, LBP),
        borsh::to_vec(&sale).unwrap(),
    )]);
    let buy = lbp_core::Message::BuyExactInput {
        amount_in: 50,
        min_amount_out: 32,
        now: 100,
        membership: Some(path.clone()),
    };
    let valid = fixed_transaction(&state, pool, &key, buy.clone());
    let reject = |transaction: PublicTransaction| {
        let mut rejected = state.clone();
        assert!(
            rejected
                .transition_from_public_transaction(&transaction, 1, 100_000)
                .is_err()
        );
        assert!(rejected == state, "rejection changed state or nonce");
    };
    reject(fixed_transaction(
        &state,
        pool,
        &key,
        lbp_core::Message::BuyExactInput {
            amount_in: 50,
            min_amount_out: 32,
            now: 100,
            membership: None,
        },
    ));
    reject(fixed_transaction(&state, pool, &other_key, buy.clone()));
    reject(PublicTransaction::new(
        valid.message.clone(),
        WitnessSet::for_message(&valid.message, &[&other_key]),
    ));
    let mut changed = valid.clone();
    changed.message.execution.root.message = borsh::to_vec(&lbp_core::Message::Execute {
        pool,
        instruction: Box::new(lbp_core::Message::BuyExactInput {
            amount_in: 51,
            min_amount_out: 32,
            now: 100,
            membership: Some(path.clone()),
        }),
    })
    .unwrap();
    reject(changed);
    let mut direct = valid.message.clone();
    direct.execution.root.to = Actor::new(pool, LBP);
    direct.execution.root.message = borsh::to_vec(&buy).unwrap();
    let witnesses = WitnessSet::for_message(&direct, &[&key]);
    reject(PublicTransaction::new(direct, witnesses));
    let mut unsigned = Message::try_new(
        Actor::new(trader, LBP),
        valid.message.public_context.actors.clone(),
        BTreeMap::new(),
        lbp_core::Message::Execute {
            pool,
            instruction: Box::new(buy.clone()),
        },
    )
    .unwrap();
    unsigned
        .admission_evidence
        .push(lee::PublicAccountEvidence::Key(
            PublicKey::new_from_private_key(&key),
        ));
    reject(PublicTransaction::new(
        unsigned.clone(),
        WitnessSet::for_message(&unsigned, &[]),
    ));
    let (diff, charge) = lee::ValidatedStateDiff::from_public_transaction_with_cycle_budget(
        &valid, &state, 1, 100_000, 10_000_000,
    )
    .unwrap();
    eprintln!("fixed allowlist exact-input cycles: {}", charge.cycles);
    state.apply_state_diff(diff).unwrap();
    assert_eq!(balance(&state, trader, LAUNCH), 32);
    let before = state.clone();
    assert!(
        state
            .transition_from_public_transaction(&valid, 1, 100_000)
            .is_err()
    );
    assert!(state == before);
    let fresh = fixed_transaction(
        &state,
        pool,
        &key,
        lbp_core::Message::BuyExactOutput {
            amount_out: 10,
            max_amount_in: 16,
            now: 100,
            membership: Some(path.clone()),
        },
    );
    let (diff, charge) = lee::ValidatedStateDiff::from_public_transaction_with_cycle_budget(
        &fresh, &state, 1, 100_000, 10_000_000,
    )
    .unwrap();
    eprintln!("fixed allowlist exact-output cycles: {}", charge.cycles);
    state.apply_state_diff(diff).unwrap();
    assert_eq!(balance(&state, trader, LAUNCH), 42);
    assert_eq!(balance(&state, trader, COLLATERAL), 1934);
    assert_eq!(fixed_pool(&state, pool).buy_policy, policy);
    let mut failed = state.with_public_accounts([account(
        holding(trader, LAUNCH),
        borsh::to_vec(&TokenState::Holding {
            owner: trader,
            definition: LAUNCH,
            kind: TokenKind::Fungible,
            balance: u128::MAX,
        })
        .unwrap(),
    )]);
    fixed_rejected(&mut failed, pool, &key, buy, "Overflow");
}

#[test]
fn fixed_factory_preserves_allowlist_through_setup_and_signed_purchase() {
    let FixedFactoryEnv {
        mut state,
        pool,
        launch,
        key,
        path,
        ..
    } = fixed_factory_env(
        Some(200),
        factory_core::CreatorAllocationRelease::AfterSetup,
    );
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (policy, _) = allowlist_for(pool, creator);
    let stored = fixed_pool(&state, pool);
    assert!(stored.funded);
    assert_eq!(stored.buy_policy, policy);
    assert_eq!(stored.ending_policy, lbp_core::EndingPolicy::Either);
    assert_eq!(balance(&state, creator, launch), 200);
    let transaction = fixed_transaction(
        &state,
        pool,
        &key,
        lbp_core::Message::BuyExactInput {
            membership: Some(path),
            amount_in: 50,
            min_amount_out: 32,
            now: 100,
        },
    );
    let (diff, charge) = lee::ValidatedStateDiff::from_public_transaction_with_cycle_budget(
        &transaction,
        &state,
        1,
        100_000,
        10_000_000,
    )
    .unwrap();
    eprintln!("created fixed allowlist buy cycles: {}", charge.cycles);
    state.apply_state_diff(diff).unwrap();
    assert_eq!(balance(&state, creator, launch), 232);
    assert_eq!(balance(&state, creator, COLLATERAL), 950);
    assert_eq!(balance(&state, pool, COLLATERAL), 50);
    assert_eq!(fixed_pool(&state, pool).buy_policy, policy);
}

#[test]
fn weighted_allowlist_signed_buy_checks_control_replay_reuse_and_rollback() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let other_key = PrivateKey::try_new([22; 32]).unwrap();
    let trader = AccountId::from(&PublicKey::new_from_private_key(&key));
    let other = AccountId::from(&PublicKey::new_from_private_key(&other_key));
    let (state, pool) = weighted_state(trader);
    let (policy, path) = allowlist_for(pool, trader);
    let mut sale = fixed_pool(&state, pool);
    sale.buy_policy = policy.clone();
    let mut state = state.with_public_accounts([account(
        Actor::new(pool, LBP),
        borsh::to_vec(&sale).unwrap(),
    )]);
    state = state.with_public_accounts([COLLATERAL, LAUNCH].map(|definition| {
        account(
            holding(other, definition),
            borsh::to_vec(&TokenState::Holding {
                owner: other,
                definition,
                kind: TokenKind::Fungible,
                balance: if definition == COLLATERAL { 2_000 } else { 0 },
            })
            .unwrap(),
        )
    }));
    let buy = lbp_core::Message::BuyWeightedExactInput {
        membership: Some(path.clone()),
        amount_in: 50,
        min_amount_out: 324,
    };
    let valid = fixed_transaction(&state, pool, &key, buy.clone());
    for (key, membership) in [(&key, None), (&other_key, Some(path.clone()))] {
        fixed_rejected(
            &mut state,
            pool,
            key,
            lbp_core::Message::BuyWeightedExactInput {
                membership,
                amount_in: 50,
                min_amount_out: 324,
            },
            "Unauthorized",
        );
    }
    let reject = |transaction: PublicTransaction| {
        let mut rejected = state.clone();
        assert!(
            rejected
                .transition_from_public_transaction(&transaction, 1, 100_000)
                .is_err()
        );
        assert!(
            rejected == state,
            "unauthorized weighted purchase changed state or nonce"
        );
    };
    reject(PublicTransaction::new(
        valid.message.clone(),
        WitnessSet::for_message(&valid.message, &[&other_key]),
    ));
    let mut changed = valid.clone();
    changed.message.execution.root.message = borsh::to_vec(&lbp_core::Message::Execute {
        pool,
        instruction: Box::new(lbp_core::Message::BuyWeightedExactInput {
            membership: Some(path.clone()),
            amount_in: 51,
            min_amount_out: 324,
        }),
    })
    .unwrap();
    reject(changed);
    let mut direct = valid.message.clone();
    direct.execution.root.to = Actor::new(pool, LBP);
    direct.execution.root.message = borsh::to_vec(&buy).unwrap();
    let witnesses = WitnessSet::for_message(&direct, &[&key]);
    reject(PublicTransaction::new(direct, witnesses));
    let mut unsigned = Message::try_new(
        Actor::new(trader, LBP),
        valid.message.public_context.actors.clone(),
        BTreeMap::new(),
        lbp_core::Message::Execute {
            pool,
            instruction: Box::new(buy.clone()),
        },
    )
    .unwrap();
    unsigned
        .admission_evidence
        .push(lee::PublicAccountEvidence::Key(
            PublicKey::new_from_private_key(&key),
        ));
    reject(PublicTransaction::new(
        unsigned.clone(),
        WitnessSet::for_message(&unsigned, &[]),
    ));
    let seed = lee_core::program::PdaSeed::new([31; 32]);
    let pda = AccountId::for_public_pda(&LBP, &seed);
    unsigned.execution.root.to = Actor::new(pda, LBP);
    unsigned.public_context.actors.insert(Actor::new(pda, LBP));
    unsigned
        .admission_evidence
        .push(lee::PublicAccountEvidence::Pda { program: LBP, seed });
    reject(PublicTransaction::new(
        unsigned.clone(),
        WitnessSet::for_message(&unsigned, &[]),
    ));
    let (diff, charge) = lee::ValidatedStateDiff::from_public_transaction_with_cycle_budget(
        &valid, &state, 1, 100_000, 10_000_000,
    )
    .unwrap();
    eprintln!(
        "weighted allowlist exact-input cycles (upstream clock): {}",
        charge.cycles
    );
    state.apply_state_diff(diff).unwrap();
    assert_eq!(balance(&state, trader, LAUNCH), 324);
    let before = state.clone();
    assert!(
        state
            .transition_from_public_transaction(&valid, 1, 100_000)
            .is_err()
    );
    assert!(state == before);
    let fresh = fixed_transaction(
        &state,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactOutput {
            membership: Some(path.clone()),
            amount_out: 100,
            max_amount_in: 27,
        },
    );
    let (diff, charge) = lee::ValidatedStateDiff::from_public_transaction_with_cycle_budget(
        &fresh, &state, 1, 100_000, 10_000_000,
    )
    .unwrap();
    eprintln!(
        "weighted allowlist exact-output cycles (upstream clock): {}",
        charge.cycles
    );
    state.apply_state_diff(diff).unwrap();
    assert_eq!(balance(&state, trader, LAUNCH), 424);
    assert_eq!(balance(&state, trader, COLLATERAL), 1923);
    assert_eq!(balance(&state, other, LAUNCH), 0);
    assert_eq!(fixed_pool(&state, pool).buy_policy, policy);
    let mut failed = state.with_public_accounts([account(
        holding(trader, LAUNCH),
        borsh::to_vec(&TokenState::Holding {
            owner: trader,
            definition: LAUNCH,
            kind: TokenKind::Fungible,
            balance: u128::MAX,
        })
        .unwrap(),
    )]);
    fixed_rejected(
        &mut failed,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactInput {
            membership: Some(path),
            amount_in: 50,
            min_amount_out: 1,
        },
        "Overflow",
    );
}
