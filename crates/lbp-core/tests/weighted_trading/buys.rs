use super::fixture::{
    config, funded, id, operation, operation_at, pricing_collateral, state_of, weight,
    weighted_inputs,
};
use lbp_core::Message;
use weighted_math_core::{calc_in_given_out, calc_out_given_in, interpolate_weight};

#[test]
fn weighted_pricing_ignores_unsolicited_vault_transfers() {
    let (input, state) = funded();
    for message in [
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        Message::BuyWeightedExactOutput {
            membership: None,
            amount_out: 500,
            max_amount_in: 1000,
        },
    ] {
        let quote = |balances| {
            operation(
                &input,
                &state,
                message.clone(),
                config(0, id(43), false),
                balances,
            )
            .unwrap()
        };
        let original = quote([100, state.remaining_inventory, 1000]);
        let donated = quote([1100, state.remaining_inventory + 500_000, 1000]);
        assert_eq!(original.post_state, donated.post_state);
        assert_eq!(original.sync_messages, donated.sync_messages);
    }
}

#[test]
fn weighted_program_rejects_unsafe_and_uncertified_quotes_with_defined_errors() {
    let (input, original) = funded();
    let exact_in = Message::BuyWeightedExactInput {
        membership: None,
        amount_in: 50,
        min_amount_out: 1,
    };
    let run = |state: &lbp_core::PoolAccount, message, balances| {
        operation(&input, state, message, config(0, id(43), false), balances).err()
    };

    for invalid in [0, 100, u128::MAX] {
        let mut state = original.clone();
        if let lbp_core::PoolMode::Weighted {
            token_weight_start, ..
        } = &mut state.mode
        {
            *token_weight_start = invalid;
        }
        assert_eq!(
            run(&state, exact_in.clone(), [100, 1_000_000, 1000]),
            Some(lbp_core::Error::InvalidTerms)
        );
    }

    let mut state = original.clone();
    if let lbp_core::PoolMode::Weighted {
        token_weight_start,
        token_weight_end,
        ..
    } = &mut state.mode
    {
        *token_weight_start = weight(99);
        *token_weight_end = weight(99);
    }
    assert_eq!(
        run(
            &state,
            Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 300_000,
                max_amount_in: u128::MAX,
            },
            [100, 1_000_000, u128::MAX]
        ),
        Some(lbp_core::Error::MathAccuracy)
    );
    assert_eq!(
        run(
            &state,
            Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 300_001,
                max_amount_in: u128::MAX,
            },
            [100, 1_000_000, u128::MAX]
        ),
        Some(lbp_core::Error::InvalidAmount)
    );

    if let lbp_core::PoolMode::Weighted {
        pricing_collateral, ..
    } = &mut state.mode
    {
        *pricing_collateral = u128::MAX;
    }
    state.recorded_collateral = u128::MAX;
    assert_eq!(
        run(&state, exact_in.clone(), [u128::MAX, 1_000_000, 1000]),
        Some(lbp_core::Error::Arithmetic)
    );

    let b = 1u128 << 100;
    state.recorded_collateral = b;
    state.sale_inventory = b;
    state.remaining_inventory = b;
    if let lbp_core::PoolMode::Weighted {
        pricing_collateral, ..
    } = &mut state.mode
    {
        *pricing_collateral = b;
    }
    assert_eq!(
        run(
            &state,
            Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: b / 10 * 3,
                max_amount_in: u128::MAX,
            },
            [b, b, u128::MAX]
        ),
        Some(lbp_core::Error::Arithmetic)
    );
    assert_eq!(
        run(
            &state,
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 1,
                min_amount_out: 0,
            },
            [b, b, 1000]
        ),
        Some(lbp_core::Error::InvalidAmount)
    );
}

#[test]
fn weighted_program_checks_full_inventory_and_post_buy_vault_capacity() {
    let (input, mut state) = funded();
    let buy = Message::BuyWeightedExactInput {
        membership: None,
        amount_in: 50,
        min_amount_out: 1,
    };
    // The vault could pay this trade, but does not back the whole pricing reserve.
    assert_eq!(
        operation(
            &input,
            &state,
            buy.clone(),
            config(100, id(43), false),
            [100, state.remaining_inventory - 1, 1000]
        )
        .err(),
        Some(lbp_core::Error::Underfunded)
    );
    // Surplus stays out of pricing, but a new input must still fit in the vault.
    assert_eq!(
        operation(
            &input,
            &state,
            buy.clone(),
            config(100, id(43), false),
            [u128::MAX, state.remaining_inventory, 1000]
        )
        .err(),
        Some(lbp_core::Error::Arithmetic)
    );
    assert_eq!(
        operation(
            &input,
            &state,
            buy.clone(),
            config(100, id(43), false),
            [99, state.remaining_inventory, 1000]
        )
        .err(),
        Some(lbp_core::Error::Underfunded)
    );
    state.recorded_collateral = u128::MAX;
    state.accrued_fees = 1;
    assert_eq!(
        operation(
            &input,
            &state,
            buy,
            config(100, id(43), false),
            [u128::MAX, state.remaining_inventory, 1000]
        )
        .err(),
        Some(lbp_core::Error::Arithmetic)
    );
}

#[test]
fn weighted_positive_fees_price_net_input_against_constant_product_reference() {
    let (input, mut state) = funded();
    if let lbp_core::PoolMode::Weighted {
        token_weight_start,
        token_weight_end,
        ..
    } = &mut state.mode
    {
        *token_weight_start = weight(50);
        *token_weight_end = weight(50);
    }
    for gross in [50_u128, 100, 250] {
        for fee_bps in [0_u16, 1, 100, 500, 9999] {
            let fee = (gross * u128::from(fee_bps)).div_ceil(10_000);
            let net = gross - fee;
            let result = operation(
                &input,
                &state,
                Message::BuyWeightedExactInput {
                    membership: None,
                    amount_in: gross,
                    min_amount_out: 0,
                },
                config(fee_bps, id(43), false),
                [100, state.remaining_inventory, 1000],
            );
            if net == 0 {
                assert_eq!(result.err(), Some(lbp_core::Error::InvalidAmount));
                continue;
            }
            let next = state_of(&result.unwrap());
            let output = state.remaining_inventory - next.remaining_inventory;
            let reference = state.remaining_inventory * net / (100 + net);
            assert!(output <= reference && reference - output <= 1);
            assert_eq!(next.recorded_collateral, 100 + net);
            assert_eq!(pricing_collateral(&next), next.recorded_collateral);
            assert_eq!(next.accrued_fees, fee);
        }
    }
}

#[test]
fn weighted_same_time_buys_use_updated_reserves_and_collection_keeps_the_quote() {
    let (input, mut state) = funded();
    // Equal weights give an independent constant-product reference.
    if let lbp_core::PoolMode::Weighted {
        token_weight_start,
        token_weight_end,
        ..
    } = &mut state.mode
    {
        *token_weight_start = weight(50);
        *token_weight_end = weight(50);
    }
    let message = Message::BuyWeightedExactInput {
        membership: None,
        amount_in: 50,
        min_amount_out: 1,
    };
    let first = operation(
        &input,
        &state,
        message.clone(),
        config(100, id(43), false),
        [100, state.remaining_inventory, 1000],
    )
    .unwrap();
    let after_first = state_of(&first);
    let first_out = state.remaining_inventory - after_first.remaining_inventory;
    let ideal_first = state.remaining_inventory * 49 / 149;
    assert!(first_out <= ideal_first && ideal_first - first_out <= 1);
    assert_eq!(pricing_collateral(&after_first), 149);
    assert_eq!(after_first.accrued_fees, 1);

    let quote_before_collection = operation(
        &input,
        &after_first,
        message.clone(),
        config(100, id(43), false),
        [150, after_first.remaining_inventory, 950],
    )
    .unwrap();
    let collected = operation(
        &input,
        &after_first,
        Message::CollectFees,
        config(100, id(43), false),
        [150, after_first.remaining_inventory, 950],
    )
    .unwrap();
    let after_collection = state_of(&collected);
    assert_eq!(after_collection.accrued_fees, 0);
    assert_eq!(pricing_collateral(&after_collection), 149);
    let quote_after_collection = operation(
        &input,
        &after_collection,
        message,
        config(100, id(43), false),
        [149, after_collection.remaining_inventory, 950],
    )
    .unwrap();
    let next = state_of(&quote_after_collection);
    assert_eq!(
        next.remaining_inventory,
        state_of(&quote_before_collection).remaining_inventory
    );
    assert_eq!(pricing_collateral(&next), 198);
    let second_out = after_first.remaining_inventory - next.remaining_inventory;
    let ideal_second = after_first.remaining_inventory * 49 / 198;
    assert!(second_out <= ideal_second && ideal_second - second_out <= 1);
    assert!(second_out < first_out);
}

#[test]
fn weighted_buy_exact_in_returns_correct_amount_from_weighted_formula() {
    let (input, state) = funded();
    let inputs = weighted_inputs();
    let amount_in = 50u128;

    let token_w = interpolate_weight(
        inputs.token_weight_start,
        inputs.token_weight_end,
        inputs.start_timestamp,
        inputs.end_timestamp,
        inputs.start_timestamp, // buy at t=start
    );
    let coll_w = lbp_core::WEIGHT_TOTAL - token_w;
    let expected_out = calc_out_given_in(
        inputs.collateral_seed, // collateral_balance = seed at start
        coll_w,
        inputs.sale_inventory,
        token_w,
        amount_in - amount_in.div_ceil(100),
    );
    assert!(expected_out > 0, "expected non-zero output");

    let response = operation_at(
        inputs.start_timestamp * 1000,
        &input,
        &state,
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in,
            min_amount_out: 0,
        },
        config(100, id(43), false),
        [inputs.collateral_seed, inputs.sale_inventory, 1_000],
    )
    .unwrap();
    let next = state_of(&response);
    assert_eq!(
        next.remaining_inventory,
        inputs.sale_inventory - expected_out
    );
}

#[test]
fn weighted_buy_exact_out_charges_correct_amount_from_weighted_formula() {
    let (input, state) = funded();
    let inputs = weighted_inputs();
    let amount_out = 500u128;
    let fee_bps = 100u128;

    let token_w = interpolate_weight(
        inputs.token_weight_start,
        inputs.token_weight_end,
        inputs.start_timestamp,
        inputs.end_timestamp,
        inputs.start_timestamp,
    );
    let coll_w = lbp_core::WEIGHT_TOTAL - token_w;
    let amount_in = calc_in_given_out(
        inputs.collateral_seed,
        coll_w,
        inputs.sale_inventory,
        token_w,
        amount_out,
    );
    let gross_in = (amount_in * 10_000).div_ceil(10_000 - fee_bps);

    let response = operation_at(
        inputs.start_timestamp * 1000,
        &input,
        &state,
        Message::BuyWeightedExactOutput {
            membership: None,
            amount_out,
            max_amount_in: gross_in,
        },
        config(fee_bps as u16, id(43), false),
        [
            inputs.collateral_seed,
            inputs.sale_inventory,
            gross_in + 100,
        ],
    )
    .unwrap();
    let next = state_of(&response);
    assert_eq!(next.remaining_inventory, inputs.sale_inventory - amount_out);
}

#[test]
fn weighted_buy_updates_collateral_balance_in_state() {
    let (input, state) = funded();
    let inputs = weighted_inputs();
    let amount_in = 50u128;
    let fee_bps = 100u128;
    let fee = amount_in.div_ceil(100); // ceil(amount_in * 100 / 10_000)
    let net_in = amount_in - fee;

    let response = operation_at(
        inputs.start_timestamp * 1000,
        &input,
        &state,
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in,
            min_amount_out: 0,
        },
        config(fee_bps as u16, id(43), false),
        [inputs.collateral_seed, inputs.sale_inventory, 1_000],
    )
    .unwrap();
    let next = state_of(&response);
    // recorded_collateral = seed + net_in received from this buy
    assert_eq!(next.recorded_collateral, inputs.collateral_seed + net_in);
    assert_eq!(next.accrued_fees, fee);
}

#[test]
fn weighted_buy_rejects_slippage_exact_in() {
    let (input, state) = funded();
    let inputs = weighted_inputs();
    // set min_amount_out to u128::MAX to guarantee slippage rejection
    assert_eq!(
        operation_at(
            inputs.start_timestamp * 1000,
            &input,
            &state,
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: u128::MAX,
            },
            config(100, id(43), false),
            [inputs.collateral_seed, inputs.sale_inventory, 1_000],
        )
        .err()
        .unwrap(),
        lbp_core::Error::Slippage
    );
}

#[test]
fn weighted_buy_rejects_slippage_exact_out() {
    let (input, state) = funded();
    let inputs = weighted_inputs();
    // set max_amount_in to 0 to guarantee slippage rejection
    assert_eq!(
        operation_at(
            inputs.start_timestamp * 1000,
            &input,
            &state,
            Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 500,
                max_amount_in: 0,
            },
            config(100, id(43), false),
            [inputs.collateral_seed, inputs.sale_inventory, 1_000],
        )
        .err()
        .unwrap(),
        lbp_core::Error::Slippage
    );
}

#[test]
fn weighted_buy_rejects_frozen_pool() {
    let (input, state) = funded();
    let inputs = weighted_inputs();
    assert_eq!(
        operation_at(
            inputs.start_timestamp * 1000,
            &input,
            &state,
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: 0,
            },
            config(100, id(43), true), // frozen
            [inputs.collateral_seed, inputs.sale_inventory, 1_000],
        )
        .err()
        .unwrap(),
        lbp_core::Error::Frozen
    );
}

#[test]
fn weighted_buy_rejects_invalid_fee() {
    let (input, state) = funded();
    let inputs = weighted_inputs();
    assert_eq!(
        operation_at(
            inputs.start_timestamp * 1000,
            &input,
            &state,
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: 0,
            },
            config(10_000, id(43), false), // fee >= 100%
            [inputs.collateral_seed, inputs.sale_inventory, 1_000],
        )
        .err()
        .unwrap(),
        lbp_core::Error::InvalidFee
    );
}

#[test]
fn weighted_buy_rejects_zero_amount_in() {
    let (input, state) = funded();
    let inputs = weighted_inputs();
    assert!(
        operation_at(
            inputs.start_timestamp * 1000,
            &input,
            &state,
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 0,
                min_amount_out: 0,
            },
            config(100, id(43), false),
            [inputs.collateral_seed, inputs.sale_inventory, 1_000],
        )
        .is_err()
    );
}

#[test]
fn weighted_buy_rejects_underfunded_vault() {
    let (input, state) = funded();
    let inputs = weighted_inputs();
    // pool collateral vault shows 0 — below liabilities (= collateral_seed)
    assert_eq!(
        operation_at(
            inputs.start_timestamp * 1000,
            &input,
            &state,
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: 0,
            },
            config(100, id(43), false),
            [0, inputs.sale_inventory, 1_000], // collateral vault = 0 < seed liabilities
        )
        .err()
        .unwrap(),
        lbp_core::Error::Underfunded
    );
}

#[test]
fn weighted_buy_closes_when_remaining_inventory_hits_zero() {
    let (input, mut state) = funded();
    let inputs = weighted_inputs();
    state.remaining_inventory = 0;
    assert_eq!(
        operation_at(
            inputs.start_timestamp * 1000,
            &input,
            &state,
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: 0,
            },
            config(100, id(43), false),
            [inputs.collateral_seed, 0, 1_000],
        )
        .err()
        .unwrap(),
        lbp_core::Error::Closed
    );
}

#[test]
fn weighted_exact_output_gross_limit_and_fee_rounding_use_net_pricing() {
    let (input, mut state) = funded();
    if let lbp_core::PoolMode::Weighted {
        token_weight_start,
        token_weight_end,
        ..
    } = &mut state.mode
    {
        *token_weight_start = weight(50);
        *token_weight_end = weight(50);
    }
    for output in [500_u128, 50_000, 250_000] {
        let ideal_net = (100 * output).div_ceil(state.remaining_inventory - output);
        let required = calc_in_given_out(
            100,
            weight(50),
            state.remaining_inventory,
            weight(50),
            output,
        );
        assert!(required >= ideal_net && required - ideal_net <= 1);
        for fee_bps in [0_u16, 1, 100, 500, 9999] {
            let gross = (required * 10_000).div_ceil(10_000 - u128::from(fee_bps));
            let fee = (gross * u128::from(fee_bps)).div_ceil(10_000);
            let net = gross - fee;
            assert!(net >= required);
            if gross > 0 {
                let lower = gross - 1;
                assert!(lower - (lower * u128::from(fee_bps)).div_ceil(10_000) < required);
                assert_eq!(
                    operation(
                        &input,
                        &state,
                        Message::BuyWeightedExactOutput {
                            membership: None,
                            amount_out: output,
                            max_amount_in: lower,
                        },
                        config(fee_bps, id(43), false),
                        [117, 1_000_007, gross]
                    )
                    .err(),
                    Some(lbp_core::Error::Slippage)
                );
            }
            let response = operation(
                &input,
                &state,
                Message::BuyWeightedExactOutput {
                    membership: None,
                    amount_out: output,
                    max_amount_in: gross,
                },
                config(fee_bps, id(43), false),
                [117, 1_000_007, gross],
            )
            .unwrap();
            let next = state_of(&response);
            assert_eq!(next.recorded_collateral, 100 + net);
            assert_eq!(pricing_collateral(&next), next.recorded_collateral);
            assert_eq!(next.accrued_fees, fee);
            assert_eq!(next.remaining_inventory, state.remaining_inventory - output);
        }
    }
}
