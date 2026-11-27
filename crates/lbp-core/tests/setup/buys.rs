use super::fixture::{
    config, exact_in, funded, holding, id, namespace_state, operation, prepare, reply, state_of,
};
use lbp_core::{FixedPrice, Message, handle};
use lee_core::account::Actor;

#[test]
fn buy_windows_keep_zero_start_no_end_and_reject_overflow() {
    let (input, mut state) = funded();
    state.start_timestamp = 0;
    state.end_timestamp = None;
    state.ending_policy = lbp_core::EndingPolicy::Depletion;
    let response = operation(
        &input,
        &state,
        exact_in(5),
        config(100, id(43), false),
        [0, 800, 100],
    )
    .unwrap();
    assert_eq!(response.validity.timestamps.start(), Some(0));
    assert_eq!(response.validity.timestamps.end(), None);
    state.end_timestamp = Some(u64::MAX);
    assert_eq!(
        operation(
            &input,
            &state,
            exact_in(5),
            config(100, id(43), false),
            [0, 800, 100]
        )
        .err(),
        Some(lbp_core::Error::InvalidTerms)
    );
    state.start_timestamp = u64::MAX;
    state.end_timestamp = None;
    state.ending_policy = lbp_core::EndingPolicy::Depletion;
    let message = Message::BuyExactInput {
        membership: None,
        amount_in: 5,
        min_amount_out: 0,
        now: u64::MAX,
    };
    assert_eq!(
        operation(
            &input,
            &state,
            message,
            config(100, id(43), false),
            [0, 800, 100]
        )
        .err(),
        Some(lbp_core::Error::InvalidTerms)
    );
}

#[test]
fn buys_bind_inclusion_to_saved_start_and_end() {
    let (input, state) = funded();
    for message in [
        exact_in(5),
        Message::BuyExactOutput {
            membership: None,
            amount_out: 3,
            max_amount_in: 6,
            now: 100,
        },
    ] {
        let response = operation(
            &input,
            &state,
            message,
            config(100, id(43), false),
            [0, 800, 100],
        )
        .unwrap();
        assert_eq!(response.validity.timestamps.start(), Some(100_000));
        assert_eq!(response.validity.timestamps.end(), Some(200_000));
    }
}

#[test]
fn buys_deduct_live_fees_before_pricing_and_gross_up_exact_output() {
    let (input, state) = funded();
    for (message, fee, paid, received, accrued) in [
        (exact_in(5), 100, 5, 2, 1),
        (exact_in(5), 0, 5, 3, 0),
        (exact_in(100), 100, 100, 66, 1),
        (exact_in(101), 100, 101, 66, 2),
        (
            Message::BuyExactOutput {
                membership: None,
                amount_out: 3,
                max_amount_in: 6,
                now: 100,
            },
            100,
            6,
            3,
            1,
        ),
        (
            Message::BuyExactOutput {
                membership: None,
                amount_out: 3,
                max_amount_in: 5,
                now: 100,
            },
            0,
            5,
            3,
            0,
        ),
    ] {
        let response = operation(
            &input,
            &state,
            message,
            config(fee, id(43), false),
            [17, 900, 101],
        )
        .unwrap();
        let next = state_of(&response);
        assert_eq!(next.remaining_inventory, 800 - received);
        assert_eq!(next.sale_inventory, 800);
        assert_eq!(next.recorded_collateral, paid - accrued);
        assert_eq!(next.accrued_fees, accrued);
        assert_eq!(response.sync_messages.len(), 2);
        assert_eq!(
            response.sync_messages[0].to,
            Actor::new(id(50), curve_core::TOKEN_PROGRAM_ACCOUNT_ID)
        );
        assert_eq!(
            response.sync_messages[1].to,
            Actor::new(
                input.receiver.account_id,
                curve_core::TOKEN_PROGRAM_ACCOUNT_ID
            )
        );
        for (call, definition, recipient, amount) in [
            (
                &response.sync_messages[0],
                id(42),
                input.receiver.account_id,
                paid,
            ),
            (&response.sync_messages[1], id(41), id(50), received),
        ] {
            assert_eq!(
                borsh::from_slice::<lez_compat_core::token::Message>(&call.message).unwrap(),
                lez_compat_core::token::Message::Transfer {
                    definition,
                    recipient,
                    amount
                }
            );
        }
        assert!(response.sync_messages[0].pda_seeds.is_empty());
        assert_eq!(
            response.sync_messages[1].pda_seeds,
            [lbp_core::compute_pool_pda_seed(
                state.namespace,
                id(41),
                id(42),
                state.factory_owner
            )]
            .into_iter()
            .collect()
        );
        assert_eq!(response.validity.timestamps.start(), Some(100_000));
        assert_eq!(response.validity.timestamps.end(), Some(200_000));
    }
}

#[test]
fn exact_output_payment_is_minimal_across_prices_and_fee_rates() {
    let (input, mut state) = funded();
    for (collateral_units, token_units) in [(3, 2), (2, 3), (1, 1), (7, 4)] {
        state.price = FixedPrice {
            collateral_units,
            token_units,
        };
        for rate in [0_u16, 1, 100, 1_001, 5_000, 9_999] {
            for amount_out in [1_u128, 2, 3, 13, 799, 800] {
                let required_net = (amount_out * collateral_units).div_ceil(token_units);
                let gross = (required_net * 10_000).div_ceil(10_000 - u128::from(rate));
                let fee = (gross * u128::from(rate)).div_ceil(10_000);
                let message = Message::BuyExactOutput {
                    membership: None,
                    amount_out,
                    max_amount_in: gross,
                    now: 100,
                };
                let response = operation(
                    &input,
                    &state,
                    message.clone(),
                    config(rate, id(43), false),
                    [17, 800, gross],
                )
                .unwrap();
                let next = state_of(&response);
                assert_eq!(next.remaining_inventory, 800 - amount_out);
                assert_eq!(next.recorded_collateral, required_net);
                assert_eq!(next.accrued_fees, fee);
                assert_eq!(next.recorded_collateral + next.accrued_fees, gross);
                assert_eq!(
                    borsh::from_slice::<lez_compat_core::token::Message>(
                        &response.sync_messages[0].message
                    )
                    .unwrap(),
                    lez_compat_core::token::Message::Transfer {
                        definition: id(42),
                        recipient: input.receiver.account_id,
                        amount: gross,
                    }
                );
                let previous_gross = gross - 1;
                let previous_fee = (previous_gross * u128::from(rate)).div_ceil(10_000);
                assert!(previous_gross - previous_fee < required_net);
                assert_eq!(
                    operation(
                        &input,
                        &state,
                        Message::BuyExactOutput {
                            membership: None,
                            amount_out,
                            max_amount_in: previous_gross,
                            now: 100,
                        },
                        config(rate, id(43), false),
                        [17, 800, gross],
                    )
                    .err(),
                    Some(lbp_core::Error::Slippage)
                );
                assert_eq!(
                    operation(
                        &input,
                        &state,
                        message,
                        config(rate, id(43), false),
                        [17, 800, previous_gross],
                    )
                    .err(),
                    Some(lbp_core::Error::Underfunded)
                );
            }
        }
    }
}

#[test]
fn exact_input_rejects_fee_consumed_dust_and_prices_only_net_collateral() {
    let (input, mut state) = funded();
    state.price = FixedPrice {
        collateral_units: 1,
        token_units: 1,
    };
    for rate in [1_u16, 100, 5_000, 9_999] {
        assert_eq!(
            operation(
                &input,
                &state,
                exact_in(1),
                config(rate, id(43), false),
                [0, 800, 1],
            )
            .err(),
            Some(lbp_core::Error::InvalidAmount)
        );
    }
    let response = operation(
        &input,
        &state,
        exact_in(10_000),
        config(9_999, id(43), false),
        [0, 800, 10_000],
    )
    .unwrap();
    let next = state_of(&response);
    assert_eq!(next.remaining_inventory, 799);
    assert_eq!(next.recorded_collateral, 1);
    assert_eq!(next.accrued_fees, 9_999);
    assert_eq!(
        operation(
            &input,
            &state,
            Message::BuyExactInput {
                membership: None,
                amount_in: 100,
                min_amount_out: 100,
                now: 100,
            },
            config(100, id(43), false),
            [0, 800, 100],
        )
        .err(),
        Some(lbp_core::Error::Slippage)
    );
}

#[test]
fn buys_reject_limits_zero_inventory_time_freeze_and_underbacking() {
    let (input, state) = funded();
    for message in [
        exact_in(0),
        exact_in(1),
        exact_in(1_215),
        Message::BuyExactInput {
            membership: None,
            amount_in: 5,
            min_amount_out: 3,
            now: 100,
        },
        Message::BuyExactOutput {
            membership: None,
            amount_out: 3,
            max_amount_in: 4,
            now: 100,
        },
        Message::BuyExactOutput {
            membership: None,
            amount_out: 0,
            max_amount_in: 100,
            now: 100,
        },
        Message::BuyExactOutput {
            membership: None,
            amount_out: 801,
            max_amount_in: 2_000,
            now: 100,
        },
        Message::BuyExactInput {
            membership: None,
            amount_in: 5,
            min_amount_out: 0,
            now: 99,
        },
        Message::BuyExactInput {
            membership: None,
            amount_in: 5,
            min_amount_out: 0,
            now: 200,
        },
    ] {
        assert!(
            operation(
                &input,
                &state,
                message,
                config(100, id(43), false),
                [0, 900, 2_000]
            )
            .is_err()
        );
    }
    assert_eq!(
        operation(
            &input,
            &state,
            exact_in(5),
            config(100, id(43), true),
            [0, 800, 100]
        )
        .err()
        .unwrap(),
        lbp_core::Error::Frozen
    );
    assert_eq!(
        operation(
            &input,
            &state,
            exact_in(5),
            config(10_000, id(43), false),
            [0, 800, 100]
        )
        .err()
        .unwrap(),
        lbp_core::Error::InvalidFee
    );
    for balances in [[0, 1, 100], [0, 800, 4]] {
        assert_eq!(
            operation(
                &input,
                &state,
                exact_in(5),
                config(100, id(43), false),
                balances
            )
            .err()
            .unwrap(),
            lbp_core::Error::Underfunded
        );
    }
    let mut state = state;
    state.recorded_collateral = 5;
    state.accrued_fees = 1;
    for message in [exact_in(5), Message::CollectFees] {
        assert_eq!(
            operation(
                &input,
                &state,
                message,
                config(100, id(43), false),
                [5, 800, 100]
            )
            .err()
            .unwrap(),
            lbp_core::Error::Underfunded
        );
    }
}

#[test]
fn no_end_sellout_donations_and_anytime_collection_keep_separate_accounts() {
    let (mut input, mut state) = funded();
    state.end_timestamp = None;
    state.ending_policy = lbp_core::EndingPolicy::Depletion;
    let response = operation(
        &input,
        &state,
        Message::BuyExactOutput {
            membership: None,
            amount_out: 800,
            max_amount_in: 1_213,
            now: 500,
        },
        config(100, id(43), false),
        [17, 900, 1_213],
    )
    .unwrap();
    state = state_of(&response);
    assert_eq!(
        (
            state.remaining_inventory,
            state.recorded_collateral,
            state.accrued_fees
        ),
        (0, 1_200, 13)
    );
    assert_eq!(response.validity.timestamps.start(), Some(100_000));
    assert_eq!(response.validity.timestamps.end(), None);
    assert_eq!(
        operation(
            &input,
            &state,
            exact_in(5),
            config(100, id(43), false),
            [1_230, 100, 100]
        )
        .err()
        .unwrap(),
        lbp_core::Error::Closed
    );
    input.from = None;
    let response = operation(
        &input,
        &state,
        Message::CollectFees,
        config(100, id(70), true),
        [1_230, 0, 0],
    )
    .unwrap();
    state = state_of(&response);
    assert_eq!((state.recorded_collateral, state.accrued_fees), (1_200, 0));
    assert_eq!(response.sync_messages.len(), 1);
    assert_eq!(
        borsh::from_slice::<lez_compat_core::token::Message>(&response.sync_messages[0].message)
            .unwrap(),
        lez_compat_core::token::Message::Transfer {
            definition: id(42),
            recipient: id(70),
            amount: 13
        }
    );
    let repeated = operation(
        &input,
        &state,
        Message::CollectFees,
        config(100, id(70), true),
        [1_217, 0, 0],
    )
    .unwrap();
    assert!(repeated.sync_messages.is_empty());
    state.accrued_fees = 13;
    let response = operation(
        &input,
        &state,
        Message::CollectFees,
        config(100, input.receiver.account_id, true),
        [1_230, 0, 0],
    )
    .unwrap();
    assert!(response.sync_messages.is_empty());
    assert_eq!(
        (
            state_of(&response).recorded_collateral,
            state_of(&response).accrued_fees
        ),
        (1_200, 0)
    );
}

#[test]
fn execute_authenticates_buyer_and_rejects_recursive_or_setup_forwarding() {
    let (mut input, state) = funded();
    input.pre_state = vec![].into();
    input.receiver.account_id = id(50);
    input.from = None;
    input.message = borsh::to_vec(&Message::Execute {
        pool: prepare().receiver.account_id,
        instruction: Box::new(exact_in(5)),
    })
    .unwrap();
    assert_eq!(handle(&input).err().unwrap(), lbp_core::Error::Unauthorized);
    input.is_authorized = true;
    let forwarded = handle(&input).unwrap();
    assert_eq!(forwarded.sync_messages.len(), 1);
    assert_eq!(forwarded.sync_messages[0].to, prepare().receiver);
    assert_eq!(
        borsh::from_slice::<Message>(&forwarded.sync_messages[0].message).unwrap(),
        exact_in(5)
    );
    for message in [
        Message::ActivateFixedPrice,
        Message::ReadState,
        Message::CollectFees,
        Message::Execute {
            pool: id(99),
            instruction: Box::new(exact_in(5)),
        },
    ] {
        input.message = borsh::to_vec(&Message::Execute {
            pool: prepare().receiver.account_id,
            instruction: Box::new(message),
        })
        .unwrap();
        assert_eq!(handle(&input).err().unwrap(), lbp_core::Error::Unauthorized);
    }
    let (mut input, _) = funded();
    for from in [None, Some(Actor::new(id(50), id(99))), Some(input.receiver)] {
        input.from = from;
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.message = borsh::to_vec(&exact_in(5)).unwrap();
        assert_eq!(handle(&input).err().unwrap(), lbp_core::Error::Unauthorized);
    }
}

#[test]
fn trade_callbacks_authenticate_program_account_owner_definition_and_pool() {
    let (mut input, state) = funded();
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&exact_in(5)).unwrap();
    let mut response = handle(&input).unwrap();
    for (step, bytes) in [
        config(100, id(43), false),
        holding(input.receiver.account_id, id(42), 0),
        holding(input.receiver.account_id, id(41), 800),
        holding(id(50), id(42), 100),
    ]
    .into_iter()
    .enumerate()
    {
        let source = response.sync_messages[0].to;
        input = reply(&input, &response, source, bytes);
        for from in [
            None,
            Some(Actor::new(id(99), source.program_account_id)),
            Some(Actor::new(source.account_id, id(99))),
        ] {
            let mut forged = input.clone();
            forged.from = from;
            assert_eq!(
                handle(&forged).err().unwrap(),
                lbp_core::Error::Unauthorized
            );
        }
        let mut wrong_pool = input.clone();
        wrong_pool.receiver.account_id = id(99);
        assert_eq!(
            handle(&wrong_pool).err().unwrap(),
            lbp_core::Error::WrongAccount
        );
        let invalid = if step == 0 {
            let mut c = curve_core::Config::decode(&namespace_state()).unwrap();
            c.namespace = id(99);
            borsh::to_vec(&c).unwrap()
        } else {
            holding(id(99), id(99), 100)
        };
        assert!(handle(&reply(&input, &response, source, invalid)).is_err());
        let mut malformed = input.clone();
        malformed.message = vec![255];
        assert!(handle(&malformed).is_err());
        response = handle(&input).unwrap();
    }
}

#[test]
fn exact_output_rejects_overflow_in_price_or_fee_gross_up() {
    let (input, mut state) = funded();
    state.price = FixedPrice {
        collateral_units: u128::MAX,
        token_units: 1,
    };
    for (amount_out, rate) in [(2, 0), (1, 1)] {
        assert_eq!(
            operation(
                &input,
                &state,
                Message::BuyExactOutput {
                    membership: None,
                    amount_out,
                    max_amount_in: u128::MAX,
                    now: 100,
                },
                config(rate, id(43), false),
                [0, 800, u128::MAX],
            )
            .err(),
            Some(lbp_core::Error::Arithmetic)
        );
    }
}

#[test]
fn arithmetic_and_pending_funding_fail_closed() {
    let (input, mut state) = funded();
    state.price.token_units = u128::MAX;
    assert_eq!(
        operation(
            &input,
            &state,
            exact_in(u128::MAX),
            config(100, id(43), false),
            [0, 800, u128::MAX]
        )
        .err()
        .unwrap(),
        lbp_core::Error::Arithmetic
    );
    state.price.token_units = 2;
    state.recorded_collateral = u128::MAX;
    state.accrued_fees = 1;
    assert_eq!(
        operation(
            &input,
            &state,
            Message::CollectFees,
            config(100, id(43), false),
            [u128::MAX, 800, 100]
        )
        .err()
        .unwrap(),
        lbp_core::Error::Arithmetic
    );
    state.recorded_collateral = 0;
    state.accrued_fees = 0;
    state.funded = false;
    for message in [exact_in(5), Message::CollectFees] {
        assert_eq!(
            operation(
                &input,
                &state,
                message,
                config(100, id(43), false),
                [0, 800, 100]
            )
            .err()
            .unwrap(),
            lbp_core::Error::PendingFunding
        );
    }
}

#[test]
fn fixed_final_sellout_buy_keeps_pre_trade_inclusion_bounds_for_both_forms() {
    let (input, mut state) = funded();
    state.remaining_inventory = 2;
    for end in [None, Some(200)] {
        state.end_timestamp = end;
        state.ending_policy = if end.is_some() {
            lbp_core::EndingPolicy::Either
        } else {
            lbp_core::EndingPolicy::Depletion
        };
        for message in [
            Message::BuyExactInput {
                membership: None,
                amount_in: 4,
                min_amount_out: 2,
                now: 199,
            },
            Message::BuyExactOutput {
                membership: None,
                amount_out: 2,
                max_amount_in: 4,
                now: 199,
            },
        ] {
            let response = operation(
                &input,
                &state,
                message,
                config(100, id(43), false),
                [17, 9, 4],
            )
            .unwrap();
            let next = state_of(&response);
            assert_eq!(
                (
                    next.remaining_inventory,
                    next.recorded_collateral,
                    next.accrued_fees
                ),
                (0, 3, 1)
            );
            assert_eq!(next.ending_policy, state.ending_policy);
            assert_eq!(response.validity.timestamps.start(), Some(100_000));
            assert_eq!(
                response.validity.timestamps.end(),
                end.map(|value| value * 1000)
            );
            assert_eq!(next.close_time_bound().unwrap(), None);
        }
    }
}

#[test]
fn fixed_exact_time_edges_apply_to_both_buy_forms() {
    let (input, state) = funded();
    for now in [99, 100, 199, 200] {
        for message in [
            Message::BuyExactInput {
                membership: None,
                amount_in: 5,
                min_amount_out: 0,
                now,
            },
            Message::BuyExactOutput {
                membership: None,
                amount_out: 3,
                max_amount_in: 6,
                now,
            },
        ] {
            let result = operation(
                &input,
                &state,
                message,
                config(100, id(43), false),
                [0, 800, 100],
            );
            if (100..200).contains(&now) {
                let response = result.unwrap();
                assert_eq!(response.validity.timestamps.start(), Some(100_000));
                assert_eq!(response.validity.timestamps.end(), Some(200_000));
            } else {
                assert_eq!(result.err(), Some(lbp_core::Error::Closed));
            }
        }
    }
}
