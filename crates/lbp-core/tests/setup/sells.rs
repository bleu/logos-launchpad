use super::fixture::{
    config, exact_in, funded, id, operation, prepare, sale_events, settle, state_of,
};
use lbp_core::{FixedPrice, Message, handle};
use lee_core::account::Actor;

#[test]
fn fixed_sells_use_reverse_floor_and_record_gross_output_fee_separately() {
    use curve_core::events::{SaleEventKindV1, TradeFormV1};
    use lbp_core::{QuoteRequest, quote_trade};
    let (input, mut state) = funded();
    state.trade_mode = lbp_core::TradeMode::BuyAndSell;
    state.recorded_collateral = 100;
    state.accrued_fees = 3;
    for retained_bps in [0_u16, 1, 250, 5000] {
        state.sell_fee_bps = retained_bps;
        for fee_bps in [0_u16, 100, 3333] {
            let request = QuoteRequest::SellExactInput {
                tokens_in: 20,
                min_collateral_out: 0,
            };
            let quoted = quote_trade(&state, request, fee_bps, 100).unwrap();
            // The saved price is three collateral units for two token units.
            assert_eq!(quoted.gross_collateral, 30);
            assert_eq!(quoted.amount_in, 20);
            assert_eq!(
                quoted.amount_out,
                30 - (30 * u128::from(fee_bps)).div_ceil(10_000)
                    - (30 * u128::from(retained_bps)).div_ceil(10_000)
            );
            assert_eq!(
                quoted.retained_fee,
                (30 * u128::from(retained_bps)).div_ceil(10_000)
            );
            let response = operation(
                &input,
                &state,
                Message::SellExactInput {
                    amount_in: 20,
                    min_amount_out: quoted.amount_out,
                    now: 100,
                },
                config(fee_bps, id(43), false),
                [1103, 800, 20],
            )
            .unwrap();
            let next = state_of(&response);
            assert_eq!(next.recorded_collateral, 70 + quoted.retained_fee);
            assert_eq!(next.accrued_fees, 3 + quoted.protocol_fee);
            assert_eq!(next.remaining_inventory, 820);
            assert_eq!(response.validity.timestamps.start(), Some(100_000));
            assert_eq!(response.validity.timestamps.end(), Some(200_000));
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
                        amount: 20
                    },
                    lez_compat_core::token::Message::Transfer {
                        definition: id(42),
                        recipient: id(50),
                        amount: quoted.amount_out
                    },
                ]
            );
            assert_eq!(
                sale_events(&response)[0].kind,
                SaleEventKindV1::Trade {
                    trader: *id(50).value(),
                    form: TradeFormV1::ExactInput,
                    token_in: *id(41).value(),
                    token_out: *id(42).value(),
                    amount_in: 20,
                    pricing_input: 20,
                    gross_output: 30,
                    amount_out: quoted.amount_out,
                    fee_definition: *id(42).value(),
                    fee_amount: quoted.protocol_fee,
                    fee_bps,
                    retained_fee: quoted.retained_fee,
                    sell_fee_bps: retained_bps,
                    pricing_time: None,
                }
            );
        }
    }
    state.sell_fee_bps = 0;
    assert_eq!(
        quote_trade(
            &state,
            QuoteRequest::SellExactInput {
                tokens_in: 3,
                min_collateral_out: 0
            },
            0,
            100
        )
        .unwrap()
        .gross_collateral,
        4
    );
}

#[test]
fn fixed_sells_reject_unavailable_budget_limits_and_invalid_lifecycle() {
    use lbp_core::{Error, QuoteRequest, quote_trade};
    let (input, mut state) = funded();
    let request = QuoteRequest::SellExactInput {
        tokens_in: 20,
        min_collateral_out: 0,
    };
    assert_eq!(
        quote_trade(&state, request, 0, 100),
        Err(Error::TradeDirectionUnsupported)
    );
    state.trade_mode = lbp_core::TradeMode::BuyAndSell;
    state.recorded_collateral = 29;
    state.accrued_fees = 100;
    let message = Message::SellExactInput {
        amount_in: 20,
        min_amount_out: 0,
        now: 100,
    };
    assert_eq!(
        operation(
            &input,
            &state,
            message.clone(),
            config(0, id(43), false),
            [10_000, 800, 20]
        )
        .err(),
        Some(Error::Underfunded)
    );
    state.recorded_collateral = 100;
    assert_eq!(
        operation(
            &input,
            &state,
            message.clone(),
            config(0, id(43), true),
            [200, 800, 20]
        )
        .err(),
        Some(Error::Frozen)
    );
    for balances in [[199, 800, 20], [200, 799, 20], [200, 800, 19]] {
        assert_eq!(
            operation(
                &input,
                &state,
                message.clone(),
                config(0, id(43), false),
                balances
            )
            .err(),
            Some(Error::Underfunded)
        );
    }
    assert_eq!(
        quote_trade(
            &state,
            QuoteRequest::SellExactInput {
                tokens_in: 20,
                min_collateral_out: 31
            },
            0,
            100
        ),
        Err(Error::Slippage)
    );
    for time in [99, 200] {
        assert_eq!(quote_trade(&state, request, 0, time), Err(Error::Closed));
    }
    assert_eq!(
        quote_trade(&state, request, 10_000, 100),
        Err(Error::InvalidFee)
    );
    for progress in [
        lbp_core::SettlementProgress::ManualStarted,
        lbp_core::SettlementProgress::ManualComplete,
        lbp_core::SettlementProgress::Graduated,
    ] {
        let mut completed = state.clone();
        completed.settlement_progress = progress;
        assert_eq!(quote_trade(&completed, request, 0, 100), Err(Error::Closed));
    }
    state.trade_control = lbp_core::TradeControl::Paused;
    assert_eq!(quote_trade(&state, request, 0, 100), Err(Error::Paused));
}

#[test]
fn fixed_scheduled_sellout_can_replenish_but_selected_depletion_is_final() {
    let (input, original) = funded();
    for ending in [
        lbp_core::EndingPolicy::Scheduled,
        lbp_core::EndingPolicy::Depletion,
        lbp_core::EndingPolicy::Either,
    ] {
        let mut state = original.clone();
        state.trade_mode = lbp_core::TradeMode::BuyAndSell;
        state.ending_policy = ending;
        state.remaining_inventory = 0;
        state.recorded_collateral = 100;
        if ending == lbp_core::EndingPolicy::Depletion {
            state.end_timestamp = None;
        }
        let result = operation(
            &input,
            &state,
            Message::SellExactInput {
                amount_in: 20,
                min_amount_out: 30,
                now: 100,
            },
            config(0, id(43), false),
            [100, 0, 20],
        );
        if ending == lbp_core::EndingPolicy::Scheduled {
            let response = result.unwrap();
            let next = state_of(&response);
            assert_eq!(next.remaining_inventory, 20);
            assert!(!next.closure_observed);
            let buy = operation(
                &input,
                &next,
                exact_in(5),
                config(0, id(43), false),
                [70, 20, 5],
            )
            .unwrap();
            assert_eq!(state_of(&buy).remaining_inventory, 17);
        } else {
            assert_eq!(result.err(), Some(lbp_core::Error::Closed));
        }
    }
}

#[test]
fn fixed_sell_bypasses_buy_membership_but_keeps_participant_and_callback_authentication() {
    let (mut input, mut state) = funded();
    state.trade_mode = lbp_core::TradeMode::BuyAndSell;
    state.recorded_collateral = 100;
    state.buy_policy = lbp_core::BuyPolicy::Allowlist(lbp_core::AllowlistPolicy {
        version: 1,
        namespace: *state.namespace.value(),
        sale_policy_id: *input.receiver.account_id.value(),
        root: [9; 32],
    });
    let message = Message::SellExactInput {
        amount_in: 20,
        min_amount_out: 0,
        now: 100,
    };
    let _ = operation(
        &input,
        &state,
        message.clone(),
        config(0, id(43), false),
        [100, 800, 20],
    )
    .unwrap();
    for from in [None, Some(input.receiver), Some(Actor::new(id(50), id(99)))] {
        input.from = from;
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.message = borsh::to_vec(&message).unwrap();
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
    }
}

#[test]
fn fixed_sell_execute_is_appended_and_requires_transaction_authority() {
    let mut input = prepare();
    input.receiver = Actor::new(id(50), input.receiver.program_account_id);
    input.pre_state = vec![].into();
    let sell = Message::SellExactInput {
        amount_in: 20,
        min_amount_out: 0,
        now: 100,
    };
    assert_eq!(borsh::to_vec(&sell).unwrap()[0], 18);
    assert_eq!(
        borsh::to_vec(&Message::SellWeightedExactInput {
            amount_in: 20,
            min_amount_out: 0
        })
        .unwrap()[0],
        19
    );
    input.message = borsh::to_vec(&Message::Execute {
        pool: id(90),
        instruction: Box::new(sell.clone()),
    })
    .unwrap();
    input.is_authorized = false;
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
    input.is_authorized = true;
    let response = handle(&input).unwrap();
    assert_eq!(response.sync_messages[0].to.account_id, id(90));
    assert_eq!(
        borsh::from_slice::<Message>(&response.sync_messages[0].message).unwrap(),
        sell
    );
}

#[test]
fn fixed_retained_collateral_stays_in_proceeds_after_fee_collection() {
    use lbp_core::{QuoteRequest, quote_trade};
    let (input, mut state) = funded();
    state.trade_mode = lbp_core::TradeMode::BuyAndSell;
    state.sell_fee_bps = 1000;
    state.recorded_collateral = 100;
    state.accrued_fees = 3;
    let quoted = quote_trade(
        &state,
        QuoteRequest::SellExactInput {
            tokens_in: 20,
            min_collateral_out: 0,
        },
        100,
        100,
    )
    .unwrap();
    assert_eq!(
        (
            quoted.gross_collateral,
            quoted.protocol_fee,
            quoted.retained_fee,
            quoted.amount_out
        ),
        (30, 1, 3, 26)
    );
    let message = Message::SellExactInput {
        amount_in: 20,
        min_amount_out: 26,
        now: 100,
    };
    assert_eq!(
        operation(
            &input,
            &state,
            message.clone(),
            config(100, id(43), false),
            [102, 800, 20]
        )
        .err(),
        Some(lbp_core::Error::Underfunded)
    );
    let response = operation(
        &input,
        &state,
        message,
        config(100, id(43), false),
        [103, 800, 20],
    )
    .unwrap();
    let next = state_of(&response);
    assert_eq!((next.recorded_collateral, next.accrued_fees), (73, 4));
    assert_eq!(next.recorded_collateral, 73);
    let collected = operation(
        &input,
        &next,
        Message::CollectFees,
        config(9900, id(43), false),
        [77, 820, 0],
    )
    .unwrap();
    let next = state_of(&collected);
    assert_eq!((next.recorded_collateral, next.accrued_fees), (73, 0));
    let settled = settle(&input, &next, 200, config(9900, id(43), false), [73, 820]).unwrap();
    let payouts: Vec<lez_compat_core::token::Message> = settled
        .sync_messages
        .iter()
        .map(|call| borsh::from_slice(&call.message).unwrap())
        .collect();
    assert!(
        payouts.contains(&lez_compat_core::token::Message::Transfer {
            definition: id(42),
            recipient: id(60),
            amount: 73
        })
    );
    assert_eq!(state_of(&settled).recorded_collateral, 0);
}

#[test]
fn fixed_live_namespace_rate_only_combines_with_retention_for_sells() {
    use lbp_core::{Error, QuoteRequest, quote_trade};
    let (mut input, mut state) = funded();
    state.trade_mode = lbp_core::TradeMode::BuyAndSell;
    state.sell_fee_bps = 6000;
    state.recorded_collateral = 100;
    state.trade_control = lbp_core::TradeControl::Running;
    for namespace_bps in [4000, 5000, 9999] {
        assert_eq!(
            quote_trade(
                &state,
                QuoteRequest::SellExactInput {
                    tokens_in: 20,
                    min_collateral_out: 0
                },
                namespace_bps,
                100
            ),
            Err(Error::InvalidFee)
        );
        for request in [
            QuoteRequest::BuyExactInput {
                collateral_in: 100 * (10_000 / u128::from(10_000 - namespace_bps)),
                min_tokens_out: 0,
            },
            QuoteRequest::BuyExactOutput {
                tokens_out: 1,
                max_collateral_in: 100_000,
            },
        ] {
            let mut no_retention = state.clone();
            no_retention.sell_fee_bps = 0;
            let quoted = quote_trade(&state, request, namespace_bps, 100).unwrap();
            assert_eq!(quoted.retained_fee, 0);
            assert_eq!(
                quoted,
                quote_trade(&no_retention, request, namespace_bps, 100).unwrap()
            );
        }
        let bought = operation(
            &input,
            &state,
            Message::BuyExactOutput {
                membership: None,
                amount_out: 1,
                max_amount_in: 100_000,
                now: 100,
            },
            config(namespace_bps, id(43), false),
            [100, 800, 100_000],
        )
        .unwrap();
        let event = sale_events(&bought).remove(0);
        assert!(matches!(
            event.kind,
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
    let resumed = state_of(&resumed);
    assert!(!resumed.trade_control.is_paused());
    input.pre_state = borsh::to_vec(&resumed).unwrap().into();
    input.message = borsh::to_vec(&Message::ReadState).unwrap();
    let _ = handle(&input).unwrap();
}

#[test]
fn fixed_retention_uses_independent_ceilings_and_checks_overflow() {
    use lbp_core::{Error, QuoteRequest, quote_trade};
    let (_, mut state) = funded();
    state.trade_mode = lbp_core::TradeMode::BuyAndSell;
    state.sell_fee_bps = 1;
    state.price = FixedPrice {
        collateral_units: 1,
        token_units: 1,
    };
    state.recorded_collateral = 10;
    let request = |tokens_in| QuoteRequest::SellExactInput {
        tokens_in,
        min_collateral_out: 0,
    };
    for tokens in [0, 1, 2] {
        assert_eq!(
            quote_trade(&state, request(tokens), 1, 100),
            Err(Error::InvalidAmount)
        );
    }
    let quote = quote_trade(&state, request(3), 1, 100).unwrap();
    assert_eq!(
        (quote.protocol_fee, quote.retained_fee, quote.amount_out),
        (1, 1, 1)
    );
    assert_eq!(
        quote_trade(
            &state,
            QuoteRequest::SellExactInput {
                tokens_in: 3,
                min_collateral_out: 2
            },
            1,
            100
        ),
        Err(Error::Slippage)
    );
    state.remaining_inventory = u128::MAX;
    assert_eq!(
        quote_trade(&state, request(3), 1, 100),
        Err(Error::Arithmetic)
    );
    state.remaining_inventory = 800;
    state.recorded_collateral = u128::MAX;
    state.accrued_fees = 1;
    assert_eq!(
        quote_trade(&state, request(3), 1, 100),
        Err(Error::Arithmetic)
    );
}

#[test]
fn fixed_rejects_invalid_stored_sell_fee_without_live_rate_combination() {
    let mut input = prepare();
    let Message::PrepareFixedPrice { mut inputs } = borsh::from_slice(&input.message).unwrap()
    else {
        unreachable!()
    };
    for invalid in [10_000, u16::MAX] {
        inputs.sell_fee_bps = invalid;
        input.message = borsh::to_vec(&Message::PrepareFixedPrice {
            inputs: inputs.clone(),
        })
        .unwrap();
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidTerms));
    }
    let (mut input, mut state) = funded();
    state.sell_fee_bps = 10_000;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::ReadState).unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidState));
}
