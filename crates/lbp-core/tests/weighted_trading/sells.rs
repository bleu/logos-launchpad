use super::fixture::{
    checkpoint_events, config, funded, id, operation, operation_at, pricing_collateral,
    sale_events, settle, state_of, weight,
};
use lbp_core::{Message, handle};
use lee_core::account::Actor;

#[test]
fn weighted_sells_price_full_reserves_and_pay_only_net_collateral() {
    use curve_core::events::{SaleEventKindV1, TradeFormV1};
    use lbp_core::{QuoteRequest, quote_trade};
    let (input, mut state) = funded();
    state.trade_mode = lbp_core::TradeMode::BuyAndSell;
    state.recorded_collateral = 1_000_000;
    state.accrued_fees = 7;
    if let lbp_core::PoolMode::Weighted {
        pricing_collateral,
        token_weight_start,
        token_weight_end,
        ..
    } = &mut state.mode
    {
        *pricing_collateral = 1_000_000;
        *token_weight_start = weight(50);
        *token_weight_end = weight(50);
    }
    for retained_bps in [0_u16, 1, 250, 5000] {
        state.sell_fee_bps = retained_bps;
        for tokens in [1000_u128, 100_000, 250_000] {
            for fee_bps in [0_u16, 100, 3333] {
                let quoted = quote_trade(
                    &state,
                    QuoteRequest::SellExactInput {
                        tokens_in: tokens,
                        min_collateral_out: 0,
                    },
                    fee_bps,
                    1500,
                )
                .unwrap();
                let reference = 1_000_000 * tokens / (1_000_000 + tokens);
                assert!(
                    quoted.gross_collateral <= reference
                        && reference - quoted.gross_collateral <= 1
                );
                assert_eq!(
                    quoted.protocol_fee,
                    (quoted.gross_collateral * u128::from(fee_bps)).div_ceil(10_000)
                );
                assert_eq!(
                    quoted.retained_fee,
                    (quoted.gross_collateral * u128::from(retained_bps)).div_ceil(10_000)
                );
                assert_eq!(
                    quoted.amount_out,
                    quoted.gross_collateral - quoted.protocol_fee - quoted.retained_fee
                );
                let message = Message::SellWeightedExactInput {
                    amount_in: tokens,
                    min_amount_out: quoted.amount_out,
                };
                let response = operation(
                    &input,
                    &state,
                    message.clone(),
                    config(fee_bps, id(43), false),
                    [1_100_007, 1_000_000, tokens],
                )
                .unwrap();
                let next = state_of(&response);
                assert_eq!(
                    next.recorded_collateral,
                    1_000_000 - quoted.gross_collateral + quoted.retained_fee
                );
                assert_eq!(pricing_collateral(&next), next.recorded_collateral);
                assert_eq!(next.accrued_fees, 7 + quoted.protocol_fee);
                assert_eq!(next.remaining_inventory, 1_000_000 + tokens);
                assert_eq!(checkpoint_events(&response).len(), 1);
                assert_eq!(
                    sale_events(&response)[0].kind,
                    SaleEventKindV1::Trade {
                        trader: *id(50).value(),
                        form: TradeFormV1::ExactInput,
                        token_in: *id(41).value(),
                        token_out: *id(42).value(),
                        amount_in: tokens,
                        pricing_input: tokens,
                        gross_output: quoted.gross_collateral,
                        amount_out: quoted.amount_out,
                        fee_definition: *id(42).value(),
                        fee_amount: quoted.protocol_fee,
                        fee_bps,
                        retained_fee: quoted.retained_fee,
                        sell_fee_bps: retained_bps,
                        pricing_time: Some(1500),
                    }
                );
                let calls: Vec<lez_compat_core::token::Message> = response
                    .sync_messages
                    .iter()
                    .map(|c| borsh::from_slice(&c.message).unwrap())
                    .collect();
                assert_eq!(
                    calls,
                    vec![
                        lez_compat_core::token::Message::Transfer {
                            definition: id(41),
                            recipient: input.receiver.account_id,
                            amount: tokens
                        },
                        lez_compat_core::token::Message::Transfer {
                            definition: id(42),
                            recipient: id(50),
                            amount: quoted.amount_out
                        },
                    ]
                );
                let donated = operation(
                    &input,
                    &state,
                    message,
                    config(fee_bps, id(43), false),
                    [2_000_007, 2_000_000, tokens],
                )
                .unwrap();
                assert_eq!(response.post_state, donated.post_state);
                let buy = operation(
                    &input,
                    &next,
                    Message::BuyWeightedExactInput {
                        membership: None,
                        amount_in: 10,
                        min_amount_out: 0,
                    },
                    config(0, id(43), false),
                    [1_100_007 - quoted.amount_out, next.remaining_inventory, 10],
                )
                .unwrap();
                assert!(state_of(&buy).remaining_inventory < next.remaining_inventory);
            }
        }
    }
}

#[test]
fn weighted_sells_protect_original_seed_even_with_fees_and_donations() {
    use lbp_core::{Error, QuoteRequest, quote_trade};
    let (input, mut state) = funded();
    state.trade_mode = lbp_core::TradeMode::BuyAndSell;
    if let lbp_core::PoolMode::Weighted {
        token_weight_start,
        token_weight_end,
        ..
    } = &mut state.mode
    {
        *token_weight_start = weight(50);
        *token_weight_end = weight(50);
    }
    state.accrued_fees = 50;
    let request = QuoteRequest::SellExactInput {
        tokens_in: 100_000,
        min_collateral_out: 0,
    };
    assert_eq!(
        quote_trade(&state, request, 0, 1500),
        Err(Error::Underfunded)
    );
    let message = Message::SellWeightedExactInput {
        amount_in: 100_000,
        min_amount_out: 0,
    };
    assert_eq!(
        operation(
            &input,
            &state,
            message,
            config(0, id(43), false),
            [10_000, 1_000_000, 100_000]
        )
        .err(),
        Some(Error::Underfunded)
    );
    state.recorded_collateral = 101;
    if let lbp_core::PoolMode::Weighted {
        pricing_collateral, ..
    } = &mut state.mode
    {
        *pricing_collateral = 101;
    }
    assert_eq!(
        quote_trade(&state, request, 0, 1500),
        Err(Error::Underfunded)
    );
}

#[test]
fn weighted_sells_keep_mode_pause_time_freeze_and_selected_depletion_checks() {
    use lbp_core::{Error, QuoteRequest, quote_trade};
    let (input, mut state) = funded();
    let request = QuoteRequest::SellExactInput {
        tokens_in: 1000,
        min_collateral_out: 0,
    };
    let message = Message::SellWeightedExactInput {
        amount_in: 1000,
        min_amount_out: 0,
    };
    assert_eq!(
        operation(
            &input,
            &state,
            message.clone(),
            config(0, id(43), false),
            [100, 1_000_000, 1000]
        )
        .err(),
        Some(Error::TradeDirectionUnsupported)
    );
    state.trade_mode = lbp_core::TradeMode::BuyAndSell;
    state.recorded_collateral = 1_000_000;
    if let lbp_core::PoolMode::Weighted {
        pricing_collateral, ..
    } = &mut state.mode
    {
        *pricing_collateral = 1_000_000;
    }
    for time in [999_999, 3_000_000] {
        assert_eq!(
            operation_at(
                time,
                &input,
                &state,
                message.clone(),
                config(0, id(43), false),
                [1_000_000, 1_000_000, 1000]
            )
            .err(),
            Some(Error::Closed)
        );
    }
    assert_eq!(
        operation(
            &input,
            &state,
            message.clone(),
            config(0, id(43), true),
            [1_000_000, 1_000_000, 1000]
        )
        .err(),
        Some(Error::Frozen)
    );
    state.trade_control = lbp_core::TradeControl::Paused;
    assert_eq!(
        operation(
            &input,
            &state,
            message.clone(),
            config(0, id(43), false),
            [1_000_000, 1_000_000, 1000]
        )
        .err(),
        Some(Error::Paused)
    );
    state.trade_control = lbp_core::TradeControl::Unrestricted;
    state.ending_policy = lbp_core::EndingPolicy::Either;
    state.remaining_inventory = 0;
    assert_eq!(quote_trade(&state, request, 0, 1500), Err(Error::Closed));
}

#[test]
fn weighted_retention_cannot_expand_gross_payout_budget_into_seed() {
    use lbp_core::{Error, QuoteRequest, quote_trade};
    let (input, mut state) = funded();
    state.trade_mode = lbp_core::TradeMode::BuyAndSell;
    state.sell_fee_bps = 5000;
    state.recorded_collateral = 1000;
    if let lbp_core::PoolMode::Weighted {
        pricing_collateral,
        token_weight_start,
        token_weight_end,
        ..
    } = &mut state.mode
    {
        *pricing_collateral = 1000;
        *token_weight_start = weight(50);
        *token_weight_end = weight(50);
    }
    let request = QuoteRequest::SellExactInput {
        tokens_in: 200_000,
        min_collateral_out: 0,
    };
    let quoted = quote_trade(&state, request, 100, 1500).unwrap();
    assert!(quoted.gross_collateral > 100);
    assert!(quoted.gross_collateral - quoted.retained_fee <= 100);
    if let lbp_core::PoolMode::Weighted {
        collateral_seed, ..
    } = &mut state.mode
    {
        *collateral_seed = 900;
    }
    assert_eq!(
        quote_trade(&state, request, 100, 1500),
        Err(Error::Underfunded)
    );
    assert_eq!(
        operation(
            &input,
            &state,
            Message::SellWeightedExactInput {
                amount_in: 200_000,
                min_amount_out: 0
            },
            config(100, id(43), false),
            [10_000, 1_000_000, 200_000]
        )
        .err(),
        Some(Error::Underfunded)
    );
}

#[test]
fn weighted_retention_preserves_custody_and_remains_creator_proceeds() {
    use lbp_core::{QuoteRequest, quote_trade};
    let (input, mut state) = funded();
    state.trade_mode = lbp_core::TradeMode::BuyAndSell;
    state.sell_fee_bps = 2500;
    state.recorded_collateral = 1000;
    state.accrued_fees = 7;
    if let lbp_core::PoolMode::Weighted {
        pricing_collateral, ..
    } = &mut state.mode
    {
        *pricing_collateral = 1000;
    }
    let quote = quote_trade(
        &state,
        QuoteRequest::SellExactInput {
            tokens_in: 100_000,
            min_collateral_out: 0,
        },
        100,
        1500,
    )
    .unwrap();
    let message = Message::SellWeightedExactInput {
        amount_in: 100_000,
        min_amount_out: quote.amount_out,
    };
    assert_eq!(
        operation(
            &input,
            &state,
            message.clone(),
            config(100, id(43), false),
            [1006, 1_000_000, 100_000]
        )
        .err(),
        Some(lbp_core::Error::Underfunded)
    );
    let response = operation(
        &input,
        &state,
        message,
        config(100, id(43), false),
        [1007, 1_000_000, 100_000],
    )
    .unwrap();
    let next = state_of(&response);
    assert_eq!(
        next.recorded_collateral + next.accrued_fees,
        1007 - quote.amount_out
    );
    assert_eq!(
        next.recorded_collateral - 100,
        900 - quote.gross_collateral + quote.retained_fee
    );
    assert_eq!(pricing_collateral(&next), next.recorded_collateral);
    let collected = operation(
        &input,
        &next,
        Message::CollectFees,
        config(8000, id(43), false),
        [1007 - quote.amount_out, 1_100_000, 0],
    )
    .unwrap();
    let next = state_of(&collected);
    assert_eq!(next.accrued_fees, 0);
    let settled = settle(
        &input,
        &next,
        Message::SettleWeighted { creator: id(60) },
        config(8000, id(43), false),
        [next.recorded_collateral, next.remaining_inventory],
    )
    .unwrap();
    let calls: Vec<lez_compat_core::token::Message> = settled
        .sync_messages
        .iter()
        .map(|call| borsh::from_slice(&call.message).unwrap())
        .collect();
    assert!(calls.contains(&lez_compat_core::token::Message::Transfer {
        definition: id(42),
        recipient: id(60),
        amount: next.recorded_collateral
    }));
}

#[test]
fn weighted_live_fee_updates_reject_sells_without_changing_buys_or_controls() {
    use lbp_core::{Error, QuoteRequest, quote_trade};
    let (mut input, mut state) = funded();
    state.trade_mode = lbp_core::TradeMode::BuyAndSell;
    state.sell_fee_bps = 6000;
    for namespace_bps in [4000, 5000, 9999] {
        assert_eq!(
            quote_trade(
                &state,
                QuoteRequest::SellExactInput {
                    tokens_in: 100_000,
                    min_collateral_out: 0
                },
                namespace_bps,
                1500
            ),
            Err(Error::InvalidFee)
        );
        let request = QuoteRequest::BuyExactOutput {
            tokens_out: 1000,
            max_collateral_in: 1_000_000,
        };
        let quoted = quote_trade(&state, request, namespace_bps, 1500).unwrap();
        assert_eq!(quoted.retained_fee, 0);
        let mut no_retention = state.clone();
        no_retention.sell_fee_bps = 0;
        assert_eq!(
            quoted,
            quote_trade(&no_retention, request, namespace_bps, 1500).unwrap()
        );
        let response = operation(
            &input,
            &state,
            Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 1000,
                max_amount_in: 1_000_000,
            },
            config(namespace_bps, id(43), false),
            [100, 1_000_000, 1_000_000],
        )
        .unwrap();
        assert!(matches!(
            sale_events(&response)[0].kind,
            curve_core::events::SaleEventKindV1::Trade {
                retained_fee: 0,
                sell_fee_bps: 6000,
                ..
            }
        ));
    }
    state.trade_control = lbp_core::TradeControl::Paused;
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SetPaused { paused: false }).unwrap();
    let resumed = handle(&input).unwrap();
    assert!(!state_of(&resumed).trade_control.is_paused());
    input.pre_state = resumed.post_state.unwrap();
    input.message = borsh::to_vec(&Message::ReadState).unwrap();
    let _ = handle(&input).unwrap();
}
