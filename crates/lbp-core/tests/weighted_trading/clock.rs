use super::fixture::{
    config, funded, holding, id, operation, operation_at, pricing_collateral, reply, state_of,
    weight, weighted_inputs,
};
use lbp_core::{Message, handle};
use lee_core::account::Actor;
use lee_core::program::ReceiveInput;
use weighted_math_core::{calc_in_given_out, calc_out_given_in, interpolate_weight};

#[test]
fn weighted_buys_cannot_bypass_clock_reads_with_fixed_price_messages() {
    let (input, state) = funded();
    for message in [
        Message::BuyExactInput {
            membership: None,
            amount_in: 50,
            min_amount_out: 1,
            now: 1500,
        },
        Message::BuyExactOutput {
            membership: None,
            amount_out: 500,
            max_amount_in: 1000,
            now: 1500,
        },
    ] {
        assert_eq!(
            operation(
                &input,
                &state,
                message,
                config(0, id(43), false),
                [100, 1_000_000, 1000]
            )
            .err(),
            Some(lbp_core::Error::InvalidMessage)
        );
    }
}

#[test]
fn weighted_buy_rejects_before_and_after_sale_window() {
    let (input, state) = funded();
    let inputs = weighted_inputs();
    for now in [
        inputs.start_timestamp - 1,
        inputs.end_timestamp,
        inputs.end_timestamp + 1,
    ] {
        assert_eq!(
            operation_at(
                now * 1000,
                &input,
                &state,
                Message::BuyWeightedExactInput {
                    membership: None,
                    amount_in: 50,
                    min_amount_out: 0,
                },
                config(100, id(43), false),
                [inputs.collateral_seed, inputs.sale_inventory, 1_000],
            )
            .err()
            .unwrap(),
            lbp_core::Error::Closed,
            "now={now}"
        );
    }
}

#[test]
fn weighted_buy_prices_fall_as_weights_shift() {
    // With unchanged reserves, a lower token weight gives more tokens for the same input.
    let (input, state) = funded();
    let inputs = weighted_inputs();

    let buy = |now: u64| -> u128 {
        let response = operation_at(
            now * 1000,
            &input,
            &state,
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: 0,
            },
            config(100, id(43), false),
            [inputs.collateral_seed, inputs.sale_inventory, 1_000],
        )
        .unwrap();
        let next = state_of(&response);
        inputs.sale_inventory - next.remaining_inventory
    };

    let out_at_start = buy(inputs.start_timestamp);
    let out_at_end = buy(inputs.end_timestamp - 1); // just before end
    assert!(
        out_at_end > out_at_start,
        "expected more tokens at end (price falling): start={out_at_start}, end={out_at_end}"
    );
}

#[test]
fn normalized_weights_keep_fractional_percentage_schedule_steps() {
    let (input, mut state) = funded();
    let lbp_core::PoolMode::Weighted {
        token_weight_start,
        token_weight_end,
        ..
    } = &mut state.mode
    else {
        unreachable!()
    };
    *token_weight_start = weight(50);
    *token_weight_end = weight(51);
    let quote = |second: u64| {
        operation_at(
            second * 1000,
            &input,
            &state,
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 10,
                min_amount_out: 1,
            },
            config(0, id(43), false),
            [100, state.remaining_inventory, 1000],
        )
        .unwrap()
    };
    let first = state_of(&quote(1500));
    let second = state_of(&quote(1501));
    // Both timestamps are between 50% and 51%; a whole-percent schedule would be flat.
    assert_ne!(first.remaining_inventory, second.remaining_inventory);
    let collateral_weight = weight(50) - weight(1) * 500 / 2000;
    let expected = calc_out_given_in(
        100,
        collateral_weight,
        state.remaining_inventory,
        lbp_core::WEIGHT_TOTAL - collateral_weight,
        10,
    );
    assert_eq!(
        state.remaining_inventory - first.remaining_inventory,
        expected
    );
}

#[test]
fn clock_read_prices_both_forms_and_preserves_sale_inclusion_bounds() {
    let (input, base) = funded();
    for (start, end) in [(95, 50), (50, 95), (50, 50)] {
        let mut state = base.clone();
        if let lbp_core::PoolMode::Weighted {
            token_weight_start,
            token_weight_end,
            ..
        } = &mut state.mode
        {
            *token_weight_start = weight(start);
            *token_weight_end = weight(end);
        }
        for ms in [1_000_000, 1_500_000, 1_500_999, 2_999_999] {
            let c = interpolate_weight(
                weight(100 - start),
                weight(100 - end),
                1000,
                3000,
                ms / 1000,
            );
            let out = calc_out_given_in(100, c, 1_000_000, lbp_core::WEIGHT_TOTAL - c, 50);
            let amount_in = calc_in_given_out(100, c, 1_000_000, lbp_core::WEIGHT_TOTAL - c, 500);
            for (message, expected_out, expected_in) in [
                (
                    Message::BuyWeightedExactInput {
                        membership: None,
                        amount_in: 50,
                        min_amount_out: out,
                    },
                    out,
                    50,
                ),
                (
                    Message::BuyWeightedExactOutput {
                        membership: None,
                        amount_out: 500,
                        max_amount_in: amount_in,
                    },
                    500,
                    amount_in,
                ),
            ] {
                let r = operation_at(
                    ms,
                    &input,
                    &state,
                    message,
                    config(0, id(43), false),
                    [100, 1_000_000, 10000],
                )
                .unwrap();
                assert_eq!(state_of(&r).remaining_inventory, 1_000_000 - expected_out);
                assert_eq!(pricing_collateral(&state_of(&r)), 100 + expected_in);
                assert_eq!(r.validity.timestamps.start(), Some(1_000_000));
                assert_eq!(r.validity.timestamps.end(), Some(3_000_000));
            }
        }
    }
}

#[test]
fn clock_replies_require_the_expected_actor_and_pending_phase() {
    let (mut input, state) = funded();
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::BuyWeightedExactInput {
        membership: None,
        amount_in: 10,
        min_amount_out: 1,
    })
    .unwrap();
    let mut response = handle(&input).unwrap();
    let early = ReceiveInput {
        receiver: input.receiver,
        from: Some(lbp_core::pricing_clock::actor()),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&clock_core::Reply::Timestamp(1_500_000)).unwrap(),
    };
    assert_eq!(handle(&early).err(), Some(lbp_core::Error::Unauthorized));

    for bytes in [
        config(0, id(43), false),
        holding(input.receiver.account_id, id(42), 100),
        holding(input.receiver.account_id, id(41), 1_000_000),
        holding(id(50), id(42), 1000),
    ] {
        let from = response.sync_messages[0].to;
        input = reply(&input, &response, from, bytes);
        response = handle(&input).unwrap();
    }
    assert_eq!(
        response.sync_messages[0].to,
        lbp_core::pricing_clock::actor()
    );
    input.pre_state = response.post_state.unwrap();
    input.message = borsh::to_vec(&clock_core::Reply::Timestamp(1_500_000)).unwrap();
    for actor in [
        Actor::new(id(1), clock_core::clock_account_id()),
        Actor::new(clock_core::CLOCK_PROGRAM_ACCOUNT_ID, id(1)),
    ] {
        input.from = Some(actor);
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
    }
    input.from = Some(lbp_core::pricing_clock::actor());
    let done = handle(&input).unwrap();
    let mut malformed = input.clone();
    malformed.message = vec![0];
    assert_eq!(
        handle(&malformed).err(),
        Some(lbp_core::Error::InvalidMessage)
    );
    let mut wrong_kind = input.clone();
    wrong_kind.message = borsh::to_vec(&clock_core::Reply::BlockId(1500)).unwrap();
    assert_eq!(
        handle(&wrong_kind).err(),
        Some(lbp_core::Error::InvalidMessage)
    );
    input.pre_state = done.post_state.unwrap();
    assert!(handle(&input).is_err(), "duplicate reply must fail");
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    assert!(handle(&input).is_err(), "unsolicited reply must fail");
}

#[test]
fn weighted_schedule_bounds_reject_millisecond_overflow() {
    let (input, mut state) = funded();
    state.end_timestamp = Some(u64::MAX);
    assert_eq!(
        operation(
            &input,
            &state,
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 10,
                min_amount_out: 1
            },
            config(0, id(43), false),
            [100, 1_000_000, 1000]
        )
        .err(),
        Some(lbp_core::Error::InvalidTerms)
    );
}

#[test]
fn weighted_exact_time_edges_apply_to_both_buy_forms() {
    let (input, state) = funded();
    for timestamp_ms in [999_999, 1_000_000, 2_999_999, 3_000_000] {
        for message in [
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: 0,
            },
            Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 500,
                max_amount_in: 1000,
            },
        ] {
            let result = operation_at(
                timestamp_ms,
                &input,
                &state,
                message,
                config(100, id(43), false),
                [100, 1_000_000, 1000],
            );
            if (1_000_000..3_000_000).contains(&timestamp_ms) {
                let response = result.unwrap();
                assert_eq!(response.validity.timestamps.start(), Some(1_000_000));
                assert_eq!(response.validity.timestamps.end(), Some(3_000_000));
            } else {
                assert_eq!(result.err(), Some(lbp_core::Error::Closed));
            }
        }
    }
}
