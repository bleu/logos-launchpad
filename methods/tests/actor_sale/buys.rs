use super::*;

#[test]
fn weighted_accuracy_and_vault_overflow_rejections_preserve_state_and_nonce() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (state, pool) = weighted_state(buyer);
    let mut sale = fixed_pool(&state, pool);
    if let lbp_core::PoolMode::Weighted {
        token_weight_start,
        token_weight_end,
        ..
    } = &mut sale.mode
    {
        *token_weight_start = 99 * lbp_core::WEIGHT_TOTAL / 100;
        *token_weight_end = 99 * lbp_core::WEIGHT_TOTAL / 100;
    }
    let mut inaccurate = state.clone().with_public_accounts([account(
        Actor::new(pool, LBP),
        borsh::to_vec(&sale).unwrap(),
    )]);
    fixed_rejected(
        &mut inaccurate,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactOutput {
            membership: None,
            amount_out: 300,
            max_amount_in: u128::MAX,
        },
        "MathAccuracy",
    );

    let mut overflow = state.with_public_accounts([account(
        holding(pool, COLLATERAL),
        borsh::to_vec(&TokenState::Holding {
            definition: COLLATERAL,
            owner: pool,
            kind: TokenKind::Fungible,
            balance: u128::MAX,
        })
        .unwrap(),
    )]);
    fixed_rejected(
        &mut overflow,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 50,
            min_amount_out: 324,
        },
        "Arithmetic",
    );
}

#[test]
fn weighted_output_credit_failure_rolls_back_input_records_and_nonce() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (state, pool) = weighted_state(buyer);
    let mut state = state.with_public_accounts([account(
        holding(buyer, LAUNCH),
        borsh::to_vec(&TokenState::Holding {
            definition: LAUNCH,
            owner: buyer,
            kind: TokenKind::Fungible,
            balance: u128::MAX,
        })
        .unwrap(),
    )]);
    let before = fixed_pool(&state, pool);
    fixed_rejected(
        &mut state,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 50,
            min_amount_out: 324,
        },
        "Overflow",
    );
    assert_eq!(balance(&state, buyer, COLLATERAL), 2_000);
    assert_eq!(balance(&state, buyer, LAUNCH), u128::MAX);
    assert_eq!(balance(&state, pool, COLLATERAL), 141);
    assert_eq!(balance(&state, pool, LAUNCH), 1_007);
    assert_eq!(fixed_pool(&state, pool), before);
}

#[test]
fn fixed_price_buys_deduct_live_fees_before_pricing_and_collect_to_current_treasury() {
    use lbp_core::Message::{BuyExactInput, BuyExactOutput, CollectFees};
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (mut state, pool) = fixed_state(buyer, Some(200));
    fixed_execute(
        &mut state,
        pool,
        &key,
        BuyExactInput {
            membership: None,
            amount_in: 101,
            min_amount_out: 64,
            now: 100,
        },
    )
    .unwrap();
    assert_eq!(balance(&state, buyer, LAUNCH), 64);
    assert_eq!(balance(&state, buyer, COLLATERAL), 1_899);
    let sale = fixed_pool(&state, pool);
    assert_eq!(
        (
            sale.remaining_inventory,
            sale.recorded_collateral,
            sale.accrued_fees
        ),
        (936, 97, 4)
    );

    state = fixed_config(state, 1_001, NEXT_TREASURY, false);
    fixed_execute(
        &mut state,
        pool,
        &key,
        BuyExactOutput {
            membership: None,
            amount_out: 13,
            max_amount_in: 23,
            now: 100,
        },
    )
    .unwrap();
    assert_eq!(balance(&state, buyer, LAUNCH), 77);
    assert_eq!(balance(&state, buyer, COLLATERAL), 1_876);
    let sale = fixed_pool(&state, pool);
    assert_eq!(
        (
            sale.remaining_inventory,
            sale.recorded_collateral,
            sale.accrued_fees
        ),
        (923, 117, 7)
    );
    assert_eq!(balance(&state, pool, COLLATERAL), 165);
    assert_eq!(balance(&state, pool, LAUNCH), 930);

    state = clock_state(fixed_config(state, 1_001, NEXT_TREASURY, true), 200_001);
    fixed_execute_at(&mut state, pool, &key, CollectFees, 200_001).unwrap();
    assert_eq!(balance(&state, TREASURY, COLLATERAL), 0);
    assert_eq!(balance(&state, NEXT_TREASURY, COLLATERAL), 7);
    assert_eq!(balance(&state, pool, COLLATERAL), 158);
    let sale = fixed_pool(&state, pool);
    assert_eq!((sale.recorded_collateral, sale.accrued_fees), (117, 0));
    assert_eq!(
        balance(&state, pool, COLLATERAL) - sale.recorded_collateral,
        41
    );
    assert_eq!(balance(&state, pool, LAUNCH) - sale.remaining_inventory, 7);
    let before = state.clone();
    fixed_execute_at(&mut state, pool, &key, CollectFees, 200_001).unwrap();
    assert!(state == before, "zero-fee retry changed runtime state");
}

#[test]
fn fixed_price_without_end_checks_inclusion_start_and_stops_at_sellout() {
    use lbp_core::Message::{BuyExactInput, BuyExactOutput, CollectFees};
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (mut state, pool) = fixed_state(buyer, None);
    let transaction = fixed_transaction(
        &state,
        pool,
        &key,
        BuyExactInput {
            membership: None,
            amount_in: 4,
            min_amount_out: 2,
            now: 100,
        },
    );
    let before = state.clone();
    assert!(matches!(
        state.transition_from_public_transaction(&transaction, 1, 99_999),
        Err(lee::error::LeeError::OutOfValidityWindow)
    ));
    assert!(state == before, "early buy changed runtime state");
    assert_eq!(
        state.get_account_by_id(buyer).nonce,
        before.get_account_by_id(buyer).nonce
    );
    fixed_execute(
        &mut state,
        pool,
        &key,
        BuyExactOutput {
            membership: None,
            amount_out: 1_000,
            max_amount_in: 1_552,
            now: 100,
        },
    )
    .unwrap();
    let sale = fixed_pool(&state, pool);
    assert_eq!(
        (
            sale.remaining_inventory,
            sale.recorded_collateral,
            sale.accrued_fees
        ),
        (0, 1_500, 52)
    );
    assert_eq!(balance(&state, buyer, LAUNCH), 1_000);
    assert_eq!(balance(&state, buyer, COLLATERAL), 448);
    assert_eq!(balance(&state, pool, COLLATERAL), 1_593);
    assert_eq!(balance(&state, pool, LAUNCH), 7);
    fixed_rejected(
        &mut state,
        pool,
        &key,
        BuyExactInput {
            membership: None,
            amount_in: 4,
            min_amount_out: 2,
            now: 100,
        },
        "Closed",
    );
    fixed_execute_at(&mut state, pool, &key, CollectFees, 200_001).unwrap();
    assert_eq!(balance(&state, TREASURY, COLLATERAL), 52);
    assert_eq!(balance(&state, pool, COLLATERAL), 1_541);
}

#[test]
fn fixed_price_fee_only_slippage_and_fee_consuming_dust_preserve_state_and_nonce() {
    use lbp_core::Message::{BuyExactInput, BuyExactOutput};
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (mut state, pool) = fixed_state(buyer, None);
    fixed_rejected(
        &mut state,
        pool,
        &key,
        BuyExactInput {
            membership: None,
            amount_in: 101,
            min_amount_out: 67,
            now: 100,
        },
        "Slippage",
    );
    state = fixed_config(state, 1_001, TREASURY, false);
    fixed_rejected(
        &mut state,
        pool,
        &key,
        BuyExactOutput {
            membership: None,
            amount_out: 13,
            max_amount_in: 20,
            now: 100,
        },
        "Slippage",
    );
    state = fixed_config(state, 9_999, TREASURY, false);
    fixed_rejected(
        &mut state,
        pool,
        &key,
        BuyExactInput {
            membership: None,
            amount_in: 3,
            min_amount_out: 0,
            now: 100,
        },
        "InvalidAmount",
    );
}

#[test]
fn fixed_price_deadline_slippage_and_frozen_buys_roll_back() {
    use lbp_core::Message::{BuyExactInput, BuyExactOutput};
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (mut state, pool) = fixed_state(buyer, Some(200));
    fixed_rejected(
        &mut state,
        pool,
        &key,
        BuyExactInput {
            membership: None,
            amount_in: 101,
            min_amount_out: 68,
            now: 100,
        },
        "Slippage",
    );
    fixed_rejected(
        &mut state,
        pool,
        &key,
        BuyExactOutput {
            membership: None,
            amount_out: 3,
            max_amount_in: 4,
            now: 100,
        },
        "Slippage",
    );
    state = fixed_config(state, 333, TREASURY, true);
    fixed_rejected(
        &mut state,
        pool,
        &key,
        BuyExactInput {
            membership: None,
            amount_in: 4,
            min_amount_out: 2,
            now: 100,
        },
        "Frozen",
    );
    state = fixed_config(state, 333, TREASURY, false);
    let instruction = BuyExactInput {
        membership: None,
        amount_in: 4,
        min_amount_out: 2,
        now: 100,
    };
    let transaction = fixed_transaction(&state, pool, &key, instruction.clone());
    for timestamp in [100_000, 199_999] {
        let mut admitted = state.clone();
        admitted
            .transition_from_public_transaction(&transaction, 1, timestamp)
            .unwrap();
        assert_eq!(balance(&admitted, buyer, LAUNCH), 2);
    }
    for timestamp in [99_999, 200_000, 200_001] {
        let mut rejected = state.clone();
        let before = rejected.clone();
        let nonce = rejected.get_account_by_id(buyer).nonce;
        assert!(matches!(
            rejected.transition_from_public_transaction(&transaction, 1, timestamp),
            Err(lee::error::LeeError::OutOfValidityWindow)
        ));
        assert!(
            rejected == before,
            "out-of-window buy changed runtime state"
        );
        assert_eq!(rejected.get_account_by_id(buyer).nonce, nonce);
    }
}

#[test]
fn fixed_price_output_transfer_failure_rolls_back_input_and_nonce() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (state, pool) = fixed_state(buyer, None);
    let mut state = state.with_public_accounts([account(
        holding(buyer, LAUNCH),
        borsh::to_vec(&TokenState::Holding {
            definition: COLLATERAL,
            owner: buyer,
            kind: TokenKind::Fungible,
            balance: 0,
        })
        .unwrap(),
    )]);
    fixed_rejected(
        &mut state,
        pool,
        &key,
        lbp_core::Message::BuyExactInput {
            membership: None,
            amount_in: 4,
            min_amount_out: 2,
            now: 100,
        },
        "WrongDefinition",
    );
    assert_eq!(balance(&state, buyer, COLLATERAL), 2_000);
    assert_eq!(balance(&state, pool, COLLATERAL), 41);
    let sale = fixed_pool(&state, pool);
    assert_eq!(
        (
            sale.remaining_inventory,
            sale.recorded_collateral,
            sale.accrued_fees
        ),
        (1_000, 0, 0)
    );
}

#[test]
fn timed_buy_uses_native_milliseconds_and_execution_deadline() {
    const BUILT_AT: u64 = 1_792_665_600;
    const DEADLINE: u64 = BUILT_AT + 60;
    let alice_key = PrivateKey::try_new([21; 32]).unwrap();
    let bob_key = PrivateKey::try_new([22; 32]).unwrap();
    let alice = AccountId::from(&PublicKey::new_from_private_key(&alice_key));
    let bob = AccountId::from(&PublicKey::new_from_private_key(&bob_key));
    let (state, pool) = seeded_state(alice, bob, Some(DEADLINE));
    let state = clock_state(state, BUILT_AT * 1_000);
    let transaction = buy_transaction(&state, pool, &alice_key, 200, BUILT_AT);
    let mut state = clock_state(state, (BUILT_AT + 5) * 1_000 + 123);
    state
        .transition_from_public_transaction(&transaction, 1, (BUILT_AT + 5) * 1_000 + 123)
        .expect("a delayed transaction before the deadline must execute");
    assert_eq!(balance(&state, alice, LAUNCH), 1_000);
    assert_eq!(balance(&state, alice, COLLATERAL), 88);
    assert_eq!(balance(&state, pool, LAUNCH), 3_000);
    assert_eq!(balance(&state, pool, COLLATERAL), 112);

    let transaction = buy_transaction(&state, pool, &bob_key, 200, BUILT_AT);
    let mut before_end = state.clone();
    before_end
        .transition_from_public_transaction(&transaction, 1, DEADLINE * 1_000 - 1)
        .expect("the last millisecond before expiry must remain valid");
    assert_eq!(balance(&before_end, bob, LAUNCH), 1_000);
    for timestamp in [DEADLINE * 1_000, DEADLINE * 1_000 + 1] {
        let mut late = state.clone();
        let before = late.clone();
        let nonce = late.get_account_by_id(bob).nonce;
        let rejected = late.transition_from_public_transaction(&transaction, 1, timestamp);
        assert!(matches!(
            rejected,
            Err(lee::error::LeeError::OutOfValidityWindow)
        ));
        assert!(late == before, "late buy changed runtime state");
        assert_eq!(late.get_account_by_id(bob).nonce, nonce);
    }
}

#[test]
fn trader_execute_requires_native_authority_and_a_fresh_signed_purchase() {
    let key = PrivateKey::try_new([21; 32]).unwrap();
    let other_key = PrivateKey::try_new([22; 32]).unwrap();
    let trader = AccountId::from(&PublicKey::new_from_private_key(&key));
    let other = AccountId::from(&PublicKey::new_from_private_key(&other_key));
    let (state, pool) = seeded_state(trader, other, None);
    let valid = buy_transaction(&state, pool, &key, 200, 1);
    let reject = |transaction: PublicTransaction| {
        let mut rejected = state.clone();
        let result = rejected.transition_from_public_transaction(&transaction, 1, 0);
        assert!(result.is_err(), "unauthorized purchase succeeded");
        assert!(
            rejected == state,
            "rejected purchase changed state or nonce"
        );
    };
    reject(PublicTransaction::new(
        valid.message.clone(),
        WitnessSet::for_message(&valid.message, &[&other_key]),
    ));
    reject(PublicTransaction::new(
        valid.message.clone(),
        WitnessSet::for_message(&valid.message, &[]),
    ));
    let unsigned = Message::try_new(
        Actor::new(trader, CURVE),
        valid.message.public_context.actors.clone(),
        BTreeMap::new(),
        CurveMessage::Execute {
            pool,
            instruction: Box::new(CurveMessage::SwapExactOutput {
                membership: None,
                amount_out: 1_000,
                max_amount_in: 200,
                token_in: COLLATERAL,
                now: 1,
            }),
        },
    )
    .unwrap();
    let mut admitted = unsigned.clone();
    admitted
        .admission_evidence
        .push(lee::PublicAccountEvidence::Key(
            PublicKey::new_from_private_key(&key),
        ));
    reject(PublicTransaction::new(
        admitted.clone(),
        WitnessSet::for_message(&admitted, &[]),
    ));
    let seed = lee_core::program::PdaSeed::new([31; 32]);
    let pda = AccountId::for_public_pda(&CURVE, &seed);
    let mut delegated = unsigned;
    delegated.execution.root.to = Actor::new(pda, CURVE);
    delegated
        .public_context
        .actors
        .insert(Actor::new(pda, CURVE));
    delegated
        .admission_evidence
        .push(lee::PublicAccountEvidence::Pda {
            program: CURVE,
            seed,
        });
    reject(PublicTransaction::new(
        delegated.clone(),
        WitnessSet::for_message(&delegated, &[]),
    ));
    let mut changed = valid.clone();
    changed.message.execution.root.message = borsh::to_vec(&CurveMessage::Execute {
        pool,
        instruction: Box::new(CurveMessage::SwapExactOutput {
            membership: None,
            amount_out: 999,
            max_amount_in: 200,
            token_in: COLLATERAL,
            now: 1,
        }),
    })
    .unwrap();
    reject(changed);
    let mut direct = valid.message.clone();
    direct.execution.root.to = Actor::new(pool, CURVE);
    direct.execution.root.message = borsh::to_vec(&CurveMessage::SwapExactOutput {
        membership: None,
        amount_out: 1_000,
        max_amount_in: 200,
        token_in: COLLATERAL,
        now: 1,
    })
    .unwrap();
    let witnesses = WitnessSet::for_message(&direct, &[&key]);
    reject(PublicTransaction::new(direct, witnesses));
    let mut successful = state;
    successful
        .transition_from_public_transaction(&valid, 1, 0)
        .unwrap();
    let after = successful.clone();
    assert!(
        successful
            .transition_from_public_transaction(&valid, 1, 0)
            .is_err()
    );
    assert!(successful == after, "replay changed state");
    let fresh = buy_transaction(&successful, pool, &other_key, 200, 1);
    successful
        .transition_from_public_transaction(&fresh, 1, 0)
        .unwrap();
    assert_eq!(balance(&successful, trader, LAUNCH), 1_000);
    assert_eq!(balance(&successful, other, LAUNCH), 1_000);
}

#[test]
fn exact_output_buy_executes_real_curve_and_token_guests() {
    let alice_key = PrivateKey::try_new([21; 32]).unwrap();
    let bob_key = PrivateKey::try_new([22; 32]).unwrap();
    let alice = AccountId::from(&PublicKey::new_from_private_key(&alice_key));
    let bob = AccountId::from(&PublicKey::new_from_private_key(&bob_key));
    let (mut state, pool) = seeded_state(alice, bob, None);
    buy(&mut state, pool, &alice_key, 200).unwrap();
    assert_eq!(balance(&state, alice, LAUNCH), 1_000);
    assert_eq!(balance(&state, alice, COLLATERAL), 88);
    assert_eq!(balance(&state, pool, LAUNCH), 3_000);
    assert_eq!(balance(&state, pool, COLLATERAL), 112);

    let before = state.clone();
    let nonce = state.get_account_by_id(bob).nonce;
    let rejected = buy(&mut state, pool, &bob_key, 112);
    assert!(
        matches!(rejected, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("InvalidPool"))
    );
    assert!(state == before, "rejected buy changed runtime state");
    assert_eq!(state.get_account_by_id(bob).nonce, nonce);

    buy(&mut state, pool, &bob_key, 200).unwrap();
    assert_eq!(balance(&state, bob, LAUNCH), 1_000);
    assert_eq!(balance(&state, bob, COLLATERAL), 62);
    assert_eq!(balance(&state, pool, LAUNCH), 2_000);
    assert_eq!(balance(&state, pool, COLLATERAL), 250);
    assert_eq!(balance(&state, alice, COLLATERAL), 88);
    let updated =
        PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(CURVE)).unwrap();
    assert_eq!(
        (updated.pool.real_reserve0, updated.pool.real_reserve1),
        (2_000, 250)
    );
}

#[test]
fn weighted_buy_limits_accept_equality_and_reject_one_beyond() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (state, pool) = weighted_state(buyer);
    for (equal, beyond, output, input) in [
        (
            lbp_core::Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: 324,
            },
            lbp_core::Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: 325,
            },
            324,
            50,
        ),
        (
            lbp_core::Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 100,
                max_amount_in: 13,
            },
            lbp_core::Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 100,
                max_amount_in: 12,
            },
            100,
            13,
        ),
    ] {
        let mut rejected = state.clone();
        fixed_rejected(&mut rejected, pool, &key, beyond, "Slippage");
        fixed_execute(&mut rejected, pool, &key, equal).unwrap();
        assert_eq!(balance(&rejected, buyer, LAUNCH), output);
        assert_eq!(balance(&rejected, buyer, COLLATERAL), 2_000 - input);
        let sale = fixed_pool(&rejected, pool);
        let fee = if input == 50 { 2 } else { 1 };
        assert_eq!(sale.accrued_fees, fee);
        assert_eq!(sale.recorded_collateral, 100 + input - fee);
        assert!(
            matches!(sale.mode, lbp_core::PoolMode::Weighted { pricing_collateral, .. } if pricing_collateral == sale.recorded_collateral)
        );
        assert_eq!(
            balance(&rejected, pool, COLLATERAL),
            sale.recorded_collateral + sale.accrued_fees + 41
        );
    }
}

#[test]
fn lbp_buys_reject_invalid_saved_ending_matrix_without_factory_checks() {
    use curve_core::EndingPolicy;
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    for weighted in [false, true] {
        let (state, pool) = if weighted {
            weighted_state(buyer)
        } else {
            fixed_state(buyer, Some(200))
        };
        let buys = if weighted {
            [
                lbp_core::Message::BuyWeightedExactInput {
                    membership: None,
                    amount_in: 50,
                    min_amount_out: 1,
                },
                lbp_core::Message::BuyWeightedExactOutput {
                    membership: None,
                    amount_out: 10,
                    max_amount_in: 100,
                },
            ]
        } else {
            [
                lbp_core::Message::BuyExactInput {
                    membership: None,
                    amount_in: 50,
                    min_amount_out: 1,
                    now: 100,
                },
                lbp_core::Message::BuyExactOutput {
                    membership: None,
                    amount_out: 10,
                    max_amount_in: 100,
                    now: 100,
                },
            ]
        };
        let mut invalid_terms = vec![
            (EndingPolicy::Manual, None),
            (EndingPolicy::Depletion, Some(200)),
            (EndingPolicy::Scheduled, None),
            (EndingPolicy::Either, None),
        ];
        if weighted {
            invalid_terms.push((EndingPolicy::Depletion, None));
        }
        for buy in buys {
            let transaction = fixed_transaction(&state, pool, &key, buy);
            for &(ending_policy, end_timestamp) in &invalid_terms {
                let mut sale = fixed_pool(&state, pool);
                sale.ending_policy = ending_policy;
                sale.end_timestamp = end_timestamp;
                let mut malformed = state.clone().with_public_accounts([account(
                    Actor::new(pool, LBP),
                    borsh::to_vec(&sale).unwrap(),
                )]);
                let before = malformed.clone();
                let nonce = malformed.get_account_by_id(buyer).nonce;
                let rejected =
                    malformed.transition_from_public_transaction(&transaction, 1, 100_000);
                let expected = if ending_policy == EndingPolicy::Manual
                    || (weighted && ending_policy == EndingPolicy::Depletion)
                {
                    "InvalidState"
                } else {
                    "InvalidTerms"
                };
                assert!(
                    matches!(&rejected, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains(expected)),
                    "expected pool policy validation, got {rejected:?}",
                );
                assert!(
                    malformed == before,
                    "pool policy bypass changed runtime state"
                );
                assert_eq!(malformed.get_account_by_id(buyer).nonce, nonce);
            }
        }
    }
}

#[test]
fn weighted_buys_reject_unbacked_claims_and_mismatched_pricing_records() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (state, pool) = weighted_state(buyer);
    for mismatch in [false, true] {
        let mut sale = fixed_pool(&state, pool);
        if mismatch {
            sale.recorded_collateral += 1;
        } else {
            sale.accrued_fees = 42;
        }
        let malformed = state.clone().with_public_accounts([account(
            Actor::new(pool, LBP),
            borsh::to_vec(&sale).unwrap(),
        )]);
        for instruction in [
            lbp_core::Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: 1,
            },
            lbp_core::Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 100,
                max_amount_in: 100,
            },
        ] {
            let mut rejected = malformed.clone();
            fixed_rejected(
                &mut rejected,
                pool,
                &key,
                instruction,
                if mismatch {
                    "InvalidState"
                } else {
                    "Underfunded"
                },
            );
        }
    }
}

#[test]
fn block_ceiling_signed_buys_share_allowance_and_failed_transfers_roll_back() {
    for weighted in [false, true] {
        let key = PrivateKey::try_new([23; 32]).unwrap();
        let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
        let (mut state, pool) = if weighted {
            weighted_state(buyer)
        } else {
            fixed_state(buyer, Some(200))
        };
        let mut sale = fixed_pool(&state, pool);
        sale.block_allocation = lbp_core::BlockAllocation::new(Some(6)).unwrap();
        state = state.with_public_accounts([account(
            Actor::new(pool, LBP),
            borsh::to_vec(&sale).unwrap(),
        )]);
        state = fixed_config(state, 200, TREASURY, false);
        let message = if weighted {
            lbp_core::Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 2,
                max_amount_in: 100,
            }
        } else {
            lbp_core::Message::BuyExactOutput {
                membership: None,
                amount_out: 2,
                max_amount_in: 100,
                now: 100,
            }
        };
        let mut overflow =
            state
                .clone()
                .with_public_accounts([token_holding(buyer, LAUNCH, u128::MAX)]);
        let transaction = fixed_transaction(&overflow, pool, &key, message.clone());
        let before = overflow.clone();
        assert!(
            overflow
                .transition_from_public_transaction(&transaction, 2, 100_000)
                .is_err()
        );
        assert!(overflow == before);
        for sold in [2, 4, 6] {
            let transaction = fixed_transaction(&state, pool, &key, message.clone());
            state
                .transition_from_public_transaction(&transaction, 2, 100_000)
                .unwrap();
            let saved = fixed_pool(&state, pool);
            assert_eq!(saved.block_allocation.tokens_sold, sold);
            assert!(saved.accrued_fees > 0);
        }
        let excess = if weighted {
            lbp_core::Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 5,
                min_amount_out: 0,
            }
        } else {
            lbp_core::Message::BuyExactInput {
                membership: None,
                amount_in: 5,
                min_amount_out: 0,
                now: 100,
            }
        };
        let transaction = fixed_transaction(&state, pool, &key, excess);
        let before = state.clone();
        assert!(
            state
                .transition_from_public_transaction(&transaction, 2, 100_000)
                .is_err()
        );
        assert!(state == before);
        state = state.with_public_accounts([account(
            clock_core::clock_actor(),
            borsh::to_vec(&clock_core::ClockAccountData {
                block_id: 2,
                timestamp: 100_000,
            })
            .unwrap(),
        )]);
        let transaction = fixed_transaction(&state, pool, &key, message);
        let mut wrong_block = state.clone();
        assert!(
            wrong_block
                .transition_from_public_transaction(&transaction, 4, 100_000)
                .is_err()
        );
        assert!(wrong_block == state);
        state
            .transition_from_public_transaction(&transaction, 3, 100_000)
            .unwrap();
        assert_eq!(fixed_pool(&state, pool).block_allocation.tokens_sold, 2);
        assert_eq!(
            fixed_pool(&state, pool).block_allocation.observed_block,
            Some(2)
        );
    }
}
