use super::*;

#[test]
fn signed_curve_trade_events_match_both_buy_forms_and_sells_with_actual_token_effects() {
    for (sell, exact_output) in [(false, false), (false, true), (true, false)] {
        let (mut state, pool, key) = curve_event_env(None);
        let trader = AccountId::from(&PublicKey::new_from_private_key(&key));
        if sell {
            curve_execute_events_at(
                &mut state,
                pool,
                &key,
                CurveMessage::SwapExactInput {
                    membership: None,
                    token_in: COLLATERAL,
                    amount_in: 100,
                    min_amount_out: 1,
                    now: 1,
                },
                1_000,
            )
            .unwrap();
        }
        let token_in = if sell { LAUNCH } else { COLLATERAL };
        let token_out = if sell { COLLATERAL } else { LAUNCH };
        let before_in = balance(&state, trader, token_in);
        let before_out = balance(&state, trader, token_out);
        let mut preview =
            PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(CURVE))
                .unwrap()
                .pool;
        let side = if sell {
            pool::TokenSide::Token0
        } else {
            pool::TokenSide::Token1
        };
        let (instruction, outcome) = if exact_output {
            let amount_out = if sell { 5 } else { 50 };
            (
                CurveMessage::SwapExactOutput {
                    membership: None,
                    token_in,
                    amount_out,
                    max_amount_in: 1_000,
                    now: 1,
                },
                preview
                    .swap_exact_output(pool::ExactOutputSwap {
                        token_in: side,
                        amount_out,
                        max_amount_in: 1_000,
                        pool_fee_bps: 0,
                        protocol_fee_bps: 1_000,
                        now: 1,
                    })
                    .unwrap(),
            )
        } else {
            (
                CurveMessage::SwapExactInput {
                    membership: None,
                    token_in,
                    amount_in: 100,
                    min_amount_out: 1,
                    now: 1,
                },
                preview
                    .swap_exact_input(pool::ExactInputSwap {
                        token_in: side,
                        amount_in: 100,
                        min_amount_out: 1,
                        pool_fee_bps: 0,
                        protocol_fee_bps: 1_000,
                        now: 1,
                    })
                    .unwrap(),
            )
        };
        let events = curve_execute_events_at(&mut state, pool, &key, instruction, 1_000).unwrap();
        let decoded = sale_events(&events, CURVE, pool_subject(pool));
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].engine, PricingEngineV1::Curve);
        assert_eq!(
            decoded[0].kind,
            SaleEventKindV1::Trade {
                trader: *trader.value(),
                form: if exact_output {
                    TradeFormV1::ExactOutput
                } else {
                    TradeFormV1::ExactInput
                },
                token_in: *token_in.value(),
                token_out: *token_out.value(),
                amount_in: outcome.amount_in,
                pricing_input: outcome.effective_amount_in,
                gross_output: outcome.raw_amount_out,
                amount_out: outcome.amount_out,
                fee_definition: *COLLATERAL.value(),
                fee_amount: outcome.protocol_fee,
                fee_bps: 1_000,
                retained_fee: 0,
                sell_fee_bps: 0,
                pricing_time: None,
            }
        );
        assert_eq!(
            before_in - balance(&state, trader, token_in),
            outcome.amount_in
        );
        assert_eq!(
            balance(&state, trader, token_out) - before_out,
            outcome.amount_out
        );
        assert!(sale_events(&events, LBP, pool_subject(pool)).is_empty());
        assert!(sale_events(&events, CURVE, pool_subject(AccountId::new([255; 32]))).is_empty());
    }
}

#[test]
fn signed_lbp_trade_events_match_both_forms_engine_and_accounting_deltas() {
    for weighted in [false, true] {
        for exact_output in [false, true] {
            let key = PrivateKey::try_new([21; 32]).unwrap();
            let trader = AccountId::from(&PublicKey::new_from_private_key(&key));
            let (mut state, pool) = if weighted {
                weighted_state(trader)
            } else {
                fixed_state(trader, Some(200))
            };
            let initial = fixed_pool(&state, pool);
            let before_in = balance(&state, trader, COLLATERAL);
            let before_out = balance(&state, trader, LAUNCH);
            let instruction = match (weighted, exact_output) {
                (false, false) => lbp_core::Message::BuyExactInput {
                    membership: None,
                    amount_in: 101,
                    min_amount_out: 1,
                    now: 100,
                },
                (false, true) => lbp_core::Message::BuyExactOutput {
                    membership: None,
                    amount_out: 10,
                    max_amount_in: 200,
                    now: 100,
                },
                (true, false) => lbp_core::Message::BuyWeightedExactInput {
                    membership: None,
                    amount_in: 50,
                    min_amount_out: 1,
                },
                (true, true) => lbp_core::Message::BuyWeightedExactOutput {
                    membership: None,
                    amount_out: 10,
                    max_amount_in: 200,
                },
            };
            let events =
                fixed_execute_events_at(&mut state, pool, &key, instruction, 100_000).unwrap();
            let updated = fixed_pool(&state, pool);
            let amount_in = before_in - balance(&state, trader, COLLATERAL);
            let amount_out = balance(&state, trader, LAUNCH) - before_out;
            let fee = updated.accrued_fees - initial.accrued_fees;
            let net = updated.recorded_collateral - initial.recorded_collateral;
            assert_eq!(amount_in, net + fee);
            let decoded = sale_events(&events, LBP, pool_subject(pool));
            assert_eq!(decoded.len(), 1);
            assert_eq!(
                decoded[0].engine,
                if weighted {
                    PricingEngineV1::Weighted
                } else {
                    PricingEngineV1::FixedPrice
                }
            );
            assert_eq!(
                decoded[0].kind,
                SaleEventKindV1::Trade {
                    trader: *trader.value(),
                    form: if exact_output {
                        TradeFormV1::ExactOutput
                    } else {
                        TradeFormV1::ExactInput
                    },
                    token_in: *COLLATERAL.value(),
                    token_out: *LAUNCH.value(),
                    amount_in,
                    pricing_input: net,
                    gross_output: amount_out,
                    amount_out,
                    fee_definition: *COLLATERAL.value(),
                    fee_amount: fee,
                    fee_bps: 333,
                    retained_fee: 0,
                    sell_fee_bps: 0,
                    pricing_time: if weighted { Some(100) } else { None },
                }
            );
            let checkpoints: Vec<_> = events
                .iter()
                .filter(|event| {
                    event.account_id == LBP
                        && event.event.selector
                            == event_selector(curve_core::events::WeightedCheckpoint::SELECTOR_NAME)
                })
                .collect();
            assert_eq!(checkpoints.len(), usize::from(weighted));
            if weighted {
                let checkpoint: curve_core::events::WeightedCheckpoint =
                    borsh::from_slice(&checkpoints[0].event.data).unwrap();
                assert_eq!(
                    (checkpoint.time, checkpoint.token_weight),
                    (100, lbp_core::WEIGHT_TOTAL / 2)
                );
                let checkpoint_position = events
                    .iter()
                    .position(|event| {
                        event.account_id == LBP
                            && event.event.selector
                                == event_selector(
                                    curve_core::events::WeightedCheckpoint::SELECTOR_NAME,
                                )
                    })
                    .unwrap();
                let trade_position = events
                    .iter()
                    .position(|event| {
                        event.account_id == LBP
                            && event.event.selector == event_selector(SaleEventV1::SELECTOR_NAME)
                    })
                    .unwrap();
                assert!(checkpoint_position < trade_position);
            }
        }
    }
}

#[test]
fn signed_pause_events_report_changed_controls_and_omit_authenticated_noops() {
    let (mut state, pool, key) = curve_event_env(None);
    for (paused, changed) in [(true, true), (true, false), (false, true), (false, false)] {
        let events = curve_execute_events_at(
            &mut state,
            pool,
            &key,
            CurveMessage::SetPaused { paused },
            1_000,
        )
        .unwrap();
        assert_eq!(
            sale_event_kinds(&events, CURVE, pool_subject(pool)),
            if changed {
                vec![SaleEventKindV1::PauseChanged { paused }]
            } else {
                vec![]
            }
        );
        if !changed {
            assert!(events.is_empty());
        }
    }
    for weighted in [false, true] {
        let mut env = lbp_graduation_env(weighted, true, vec![], lbp_kind(weighted));
        env.state = clock_state(env.state, 299_000);
        let creator = env.creator();
        for (paused, changed) in [(true, true), (true, false), (false, true), (false, false)] {
            let events = env
                .execute_events(
                    factory_core::Message::SetPaused { creator, paused },
                    299_000,
                )
                .unwrap();
            assert_eq!(
                sale_event_kinds(&events, LBP, pool_subject(env.pool)),
                if changed {
                    vec![SaleEventKindV1::PauseChanged { paused }]
                } else {
                    vec![]
                }
            );
            assert!(
                sale_events(
                    &events,
                    LBP_FACTORY_PROGRAM,
                    launch_subject(env.factory, env.pool)
                )
                .is_empty()
            );
            let legacy: Vec<_> = events
                .iter()
                .filter(|event| {
                    event.account_id == LBP
                        && event.event.selector
                            == event_selector(WeightedPauseChanged::SELECTOR_NAME)
                })
                .collect();
            assert_eq!(legacy.len(), usize::from(weighted && changed));
            if let Some(event) = legacy.first() {
                assert_eq!(
                    borsh::from_slice::<WeightedPauseChanged>(&event.event.data)
                        .unwrap()
                        .paused,
                    paused
                );
            }
            if !changed {
                assert!(events.is_empty());
            }
        }
    }
}

#[test]
fn signed_fee_events_report_nonzero_discharge_and_actual_transfers_then_silence_on_retry() {
    for weighted in [false, true] {
        let key = PrivateKey::try_new([21; 32]).unwrap();
        let trader = AccountId::from(&PublicKey::new_from_private_key(&key));
        let (mut state, pool) = if weighted {
            weighted_state(trader)
        } else {
            fixed_state(trader, Some(200))
        };
        let buy = if weighted {
            lbp_core::Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: 1,
            }
        } else {
            lbp_core::Message::BuyExactInput {
                membership: None,
                amount_in: 101,
                min_amount_out: 1,
                now: 100,
            }
        };
        fixed_execute_events_at(&mut state, pool, &key, buy, 100_000).unwrap();
        let fees = fixed_pool(&state, pool).accrued_fees;
        let vault = balance(&state, pool, COLLATERAL);
        let events = fixed_execute_events_at(
            &mut state,
            pool,
            &key,
            lbp_core::Message::CollectFees,
            100_000,
        )
        .unwrap();
        assert_eq!(
            sale_event_kinds(&events, LBP, pool_subject(pool)),
            vec![SaleEventKindV1::FeesCollected {
                definition: *COLLATERAL.value(),
                treasury: *TREASURY.value(),
                liability_discharged: fees,
                tokens_transferred: fees,
            }]
        );
        assert_eq!(balance(&state, TREASURY, COLLATERAL), fees);
        assert_eq!(balance(&state, pool, COLLATERAL), vault - fees);
        assert_eq!(fixed_pool(&state, pool).accrued_fees, 0);
        assert!(
            fixed_execute_events_at(
                &mut state,
                pool,
                &key,
                lbp_core::Message::CollectFees,
                100_000
            )
            .unwrap()
            .is_empty()
        );
    }
    for self_treasury in [false, true] {
        let (mut state, pool, key) = curve_event_env(None);
        let treasury = if self_treasury { pool } else { TREASURY };
        state = fixed_config(state, 1_000, treasury, false);
        curve_execute_events_at(
            &mut state,
            pool,
            &key,
            CurveMessage::SwapExactInput {
                membership: None,
                token_in: COLLATERAL,
                amount_in: 100,
                min_amount_out: 1,
                now: 1,
            },
            1_000,
        )
        .unwrap();
        let before =
            PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(CURVE)).unwrap();
        assert_eq!(
            (
                before.pool.real_reserve1,
                before.pool.fees_accrued,
                balance(&state, pool, COLLATERAL)
            ),
            (90, 10, 100)
        );
        let events =
            curve_execute_events_at(&mut state, pool, &key, CurveMessage::CollectFees, 1_000)
                .unwrap();
        assert_eq!(
            sale_event_kinds(&events, CURVE, pool_subject(pool)),
            vec![SaleEventKindV1::FeesCollected {
                definition: *COLLATERAL.value(),
                treasury: *treasury.value(),
                liability_discharged: 10,
                tokens_transferred: if self_treasury { 0 } else { 10 },
            }]
        );
        let after =
            PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(CURVE)).unwrap();
        assert_eq!((after.pool.real_reserve1, after.pool.fees_accrued), (90, 0));
        assert_eq!(
            balance(&state, pool, COLLATERAL),
            if self_treasury { 100 } else { 90 }
        );
        assert!(
            curve_execute_events_at(&mut state, pool, &key, CurveMessage::CollectFees, 1_000)
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn signed_allocation_read_emits_no_closure_and_settlement_emits_one_bound_closure_before_effects() {
    for weighted in [false, true] {
        for graduation in [false, true] {
            let mut env = if graduation {
                lbp_graduation_env(weighted, true, vec![], lbp_kind(weighted))
            } else {
                lbp_manual_env(weighted, true)
            };
            env.state = fixed_config(env.state, 100, NEXT_TREASURY, true);
            let creator = env.creator();
            let events = env
                .execute_events(lbp_allocation_message(weighted, creator), 300_000)
                .unwrap();
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
                    release: AllocationReleaseV1::AfterEnd
                }]
            );
            assert!(sale_event_kinds(&events, LBP, pool_subject(env.pool)).is_empty());
            assert!(!fixed_pool(&env.state, env.pool).closure_observed);
            assert!(
                env.execute_events(lbp_allocation_message(weighted, creator), 300_000)
                    .unwrap()
                    .is_empty()
            );
            let initial = fixed_pool(&env.state, env.pool);
            let instruction = if graduation {
                lbp_hook_message(creator)
            } else {
                lbp_manual_message(weighted, creator)
            };
            let before = env.state.clone();
            let nonce = env.state.get_account_by_id(creator).nonce;
            let rejected = env.execute_events(instruction.clone(), 299_999);
            assert!(
                matches!(rejected, Err(lee::error::LeeError::OutOfValidityWindow)),
                "{rejected:?}"
            );
            assert!(env.state == before);
            assert_eq!(env.state.get_account_by_id(creator).nonce, nonce);
            let events = env.execute_events(instruction, 300_000).unwrap();
            let adapter = graduation.then(|| lbp_adapter(&env).account_id);
            assert_lbp_settlement_events(
                &events,
                env.pool,
                &initial,
                creator,
                NEXT_TREASURY,
                adapter,
            );
            if graduation {
                assert_lbp_graduated_event(&events, &env, weighted, &initial);
            } else {
                assert!(
                    sale_events(
                        &events,
                        LBP_FACTORY_PROGRAM,
                        launch_subject(env.factory, env.pool)
                    )
                    .is_empty()
                );
            }
            assert!(fixed_pool(&env.state, env.pool).closure_observed);
            if graduation {
                reject_lbp_factory(
                    &mut env,
                    lbp_manual_message(weighted, creator),
                    300_000,
                    "InvalidTerms",
                );
            } else {
                assert!(
                    env.execute_events(lbp_manual_message(weighted, creator), 300_000)
                        .unwrap()
                        .is_empty()
                );
            }
        }
    }
}

#[test]
fn signed_empty_manual_events_are_final_and_retries_do_not_repeat_effects() {
    for weighted in [false, true] {
        let mut env = lbp_manual_env(weighted, false);
        env.state = fixed_config(env.state, 100, NEXT_TREASURY, true);
        let creator = env.creator();
        let initial = fixed_pool(&env.state, env.pool);
        let events = env
            .execute_events(lbp_manual_message(weighted, creator), 300_000)
            .unwrap();
        assert_lbp_settlement_events(&events, env.pool, &initial, creator, NEXT_TREASURY, None);
        assert!(
            matches!(sale_event_kinds(&events, LBP, pool_subject(env.pool)).last(),
            Some(SaleEventKindV1::Settlement { step: SettlementStepV1::Completed, route: Some(SettlementRouteV1::Manual), effects }) if effects.is_empty())
        );
        let settled = fixed_pool(&env.state, env.pool);
        assert!(
            env.execute_events(lbp_manual_message(weighted, creator), 300_000)
                .unwrap()
                .is_empty()
        );
        assert_eq!(fixed_pool(&env.state, env.pool), settled);
        reject_lbp_factory(
            &mut env,
            lbp_hook_message(creator),
            300_000,
            "HookNotConfigured",
        );
    }
}

#[test]
fn signed_fixed_sellout_buy_is_the_only_closure_event_before_allocation_and_settlement() {
    let mut env = lbp_manual_env(false, false);
    let creator = env.creator();
    env.state = clock_state(
        env.state
            .with_public_accounts([token_holding(creator, COLLATERAL, 2_000)]),
        100_000,
    );
    let events = fixed_execute_events_at(
        &mut env.state,
        env.pool,
        &env.key,
        lbp_core::Message::BuyExactOutput {
            membership: Some(env.path.clone()),
            amount_out: 800,
            max_amount_in: 2_000,
            now: 100,
        },
        100_000,
    )
    .unwrap();
    let kinds = sale_event_kinds(&events, LBP, pool_subject(env.pool));
    assert_eq!(kinds.len(), 2);
    assert!(matches!(kinds[0], SaleEventKindV1::Trade { .. }));
    assert_eq!(
        kinds[1],
        SaleEventKindV1::Closed {
            reason: ClosureReasonV1::Depleted {
                definition: *env.launch.value()
            },
            inclusion_bounds: EventTimeBoundsV1 {
                at_least: None,
                before: None
            }
        }
    );
    assert!(fixed_pool(&env.state, env.pool).closure_observed);
    env.state = clock_state(env.state, 300_000);
    let allocation = env
        .execute_events(lbp_allocation_message(false, creator), 300_000)
        .unwrap();
    assert!(sale_event_kinds(&allocation, LBP, pool_subject(env.pool)).is_empty());
    let initial = fixed_pool(&env.state, env.pool);
    let events = env
        .execute_events(lbp_manual_message(false, creator), 300_000)
        .unwrap();
    assert_lbp_settlement_events(&events, env.pool, &initial, creator, TREASURY, None);
    assert!(
        !sale_event_kinds(&events, LBP, pool_subject(env.pool))
            .iter()
            .any(|kind| matches!(kind, SaleEventKindV1::Closed { .. }))
    );
    assert!(
        env.execute_events(lbp_manual_message(false, creator), 300_000)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn signed_curve_sellout_buy_emits_one_closure_and_withdrawal_does_not_repeat_it() {
    let (mut state, pool, key) = curve_event_env(None);
    let events = curve_execute_events_at(
        &mut state,
        pool,
        &key,
        CurveMessage::SwapExactOutput {
            membership: None,
            token_in: COLLATERAL,
            amount_out: 4_000,
            max_amount_in: 2_000,
            now: 1,
        },
        1_000,
    )
    .unwrap();
    let kinds = sale_event_kinds(&events, CURVE, pool_subject(pool));
    assert_eq!(kinds.len(), 2);
    assert!(matches!(kinds[0], SaleEventKindV1::Trade { .. }));
    assert_eq!(
        kinds[1],
        SaleEventKindV1::Closed {
            reason: ClosureReasonV1::Depleted {
                definition: *LAUNCH.value()
            },
            inclusion_bounds: EventTimeBoundsV1 {
                at_least: None,
                before: None
            }
        }
    );
    assert!(
        PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(CURVE))
            .unwrap()
            .closure_observed
    );
    let events = curve_execute_events_at(
        &mut state,
        pool,
        &key,
        CurveMessage::WithdrawReserves { now: 1 },
        1_000,
    )
    .unwrap();
    let kinds = sale_event_kinds(&events, CURVE, pool_subject(pool));
    assert!(
        !kinds
            .iter()
            .any(|kind| matches!(kind, SaleEventKindV1::Closed { .. }))
    );
    assert!(matches!(
        kinds.last(),
        Some(SaleEventKindV1::Settlement {
            step: SettlementStepV1::ReservesWithdrawn,
            ..
        })
    ));
}

#[test]
fn signed_curve_expiry_closure_uses_the_end_bound_and_manual_close_is_reported_once() {
    for manual in [false, true] {
        let (mut state, pool, key) = curve_event_env(Some(200));
        if manual {
            let events = curve_execute_events_at(
                &mut state,
                pool,
                &key,
                CurveMessage::ClosePool { now: 100 },
                100_000,
            )
            .unwrap();
            assert_eq!(
                sale_event_kinds(&events, CURVE, pool_subject(pool)),
                vec![SaleEventKindV1::Closed {
                    reason: ClosureReasonV1::Manual,
                    inclusion_bounds: EventTimeBoundsV1 {
                        at_least: None,
                        before: Some(200)
                    },
                }]
            );
        } else {
            let before = state.clone();
            let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
            let nonce = state.get_account_by_id(creator).nonce;
            let rejected = curve_execute_events_at(
                &mut state,
                pool,
                &key,
                CurveMessage::WithdrawReserves { now: 9_999 },
                199_999,
            );
            assert!(
                matches!(rejected, Err(lee::error::LeeError::OutOfValidityWindow)),
                "{rejected:?}"
            );
            assert!(state == before);
            assert_eq!(state.get_account_by_id(creator).nonce, nonce);
        }
        let events = curve_execute_events_at(
            &mut state,
            pool,
            &key,
            CurveMessage::WithdrawReserves { now: 9_999 },
            200_000,
        )
        .unwrap();
        let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
        let mut expected = vec![];
        if !manual {
            expected.push(SaleEventKindV1::Closed {
                reason: ClosureReasonV1::ScheduledEnd { timestamp: 200 },
                inclusion_bounds: EventTimeBoundsV1 {
                    at_least: Some(200),
                    before: None,
                },
            });
        }
        expected.push(SaleEventKindV1::Settlement {
            step: SettlementStepV1::ReservesWithdrawn,
            route: None,
            effects: vec![SettlementEffectV1::Payout {
                definition: *LAUNCH.value(),
                recipient: *creator.value(),
                purpose: PayoutPurposeV1::Reserve,
                liability_discharged: 4_000,
                tokens_transferred: 4_000,
            }],
        });
        assert_eq!(
            sale_event_kinds(&events, CURVE, pool_subject(pool)),
            expected
        );
    }
}

#[test]
fn signed_scheduled_sellout_does_not_emit_early_closure() {
    let mut env = fixed_factory_env_with_ending(
        Some(300),
        factory_core::CreatorAllocationRelease::AfterEnd,
        curve_core::PausePolicy::Disabled,
        lbp_core::EndingPolicy::Scheduled,
    );
    let creator = env.creator();
    env.state = env
        .state
        .with_public_accounts([token_holding(creator, COLLATERAL, 2_000)]);
    let events = fixed_execute_events_at(
        &mut env.state,
        env.pool,
        &env.key,
        lbp_core::Message::BuyExactOutput {
            membership: Some(env.path.clone()),
            amount_out: 800,
            max_amount_in: 2_000,
            now: 100,
        },
        100_000,
    )
    .unwrap();
    assert!(matches!(
        sale_event_kinds(&events, LBP, pool_subject(env.pool)).as_slice(),
        [SaleEventKindV1::Trade { .. }]
    ));
    let initial = fixed_pool(&env.state, env.pool);
    assert_eq!(initial.remaining_inventory, 0);
    assert!(!initial.closure_observed);
    let before = env.state.clone();
    assert!(matches!(
        env.execute_events(lbp_manual_message(false, creator), 299_999),
        Err(lee::error::LeeError::OutOfValidityWindow)
    ));
    assert!(env.state == before);
    let events = env
        .execute_events(lbp_manual_message(false, creator), 300_000)
        .unwrap();
    assert_lbp_settlement_events(&events, env.pool, &initial, creator, TREASURY, None);
    assert!(matches!(
        sale_event_kinds(&events, LBP, pool_subject(env.pool)).first(),
        Some(SaleEventKindV1::Closed {
            reason: ClosureReasonV1::ScheduledEnd { timestamp: 300 },
            ..
        })
    ));

    let (state, pool, key) = curve_event_env(Some(200));
    let mut sale =
        PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(CURVE)).unwrap();
    sale.pool.ending_policy = curve_core::EndingPolicy::Scheduled;
    sale.pool.close_on_depletion = None;
    sale.pool.validate_policy().unwrap();
    let mut state = state.with_public_accounts([account(
        Actor::new(pool, CURVE),
        borsh::to_vec(&sale).unwrap(),
    )]);
    let events = curve_execute_events_at(
        &mut state,
        pool,
        &key,
        CurveMessage::SwapExactOutput {
            membership: None,
            token_in: COLLATERAL,
            amount_out: 4_000,
            max_amount_in: 2_000,
            now: 1,
        },
        1_000,
    )
    .unwrap();
    assert!(matches!(
        sale_event_kinds(&events, CURVE, pool_subject(pool)).as_slice(),
        [SaleEventKindV1::Trade { .. }]
    ));
    let sale = PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(CURVE)).unwrap();
    assert_eq!(sale.pool.real_reserve0, 0);
    assert!(!sale.closure_observed);
    let before = state.clone();
    assert!(matches!(
        curve_execute_events_at(
            &mut state,
            pool,
            &key,
            CurveMessage::WithdrawReserves { now: 200 },
            199_999
        ),
        Err(lee::error::LeeError::OutOfValidityWindow)
    ));
    assert!(state == before);
    let events = curve_execute_events_at(
        &mut state,
        pool,
        &key,
        CurveMessage::WithdrawReserves { now: 200 },
        200_000,
    )
    .unwrap();
    assert!(matches!(
        sale_event_kinds(&events, CURVE, pool_subject(pool)).first(),
        Some(SaleEventKindV1::Closed {
            reason: ClosureReasonV1::ScheduledEnd { timestamp: 200 },
            ..
        })
    ));
}
