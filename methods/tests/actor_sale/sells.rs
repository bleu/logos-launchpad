use super::*;

#[test]
fn signed_common_sells_fixed_inventory_and_fee_surplus_isolation() {
    let mut env = fixed_factory_env_with_trade_mode(
        Some(200),
        factory_core::CreatorAllocationRelease::AfterSetup,
        curve_core::PausePolicy::AuthorityControlled,
        lbp_core::EndingPolicy::Scheduled,
        None,
        lbp_core::TradeMode::BuyAndSell,
    );
    let creator = env.creator();
    let seller_key = PrivateKey::try_new([22; 32]).unwrap();
    let seller = AccountId::from(&PublicKey::new_from_private_key(&seller_key));
    fixed_execute(
        &mut env.state,
        env.pool,
        &env.key,
        lbp_core::Message::BuyExactInput {
            membership: Some(env.path.clone()),
            amount_in: 101,
            min_amount_out: 64,
            now: 100,
        },
    )
    .unwrap();
    let bought = fixed_pool(&env.state, env.pool);
    assert_eq!(bought.recorded_collateral, 97);
    assert_eq!(bought.accrued_fees, 4);
    // The seller holds tokens from the creator allocation and has no buy membership.
    env.state = env.state.with_public_accounts([
        token_holding(creator, env.launch, 199),
        token_holding(seller, env.launch, 65),
        token_holding(seller, COLLATERAL, 0),
        token_holding(env.pool, COLLATERAL, 142),
        token_holding(env.pool, env.launch, 743),
    ]);
    fixed_rejected(
        &mut env.state,
        env.pool,
        &seller_key,
        lbp_core::Message::BuyExactInput {
            membership: None,
            amount_in: 1,
            min_amount_out: 1,
            now: 100,
        },
        "Unauthorized",
    );
    let sell = lbp_core::Message::SellExactInput {
        amount_in: 65,
        min_amount_out: 93,
        now: 100,
    };
    fixed_rejected(
        &mut env.state,
        env.pool,
        &seller_key,
        lbp_core::Message::SellExactInput {
            amount_in: 65,
            min_amount_out: 94,
            now: 100,
        },
        "Slippage",
    );
    let events =
        fixed_execute_events_at(&mut env.state, env.pool, &seller_key, sell, 100_000).unwrap();
    assert_eq!(
        sale_events(&events, LBP, pool_subject(env.pool))[0].engine,
        PricingEngineV1::FixedPrice
    );
    let sale = fixed_pool(&env.state, env.pool);
    assert_eq!(sale.trade_mode, lbp_core::TradeMode::BuyAndSell);
    assert_eq!(sale.remaining_inventory, 801);
    assert!(sale.remaining_inventory > sale.sale_inventory);
    assert_eq!(sale.recorded_collateral, 0);
    assert_eq!(sale.accrued_fees, 8);
    assert_eq!(balance(&env.state, seller, env.launch), 0);
    assert_eq!(balance(&env.state, seller, COLLATERAL), 93);
    assert_eq!(balance(&env.state, env.pool, COLLATERAL), 49);
    assert_eq!(balance(&env.state, env.pool, env.launch), 808);
    assert_eq!(
        sale_event_kinds(&events, LBP, pool_subject(env.pool)),
        vec![SaleEventKindV1::Trade {
            trader: seller.to_bytes(),
            form: TradeFormV1::ExactInput,
            token_in: env.launch.to_bytes(),
            token_out: COLLATERAL.to_bytes(),
            amount_in: 65,
            pricing_input: 65,
            gross_output: 97,
            amount_out: 93,
            fee_definition: COLLATERAL.to_bytes(),
            fee_amount: 4,
            fee_bps: 333,
            retained_fee: 0,
            sell_fee_bps: 0,
            pricing_time: None,
        }]
    );
    env.state = env
        .state
        .with_public_accounts([token_holding(seller, env.launch, 2)]);
    fixed_rejected(
        &mut env.state,
        env.pool,
        &seller_key,
        lbp_core::Message::SellExactInput {
            amount_in: 2,
            min_amount_out: 0,
            now: 100,
        },
        "Underfunded",
    );
    fixed_execute(
        &mut env.state,
        env.pool,
        &seller_key,
        lbp_core::Message::CollectFees,
    )
    .unwrap();
    assert_eq!(balance(&env.state, TREASURY, COLLATERAL), 8);
    assert_eq!(balance(&env.state, env.pool, COLLATERAL), 41);
    env.execute(
        factory_core::Message::WithdrawFactoryProceeds { creator, now: 200 },
        200_000,
    )
    .unwrap();
    assert_eq!(balance(&env.state, env.pool, COLLATERAL), 41);
    assert_eq!(balance(&env.state, env.pool, env.launch), 7);
    assert_eq!(
        fixed_pool(&env.state, env.pool).settlement_progress,
        lbp_core::SettlementProgress::ManualComplete
    );
}

#[test]
fn signed_common_sells_weighted_seed_fee_surplus_isolation() {
    let (mut state, factory, pool, launch, _escrow, key, actors, salt) =
        weighted_factory_env_with_trade_mode(true, None, lbp_core::TradeMode::BuyAndSell);
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
    let before = fixed_pool(&state, pool);
    state = state.with_public_accounts([token_holding(
        pool,
        COLLATERAL,
        before.recorded_collateral + before.accrued_fees + 41,
    )]);
    let quote = lbp_core::quote_trade(
        &before,
        lbp_core::QuoteRequest::SellExactInput {
            tokens_in: 100,
            min_collateral_out: 0,
        },
        100,
        150,
    )
    .unwrap();
    assert!(quote.protocol_fee > 0);
    let collateral = balance(&state, creator, COLLATERAL);
    let tokens = balance(&state, creator, launch);
    let pool_tokens = balance(&state, pool, launch);
    let sell = lbp_core::Message::SellWeightedExactInput {
        amount_in: 100,
        min_amount_out: quote.amount_out,
    };
    let rejected = lbp_core::Message::SellWeightedExactInput {
        amount_in: 100,
        min_amount_out: quote.amount_out + 1,
    };
    let saved = state.clone();
    assert!(
        matches!(fixed_execute_events_at(&mut state, pool, &key, rejected, 150_000),
        Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains("Slippage"))
    );
    assert!(state == saved);
    let events = fixed_execute_events_at(&mut state, pool, &key, sell, 150_000).unwrap();
    assert_eq!(
        sale_events(&events, LBP, pool_subject(pool))[0].engine,
        PricingEngineV1::Weighted
    );
    let sale = fixed_pool(&state, pool);
    assert_eq!(
        sale.recorded_collateral,
        before.recorded_collateral - quote.gross_collateral
    );
    assert!(sale.recorded_collateral >= 100);
    assert_eq!(sale.accrued_fees, before.accrued_fees + quote.protocol_fee);
    assert_eq!(sale.remaining_inventory, before.remaining_inventory + 100);
    assert!(
        matches!(sale.mode, lbp_core::PoolMode::Weighted { pricing_collateral, .. } if pricing_collateral == sale.recorded_collateral)
    );
    assert_eq!(
        balance(&state, creator, COLLATERAL),
        collateral + quote.amount_out
    );
    assert_eq!(balance(&state, creator, launch), tokens - 100);
    assert_eq!(balance(&state, pool, launch), pool_tokens + 100);
    assert_eq!(
        balance(&state, pool, COLLATERAL),
        sale.recorded_collateral + sale.accrued_fees + 41
    );
    assert_eq!(
        sale_event_kinds(&events, LBP, pool_subject(pool)),
        vec![SaleEventKindV1::Trade {
            trader: creator.to_bytes(),
            form: TradeFormV1::ExactInput,
            token_in: launch.to_bytes(),
            token_out: COLLATERAL.to_bytes(),
            amount_in: 100,
            pricing_input: 100,
            gross_output: quote.gross_collateral,
            amount_out: quote.amount_out,
            fee_definition: COLLATERAL.to_bytes(),
            fee_amount: quote.protocol_fee,
            fee_bps: 100,
            retained_fee: 0,
            sell_fee_bps: 0,
            pricing_time: Some(150),
        }]
    );
    let mut drain = state
        .clone()
        .with_public_accounts([token_holding(creator, launch, 1_000)]);
    let saved = drain.clone();
    assert!(matches!(fixed_execute_events_at(&mut drain, pool, &key,
        lbp_core::Message::SellWeightedExactInput { amount_in: 1_000, min_amount_out: 0 }, 150_000),
        Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains("Underfunded")));
    assert!(drain == saved);
    fixed_execute_at(
        &mut state,
        pool,
        &key,
        lbp_core::Message::CollectFees,
        150_000,
    )
    .unwrap();
    assert_eq!(balance(&state, TREASURY, COLLATERAL), sale.accrued_fees);
    assert_eq!(
        balance(&state, pool, COLLATERAL),
        sale.recorded_collateral + 41
    );
    let collateral = balance(&state, creator, COLLATERAL);
    state = clock_state(state, 300_000);
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
        collateral + sale.recorded_collateral
    );
    assert_eq!(balance(&state, pool, COLLATERAL), 41);
    assert_eq!(balance(&state, pool, launch), 0);
}

#[test]
fn signed_common_sells_selected_depletion_and_scheduled_refill() {
    for ending in [
        lbp_core::EndingPolicy::Scheduled,
        lbp_core::EndingPolicy::Depletion,
        lbp_core::EndingPolicy::Either,
    ] {
        let mut env = fixed_factory_env_with_trade_mode(
            if ending == lbp_core::EndingPolicy::Depletion {
                None
            } else {
                Some(200)
            },
            factory_core::CreatorAllocationRelease::AfterEnd,
            curve_core::PausePolicy::Disabled,
            ending,
            None,
            lbp_core::TradeMode::BuyAndSell,
        );
        let creator = env.creator();
        env.state = env
            .state
            .with_public_accounts([token_holding(creator, COLLATERAL, 2_000)]);
        env.buy(800, 1_242);
        let sell = lbp_core::Message::SellExactInput {
            amount_in: 100,
            min_amount_out: 145,
            now: 100,
        };
        if ending == lbp_core::EndingPolicy::Scheduled {
            assert!(!fixed_pool(&env.state, env.pool).closure_observed);
            let events =
                fixed_execute_events_at(&mut env.state, env.pool, &env.key, sell, 100_000).unwrap();
            assert_eq!(
                sale_event_kinds(&events, LBP, pool_subject(env.pool)).len(),
                1
            );
            assert_eq!(
                sale_events(&events, LBP, pool_subject(env.pool))[0].engine,
                PricingEngineV1::FixedPrice
            );
            assert_eq!(fixed_pool(&env.state, env.pool).remaining_inventory, 100);
            assert!(!fixed_pool(&env.state, env.pool).closure_observed);
            env.buy(100, 156);
            assert!(!fixed_pool(&env.state, env.pool).closure_observed);
        } else {
            assert!(fixed_pool(&env.state, env.pool).closure_observed);
            fixed_rejected(&mut env.state, env.pool, &env.key, sell, "Closed");
        }
    }
}

#[test]
fn signed_common_sells_buy_only_wrapper_and_bare_pool_rejection() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let trader = AccountId::from(&PublicKey::new_from_private_key(&key));
    for weighted in [false, true] {
        let (mut state, pool) = if weighted {
            weighted_state(trader)
        } else {
            fixed_state(trader, Some(200))
        };
        let sell = if weighted {
            lbp_core::Message::SellWeightedExactInput {
                amount_in: 1,
                min_amount_out: 0,
            }
        } else {
            lbp_core::Message::SellExactInput {
                amount_in: 1,
                min_amount_out: 0,
                now: 100,
            }
        };
        fixed_rejected(
            &mut state,
            pool,
            &key,
            sell.clone(),
            "TradeDirectionUnsupported",
        );
        let transaction = fixed_transaction(&state, pool, &key, sell.clone());
        let mut direct = transaction.message.clone();
        direct.execution.root.to = Actor::new(pool, LBP);
        direct.execution.root.message = borsh::to_vec(&sell).unwrap();
        let witnesses = WitnessSet::for_message(&direct, &[&key]);
        let saved = state.clone();
        assert!(
            matches!(state.transition_from_public_transaction(&PublicTransaction::new(direct, witnesses), 1, 100_000),
            Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains("TradeDirectionUnsupported"))
        );
        assert!(state == saved);
    }
}

#[test]
fn signed_common_sells_controls_and_late_credit_rollback() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let trader = AccountId::from(&PublicKey::new_from_private_key(&key));
    for weighted in [false, true] {
        let (state, pool) = if weighted {
            weighted_state(trader)
        } else {
            fixed_state(trader, Some(200))
        };
        let mut sale = fixed_pool(&state, pool);
        sale.trade_mode = lbp_core::TradeMode::BuyAndSell;
        let mut state = state.with_public_accounts([account(
            Actor::new(pool, LBP),
            borsh::to_vec(&sale).unwrap(),
        )]);
        let buy = if weighted {
            lbp_core::Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 101,
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
        fixed_execute(&mut state, pool, &key, buy).unwrap();
        let sell = if weighted {
            lbp_core::Message::SellWeightedExactInput {
                amount_in: 10,
                min_amount_out: 1,
            }
        } else {
            lbp_core::Message::SellExactInput {
                amount_in: 10,
                min_amount_out: 1,
                now: 100,
            }
        };
        let mut sale = fixed_pool(&state, pool);
        sale.trade_control = lbp_core::TradeControl::Paused;
        let mut paused = state.clone().with_public_accounts([account(
            Actor::new(pool, LBP),
            borsh::to_vec(&sale).unwrap(),
        )]);
        fixed_rejected(&mut paused, pool, &key, sell.clone(), "Paused");
        let mut frozen = fixed_config(state.clone(), 333, TREASURY, true);
        fixed_rejected(&mut frozen, pool, &key, sell.clone(), "Frozen");
        let tokens = balance(&state, trader, LAUNCH);
        let pool_tokens = balance(&state, pool, LAUNCH);
        let collateral = balance(&state, trader, COLLATERAL);
        let mut overflow =
            state.with_public_accounts([token_holding(trader, COLLATERAL, u128::MAX)]);
        fixed_rejected(&mut overflow, pool, &key, sell.clone(), "Overflow");
        overflow = overflow.with_public_accounts([token_holding(trader, COLLATERAL, collateral)]);
        let events = fixed_execute_events_at(&mut overflow, pool, &key, sell, 100_000).unwrap();
        assert_eq!(sale_event_kinds(&events, LBP, pool_subject(pool)).len(), 1);
        assert_eq!(
            sale_events(&events, LBP, pool_subject(pool))[0].engine,
            if weighted {
                PricingEngineV1::Weighted
            } else {
                PricingEngineV1::FixedPrice
            }
        );
        assert_eq!(balance(&overflow, trader, LAUNCH), tokens - 10);
        assert_eq!(balance(&overflow, pool, LAUNCH), pool_tokens + 10);
        assert!(balance(&overflow, trader, COLLATERAL) > collateral);
    }
}

#[test]
fn signed_retained_sells_fixed_and_weighted_accounting_and_live_fee_scope() {
    let retained_bps = 777;
    for weighted in [false, true] {
        let (mut state, factory, pool, launch, key, actors, salt, membership, now, namespace_bps) =
            if weighted {
                let (state, factory, pool, launch, _, key, actors, salt) =
                    weighted_factory_env_with_sell_fee(
                        true,
                        None,
                        lbp_core::TradeMode::BuyAndSell,
                        retained_bps,
                    );
                (
                    state, factory, pool, launch, key, actors, salt, None, 150, 100,
                )
            } else {
                let mut env = fixed_factory_env_with_sell_fee(
                    Some(200),
                    factory_core::CreatorAllocationRelease::AfterEnd,
                    curve_core::PausePolicy::AuthorityControlled,
                    lbp_core::EndingPolicy::Scheduled,
                    None,
                    lbp_core::TradeMode::BuyAndSell,
                    retained_bps,
                );
                env.buy(100, 156);
                (
                    env.state,
                    env.factory,
                    env.pool,
                    env.launch,
                    env.key,
                    env.actors,
                    env.salt,
                    Some(env.path),
                    100,
                    333,
                )
            };
        let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
        let initial = fixed_pool(&state, pool);
        assert_eq!(initial.sell_fee_bps, retained_bps);
        let quote = lbp_core::quote_trade(
            &initial,
            lbp_core::QuoteRequest::SellExactInput {
                tokens_in: 40,
                min_collateral_out: 0,
            },
            namespace_bps,
            now,
        )
        .unwrap();
        let ceil_fee = |bps: u16| (quote.gross_collateral * u128::from(bps)).div_ceil(10_000);
        assert_eq!(quote.protocol_fee, ceil_fee(namespace_bps));
        assert_eq!(quote.retained_fee, ceil_fee(retained_bps));
        if weighted {
            assert!(
                quote.protocol_fee + quote.retained_fee
                    > (quote.gross_collateral * u128::from(namespace_bps + retained_bps))
                        .div_ceil(10_000)
            );
        }
        assert_eq!(
            quote.amount_out,
            quote.gross_collateral - quote.protocol_fee - quote.retained_fee
        );
        assert!(quote.retained_fee > 0);
        let sell = if weighted {
            lbp_core::Message::SellWeightedExactInput {
                amount_in: 40,
                min_amount_out: quote.amount_out,
            }
        } else {
            lbp_core::Message::SellExactInput {
                amount_in: 40,
                min_amount_out: quote.amount_out,
                now,
            }
        };
        let collateral = balance(&state, creator, COLLATERAL);
        let pool_collateral = balance(&state, pool, COLLATERAL);
        let tokens = balance(&state, creator, launch);
        let pool_tokens = balance(&state, pool, launch);
        let mut overflow =
            state
                .clone()
                .with_public_accounts([token_holding(creator, COLLATERAL, u128::MAX)]);
        let saved = overflow.clone();
        assert!(
            matches!(fixed_execute_events_at(&mut overflow, pool, &key, sell.clone(), now * 1_000),
            Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains("Overflow"))
        );
        assert!(overflow == saved);
        let events = fixed_execute_events_at(&mut state, pool, &key, sell, now * 1_000).unwrap();
        let sale = fixed_pool(&state, pool);
        assert_eq!(sale.sell_fee_bps, retained_bps);
        assert_eq!(
            sale.recorded_collateral,
            initial.recorded_collateral - quote.gross_collateral + quote.retained_fee
        );
        assert_eq!(sale.accrued_fees, initial.accrued_fees + quote.protocol_fee);
        assert_eq!(
            balance(&state, creator, COLLATERAL),
            collateral + quote.amount_out
        );
        assert_eq!(
            balance(&state, pool, COLLATERAL),
            pool_collateral - quote.amount_out
        );
        assert_eq!(balance(&state, creator, launch), tokens - 40);
        assert_eq!(balance(&state, pool, launch), pool_tokens + 40);
        assert_eq!(sale.remaining_inventory, initial.remaining_inventory + 40);
        if weighted {
            assert!(
                matches!(sale.mode, lbp_core::PoolMode::Weighted { collateral_seed: 100, pricing_collateral, .. } if pricing_collateral == sale.recorded_collateral && pricing_collateral >= 100)
            );
        } else {
            assert_eq!(sale.price, initial.price);
        }
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
                trader: creator.to_bytes(),
                form: TradeFormV1::ExactInput,
                token_in: launch.to_bytes(),
                token_out: COLLATERAL.to_bytes(),
                amount_in: 40,
                pricing_input: 40,
                gross_output: quote.gross_collateral,
                amount_out: quote.amount_out,
                fee_definition: COLLATERAL.to_bytes(),
                fee_amount: quote.protocol_fee,
                fee_bps: namespace_bps,
                retained_fee: quote.retained_fee,
                sell_fee_bps: retained_bps,
                pricing_time: if weighted { Some(now) } else { None },
            }
        );
        let dust = if weighted {
            lbp_core::Message::SellWeightedExactInput {
                amount_in: 5,
                min_amount_out: 0,
            }
        } else {
            lbp_core::Message::SellExactInput {
                amount_in: 1,
                min_amount_out: 0,
                now,
            }
        };
        let saved = state.clone();
        assert!(fixed_execute_events_at(&mut state, pool, &key, dust, now * 1_000).is_err());
        assert!(state == saved);
        state = fixed_config(state, 10_000 - retained_bps, TREASURY, false);
        let saved = state.clone();
        let sell = if weighted {
            lbp_core::Message::SellWeightedExactInput {
                amount_in: 10,
                min_amount_out: 0,
            }
        } else {
            lbp_core::Message::SellExactInput {
                amount_in: 10,
                min_amount_out: 0,
                now,
            }
        };
        assert!(
            matches!(fixed_execute_events_at(&mut state, pool, &key, sell, now * 1_000),
            Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains("InvalidFee"))
        );
        assert!(state == saved);
        let buy = if weighted {
            lbp_core::Message::BuyWeightedExactInput {
                membership,
                amount_in: 500,
                min_amount_out: 1,
            }
        } else {
            lbp_core::Message::BuyExactInput {
                membership,
                amount_in: 500,
                min_amount_out: 1,
                now,
            }
        };
        let before_buy = balance(&state, creator, launch);
        let before_payment = balance(&state, creator, COLLATERAL);
        fixed_execute_at(&mut state, pool, &key, buy, now * 1_000).unwrap();
        assert!(balance(&state, creator, launch) > before_buy);
        assert_eq!(balance(&state, creator, COLLATERAL), before_payment - 500);
        fixed_execute_at(
            &mut state,
            pool,
            &key,
            lbp_core::Message::CollectFees,
            now * 1_000,
        )
        .unwrap();
        for paused in [true, false] {
            let control = if weighted {
                factory_core::Message::SetWeightedPaused { creator, paused }
            } else {
                factory_core::Message::SetPaused { creator, paused }
            };
            factory_execute_at(
                &mut state,
                factory,
                &actors,
                &key,
                salt,
                launch,
                pool,
                control,
                now * 1_000,
            )
            .unwrap();
        }
        let sale = fixed_pool(&state, pool);
        assert_eq!(sale.sell_fee_bps, retained_bps);
        let collateral = balance(&state, creator, COLLATERAL);
        let end = sale.end_timestamp.unwrap();
        state = clock_state(state, end * 1_000);
        let settle = if weighted {
            factory_core::Message::WithdrawWeightedProceeds { creator }
        } else {
            factory_core::Message::WithdrawFactoryProceeds { creator, now: end }
        };
        factory_execute_at(
            &mut state,
            factory,
            &actors,
            &key,
            salt,
            launch,
            pool,
            settle,
            end * 1_000,
        )
        .unwrap();
        assert_eq!(
            balance(&state, creator, COLLATERAL),
            collateral + sale.recorded_collateral
        );
        assert_eq!(balance(&state, pool, COLLATERAL), 0);
    }
}

#[test]
fn signed_retained_sells_curve_grows_k_without_consuming_rounding_slack() {
    let (mut state, pool, key) = curve_event_env_with_sell_fee(Some(200), 777);
    let trader = AccountId::from(&PublicKey::new_from_private_key(&key));
    let read = |state: &V03State| {
        PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(CURVE)).unwrap()
    };
    let initial = read(&state);
    assert_eq!(initial.pool.sell_fee_bps, 777);
    assert_eq!(initial.pool.initial_k, initial.pool.k);
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
    let before = read(&state);
    let mut without_retention = before.pool.clone();
    without_retention.sell_fee_bps = 0;
    let baseline = without_retention
        .swap_exact_input(pool::ExactInputSwap {
            token_in: pool::TokenSide::Token0,
            amount_in: 300,
            min_amount_out: 1,
            pool_fee_bps: 0,
            protocol_fee_bps: 1_000,
            now: 1,
        })
        .unwrap();
    let namespace = (baseline.raw_amount_out * 1_000).div_ceil(10_000);
    let retained = (baseline.raw_amount_out * 777).div_ceil(10_000);
    let net = baseline.raw_amount_out - namespace - retained;
    assert!(retained > 0);
    let collateral = balance(&state, trader, COLLATERAL);
    let tokens = balance(&state, trader, LAUNCH);
    let vault_collateral = balance(&state, pool, COLLATERAL);
    let vault_tokens = balance(&state, pool, LAUNCH);
    let sell = CurveMessage::SwapExactInput {
        membership: None,
        token_in: LAUNCH,
        amount_in: 300,
        min_amount_out: net,
        now: 1,
    };
    let mut overflow =
        state
            .clone()
            .with_public_accounts([token_holding(trader, COLLATERAL, u128::MAX)]);
    let saved = overflow.clone();
    assert!(
        matches!(curve_execute_events_at(&mut overflow, pool, &key, sell.clone(), 1_000),
        Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains("Overflow"))
    );
    assert!(overflow == saved);
    let events = curve_execute_events_at(&mut state, pool, &key, sell, 1_000).unwrap();
    let after = read(&state);
    assert_eq!(after.pool.initial_k, initial.pool.initial_k);
    assert_eq!(
        after.pool.k,
        without_retention.k + without_retention.virtual_reserve0 * retained
    );
    assert!(after.pool.k > before.pool.k);
    assert_eq!(
        after.pool.virtual_reserve0 * after.pool.virtual_reserve1 - after.pool.k,
        without_retention.virtual_reserve0 * without_retention.virtual_reserve1
            - without_retention.k
    );
    assert_eq!(
        after.pool.real_reserve1,
        before.pool.real_reserve1 - baseline.raw_amount_out + retained
    );
    assert_eq!(
        after.pool.fees_accrued,
        before.pool.fees_accrued + namespace
    );
    assert_eq!(balance(&state, trader, COLLATERAL), collateral + net);
    assert_eq!(balance(&state, pool, COLLATERAL), vault_collateral - net);
    assert_eq!(balance(&state, trader, LAUNCH), tokens - 300);
    assert_eq!(balance(&state, pool, LAUNCH), vault_tokens + 300);
    let decoded = sale_events(&events, CURVE, pool_subject(pool));
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].engine, PricingEngineV1::Curve);
    assert_eq!(
        decoded[0].kind,
        SaleEventKindV1::Trade {
            trader: trader.to_bytes(),
            form: TradeFormV1::ExactInput,
            token_in: LAUNCH.to_bytes(),
            token_out: COLLATERAL.to_bytes(),
            amount_in: 300,
            pricing_input: 300,
            gross_output: baseline.raw_amount_out,
            amount_out: net,
            fee_definition: COLLATERAL.to_bytes(),
            fee_amount: namespace,
            fee_bps: 1_000,
            retained_fee: retained,
            sell_fee_bps: 777,
            pricing_time: None,
        }
    );
    let saved = state.clone();
    assert!(
        curve_execute_events_at(
            &mut state,
            pool,
            &key,
            CurveMessage::SwapExactInput {
                membership: None,
                token_in: LAUNCH,
                amount_in: 1,
                min_amount_out: 0,
                now: 1,
            },
            1_000
        )
        .is_err()
    );
    assert!(state == saved);
    state = fixed_config(state, 9_223, TREASURY, false);
    let saved = state.clone();
    assert!(
        curve_execute_events_at(
            &mut state,
            pool,
            &key,
            CurveMessage::SwapExactInput {
                membership: None,
                token_in: LAUNCH,
                amount_in: 100,
                min_amount_out: 0,
                now: 1,
            },
            1_000
        )
        .is_err()
    );
    assert!(state == saved);
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
    curve_execute_events_at(&mut state, pool, &key, CurveMessage::CollectFees, 1_000).unwrap();
    for paused in [true, false] {
        curve_execute_events_at(
            &mut state,
            pool,
            &key,
            CurveMessage::SetPaused { paused },
            1_000,
        )
        .unwrap();
    }
    let final_state = read(&state);
    assert_eq!(final_state.pool.sell_fee_bps, 777);
    assert_eq!(final_state.pool.initial_k, initial.pool.initial_k);
    let collateral = balance(&state, trader, COLLATERAL);
    let tokens = balance(&state, trader, LAUNCH);
    curve_execute_events_at(
        &mut state,
        pool,
        &key,
        CurveMessage::WithdrawReserves { now: 200 },
        200_000,
    )
    .unwrap();
    assert_eq!(
        balance(&state, trader, COLLATERAL),
        collateral + final_state.pool.real_reserve1
    );
    assert_eq!(
        balance(&state, trader, LAUNCH),
        tokens + final_state.pool.real_reserve0
    );
    assert_eq!(balance(&state, pool, COLLATERAL), 0);
    assert_eq!(balance(&state, pool, LAUNCH), 0);
    assert_eq!(read(&state).pool.initial_k, initial.pool.initial_k);
}
