use lbp_core::{CreatorAllocationRelease, Message, WeightedPoolInputs, compute_pool_pda, handle};
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, ReceiveInput},
};
use weighted_math_core::{calc_in_given_out, calc_out_given_in, interpolate_weight};

fn weight(percent: u128) -> u128 {
    percent * lbp_core::WEIGHT_TOTAL / 100
}

fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}

fn weighted_inputs() -> WeightedPoolInputs {
    WeightedPoolInputs {
        namespace: id(40),
        namespace_program_id: id(6),
        factory_program_id: id(7),
        factory_seed: [3; 32],
        token_definition_id: id(41),
        collateral_definition_id: id(42),
        sale_inventory: 1_000_000,
        collateral_seed: 100,
        creator_allocation: 0,
        token_weight_start: weight(95),
        token_weight_end: weight(50),
        start_timestamp: 1000,
        end_timestamp: 3000,
        creator_allocation_release: CreatorAllocationRelease::AfterSetup,
    }
}

fn namespace_state() -> Vec<u8> {
    let input = ReceiveInput {
        receiver: Actor::new(curve_core::compute_config_pda(id(40), id(6)), id(6)),
        from: Some(Actor::new(id(40), id(6))),
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&curve_core::Message::InitializeNamespace {
            namespace: id(40),
            protocol_fee_bps: 100,
            treasury: id(43),
        })
        .unwrap(),
    };
    curve_core::handle(&input)
        .unwrap()
        .post_state
        .unwrap()
        .to_vec()
}

fn config(fee: u16, treasury: AccountId, frozen: bool) -> Vec<u8> {
    let mut config = curve_core::Config::decode(&namespace_state()).unwrap();
    config.protocol_fee_bps = fee;
    config.treasury = treasury;
    config.emergency.is_frozen = frozen;
    borsh::to_vec(&config).unwrap()
}

fn reply(
    input: &ReceiveInput,
    response: &lee_core::program::Response,
    from: Actor,
    bytes: Vec<u8>,
) -> ReceiveInput {
    ReceiveInput {
        receiver: input.receiver,
        from: Some(from),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&lee_core::program::StateReply {
            state: bytes.into(),
        })
        .unwrap(),
    }
}

fn definition() -> Vec<u8> {
    borsh::to_vec(&lez_compat_core::token::TokenState::Definition {
        name: "Token".into(),
        kind: lez_compat_core::token::TokenKind::Fungible,
        supply: 2_000_000,
        metadata: None,
    })
    .unwrap()
}

fn holding(owner: AccountId, definition: AccountId, balance: u128) -> Vec<u8> {
    borsh::to_vec(&lez_compat_core::token::TokenState::Holding {
        owner,
        definition,
        kind: lez_compat_core::token::TokenKind::Fungible,
        balance,
    })
    .unwrap()
}

fn prepared() -> (ReceiveInput, lee_core::program::Response) {
    let inputs = weighted_inputs();
    let factory = AccountId::for_public_pda(&id(7), &PdaSeed::new([3; 32]));
    let mut input = ReceiveInput {
        receiver: Actor::new(
            compute_pool_pda(id(40), id(8), id(41), id(42), factory),
            id(8),
        ),
        from: Some(Actor::new(factory, id(7))),
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&Message::PrepareWeighted {
            inputs: inputs.clone(),
        })
        .unwrap(),
    };
    let mut response = handle(&input).unwrap();
    for bytes in [
        namespace_state(),
        definition(),
        definition(),
        holding(factory, id(41), inputs.sale_inventory),
        holding(input.receiver.account_id, id(41), 0),
        holding(input.receiver.account_id, id(42), 0),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle(&input).unwrap();
    }
    (input, response)
}

/// Drive preparation + activation to a ready-to-trade state.
fn funded() -> (ReceiveInput, lbp_core::PoolAccount) {
    let inputs = weighted_inputs();
    let factory = AccountId::for_public_pda(&id(7), &PdaSeed::new([3; 32]));
    let (mut input, prepared) = prepared();
    input.pre_state = prepared.post_state.unwrap();
    input.from = Some(Actor::new(factory, id(7)));
    input.message = borsh::to_vec(&Message::ActivateWeighted).unwrap();
    let mut response = handle(&input).unwrap();
    for bytes in [
        namespace_state(),
        definition(),
        definition(),
        holding(factory, id(41), 0),
        holding(input.receiver.account_id, id(41), inputs.sale_inventory),
        holding(input.receiver.account_id, id(42), inputs.collateral_seed),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle(&input).unwrap();
    }
    let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    input.from = Some(Actor::new(id(50), id(8)));
    (input, state)
}

/// Drive a buy or collect-fees message through all async steps.
fn operation(
    input: &ReceiveInput,
    state: &lbp_core::PoolAccount,
    message: Message,
    config: Vec<u8>,
    // [pool_collateral_vault, pool_token_vault, buyer_collateral]
    balances: [u128; 3],
) -> Result<lee_core::program::Response, lbp_core::Error> {
    let collect = matches!(message, Message::CollectFees);
    let mut input = input.clone();
    input.pre_state = borsh::to_vec(state).unwrap().into();
    input.message = borsh::to_vec(&message).unwrap();
    let mut response = handle(&input)?;
    let values = [
        config,
        holding(input.receiver.account_id, id(42), balances[0]),
        holding(input.receiver.account_id, id(41), balances[1]),
        holding(id(50), id(42), balances[2]),
    ];
    for bytes in values.into_iter().take(if collect { 2 } else { 4 }) {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle(&input)?;
    }
    Ok(response)
}

fn state_of(response: &lee_core::program::Response) -> lbp_core::PoolAccount {
    lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap()
}

#[test]
fn weighted_buy_asserts_the_full_validity_window_on_the_canonical_clock() {
    let (input, state) = funded();
    let response = operation(
        &input,
        &state,
        Message::BuyWeightedExactInput {
            amount_in: 10,
            min_amount_out: 1,
            valid_from: 1500,
            valid_until: 1600,
        },
        config(0, id(43), false),
        [100, state.remaining_inventory, 1000],
    )
    .unwrap();
    let clock = &response.sync_messages[0];
    assert_eq!(
        clock.to,
        Actor::new(
            clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
            clock_core::clock_account_id()
        )
    );
    let clock_core::Message::AssertTimestamp { at_least, at_most } =
        borsh::from_slice(&clock.message).unwrap()
    else {
        panic!("expected a clock assertion");
    };
    assert_eq!((at_least, at_most), (1_500_000, 1_599_999));
    for timestamp in [1_500_000, 1_550_000, 1_599_999] {
        assert!(at_least <= timestamp && timestamp <= at_most);
    }
    for timestamp in [1_499_999, 1_600_000, 2_000_000] {
        assert!(!(at_least <= timestamp && timestamp <= at_most));
    }
}

#[test]
fn weighted_windows_use_the_least_favourable_pair_for_both_buy_forms() {
    let (input, state) = funded();
    for (start, end, worst_collateral_weight) in [
        (95, 50, weight(16) + lbp_core::WEIGHT_TOTAL / 400),
        (50, 95, weight(27) + lbp_core::WEIGHT_TOTAL / 200),
        (50, 50, weight(50)),
    ] {
        let mut state = state.clone();
        if let lbp_core::PoolMode::Weighted {
            token_weight_start,
            token_weight_end,
            ..
        } = &mut state.mode
        {
            *token_weight_start = weight(start);
            *token_weight_end = weight(end);
        }
        let expected_out = calc_out_given_in(
            100,
            worst_collateral_weight,
            1_000_000,
            lbp_core::WEIGHT_TOTAL - worst_collateral_weight,
            50,
        );
        let expected_in = calc_in_given_out(
            100,
            worst_collateral_weight,
            1_000_000,
            lbp_core::WEIGHT_TOTAL - worst_collateral_weight,
            100_000,
        );
        let buy = |message| {
            operation(
                &input,
                &state,
                message,
                config(0, id(43), false),
                [100, 1_000_000, 10_000],
            )
        };
        let exact_in = |limit| Message::BuyWeightedExactInput {
            amount_in: 50,
            min_amount_out: limit,
            valid_from: 1500,
            valid_until: 2001,
        };
        let next = state_of(&buy(exact_in(expected_out)).unwrap());
        assert_eq!(next.remaining_inventory, 1_000_000 - expected_out);
        assert_eq!(
            buy(exact_in(expected_out + 1)).err(),
            Some(lbp_core::Error::Slippage)
        );
        let exact_out = |limit| Message::BuyWeightedExactOutput {
            amount_out: 100_000,
            max_amount_in: limit,
            valid_from: 1500,
            valid_until: 2001,
        };
        let next = state_of(&buy(exact_out(expected_in)).unwrap());
        assert_eq!(pricing_collateral(&next), 100 + expected_in);
        assert_eq!(
            buy(exact_out(expected_in - 1)).err(),
            Some(lbp_core::Error::Slippage)
        );
        for time in [1500, 1600, 1750, 1999, 2000] {
            let c = interpolate_weight(weight(100 - start), weight(100 - end), 1000, 3000, time);
            assert!(
                expected_out
                    <= calc_out_given_in(100, c, 1_000_000, lbp_core::WEIGHT_TOTAL - c, 50)
            );
            assert!(
                expected_in
                    >= calc_in_given_out(100, c, 1_000_000, lbp_core::WEIGHT_TOTAL - c, 100_000)
            );
        }
    }
}

#[test]
fn wider_windows_cannot_improve_the_buyers_scheduled_price() {
    let (input, state) = funded();
    for (start, end) in [(95, 50), (50, 95), (50, 50)] {
        let mut state = state.clone();
        if let lbp_core::PoolMode::Weighted {
            token_weight_start,
            token_weight_end,
            ..
        } = &mut state.mode
        {
            *token_weight_start = weight(start);
            *token_weight_end = weight(end);
        }
        let quote = |valid_from, valid_until, exact_in| {
            let message = if exact_in {
                Message::BuyWeightedExactInput {
                    amount_in: 50,
                    min_amount_out: 1,
                    valid_from,
                    valid_until,
                }
            } else {
                Message::BuyWeightedExactOutput {
                    amount_out: 100_000,
                    max_amount_in: 10_000,
                    valid_from,
                    valid_until,
                }
            };
            state_of(
                &operation(
                    &input,
                    &state,
                    message,
                    config(0, id(43), false),
                    [100, 1_000_000, 10_000],
                )
                .unwrap(),
            )
        };
        assert!(
            quote(1000, 3000, true).remaining_inventory
                >= quote(1500, 2001, true).remaining_inventory
        );
        assert!(
            pricing_collateral(&quote(1000, 3000, false))
                >= pricing_collateral(&quote(1500, 2001, false))
        );
    }
}

#[test]
fn weighted_windows_reject_invalid_bounds_and_time_conversion_overflow() {
    let (input, state) = funded();
    for exact_in in [true, false] {
        for (valid_from, valid_until, error) in [
            (1500, 1500, lbp_core::Error::InvalidTerms),
            (1600, 1500, lbp_core::Error::InvalidTerms),
            (999, 1500, lbp_core::Error::Closed),
            (1500, 3001, lbp_core::Error::Closed),
        ] {
            let message = if exact_in {
                Message::BuyWeightedExactInput {
                    amount_in: 50,
                    min_amount_out: 1,
                    valid_from,
                    valid_until,
                }
            } else {
                Message::BuyWeightedExactOutput {
                    amount_out: 500,
                    max_amount_in: 1000,
                    valid_from,
                    valid_until,
                }
            };
            assert_eq!(
                operation(
                    &input,
                    &state,
                    message,
                    config(0, id(43), false),
                    [100, 1_000_000, 1000]
                )
                .err(),
                Some(error)
            );
        }
    }
    let mut state = state;
    state.end_timestamp = Some(u64::MAX);
    assert_eq!(
        operation(
            &input,
            &state,
            Message::BuyWeightedExactInput {
                amount_in: 50,
                min_amount_out: 1,
                valid_from: 1500,
                valid_until: u64::MAX,
            },
            config(0, id(43), false),
            [100, 1_000_000, 1000]
        )
        .err(),
        Some(lbp_core::Error::Arithmetic)
    );
}

#[test]
fn weighted_buys_cannot_bypass_windows_with_fixed_price_messages() {
    let (input, state) = funded();
    for message in [
        Message::BuyExactInput {
            amount_in: 50,
            min_amount_out: 1,
            now: 1500,
        },
        Message::BuyExactOutput {
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
fn weighted_window_excludes_the_deadline_from_weight_selection() {
    let (input, mut state) = funded();
    state.end_timestamp = Some(1003);
    if let lbp_core::PoolMode::Weighted {
        token_weight_start,
        token_weight_end,
        ..
    } = &mut state.mode
    {
        *token_weight_start = weight(3);
        *token_weight_end = weight(7);
    }
    // Collateral weights at seconds 1000, 1001 and 1002 are 97, 95 and 94.
    // The window excludes 1002, so select 95/5, not 94/6.
    let collateral = weight(97) - weight(4).div_ceil(3);
    let expected = calc_out_given_in(
        100,
        collateral,
        1_000_000,
        lbp_core::WEIGHT_TOTAL - collateral,
        10,
    );
    let response = operation(
        &input,
        &state,
        Message::BuyWeightedExactInput {
            amount_in: 10,
            min_amount_out: expected,
            valid_from: 1000,
            valid_until: 1002,
        },
        config(0, id(43), false),
        [100, 1_000_000, 1000],
    )
    .unwrap();
    assert_eq!(
        state_of(&response).remaining_inventory,
        1_000_000 - expected
    );
}

#[test]
fn weighted_pricing_ignores_unsolicited_vault_transfers() {
    let (input, state) = funded();
    for message in [
        Message::BuyWeightedExactInput {
            amount_in: 10,
            min_amount_out: 1,
            valid_from: 1500,
            valid_until: 1500 + 1,
        },
        Message::BuyWeightedExactOutput {
            amount_out: 500,
            max_amount_in: 1000,
            valid_from: 1500,
            valid_until: 1500 + 1,
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

fn pricing_collateral(state: &lbp_core::PoolAccount) -> u128 {
    let lbp_core::PoolMode::Weighted {
        pricing_collateral, ..
    } = state.mode
    else {
        panic!("expected weighted state");
    };
    pricing_collateral
}

#[test]
fn weighted_program_rejects_unsafe_and_uncertified_quotes_with_defined_errors() {
    let (input, original) = funded();
    let exact_in = Message::BuyWeightedExactInput {
        amount_in: 50,
        min_amount_out: 1,
        valid_from: 1500,
        valid_until: 1600,
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
                amount_out: 300_000,
                max_amount_in: u128::MAX,
                valid_from: 1500,
                valid_until: 1600,
            },
            [100, 1_000_000, u128::MAX]
        ),
        Some(lbp_core::Error::MathAccuracy)
    );
    assert_eq!(
        run(
            &state,
            Message::BuyWeightedExactOutput {
                amount_out: 300_001,
                max_amount_in: u128::MAX,
                valid_from: 1500,
                valid_until: 1600,
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
    assert_eq!(
        run(&state, exact_in.clone(), [100, 1_000_000, 1000]),
        Some(lbp_core::Error::Arithmetic)
    );

    let b = 1u128 << 100;
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
                amount_out: b / 10 * 3,
                max_amount_in: u128::MAX,
                valid_from: 1500,
                valid_until: 1600,
            },
            [100, b, u128::MAX]
        ),
        Some(lbp_core::Error::Arithmetic)
    );
    assert_eq!(
        run(
            &state,
            Message::BuyWeightedExactInput {
                amount_in: 1,
                min_amount_out: 0,
                valid_from: 1500,
                valid_until: 1600,
            },
            [100, b, 1000]
        ),
        Some(lbp_core::Error::InvalidAmount)
    );
}

#[test]
fn weighted_program_checks_full_inventory_and_post_buy_vault_capacity() {
    let (input, mut state) = funded();
    let buy = Message::BuyWeightedExactInput {
        amount_in: 50,
        min_amount_out: 1,
        valid_from: 1500,
        valid_until: 1600,
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
fn weighted_pricing_is_independent_of_live_creator_borne_fees() {
    let (input, state) = funded();
    for message in [
        Message::BuyWeightedExactInput {
            amount_in: 50,
            min_amount_out: 1,
            valid_from: 1500,
            valid_until: 1500 + 1,
        },
        Message::BuyWeightedExactOutput {
            amount_out: 500,
            max_amount_in: 1000,
            valid_from: 1500,
            valid_until: 1500 + 1,
        },
    ] {
        let mut reference = None;
        for fee in [0, 100, 500, 9999] {
            let response = operation(
                &input,
                &state,
                message.clone(),
                config(fee, id(43), false),
                [100, state.remaining_inventory, 1000],
            )
            .unwrap();
            let next = state_of(&response);
            let quote = (next.remaining_inventory, pricing_collateral(&next));
            if let Some(expected) = reference {
                assert_eq!(quote, expected);
            }
            reference = Some(quote);
            assert_eq!(
                next.recorded_collateral + next.accrued_fees,
                pricing_collateral(&next)
            );
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
        amount_in: 50,
        min_amount_out: 1,
        valid_from: 1500,
        valid_until: 1500 + 1,
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
    let ideal_first = state.remaining_inventory * 50 / 150;
    assert!(first_out <= ideal_first && ideal_first - first_out <= 1);
    assert_eq!(pricing_collateral(&after_first), 150);
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
    assert_eq!(pricing_collateral(&after_collection), 150);
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
    assert_eq!(pricing_collateral(&next), 200);
    let second_out = after_first.remaining_inventory - next.remaining_inventory;
    let ideal_second = after_first.remaining_inventory * 50 / 200;
    assert!(second_out <= ideal_second && ideal_second - second_out <= 1);
    assert!(second_out < first_out);
}

// ---------------------------------------------------------------------------
// Buy math correctness
// ---------------------------------------------------------------------------

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
        amount_in,
    );
    assert!(expected_out > 0, "expected non-zero output");

    let response = operation(
        &input,
        &state,
        Message::BuyWeightedExactInput {
            amount_in,
            min_amount_out: 0,
            valid_from: inputs.start_timestamp,
            valid_until: inputs.start_timestamp + 1,
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
    // The creator pays fees from the required input. The buyer pays no extra.
    let gross_in = amount_in;

    let response = operation(
        &input,
        &state,
        Message::BuyWeightedExactOutput {
            amount_out,
            max_amount_in: gross_in,
            valid_from: inputs.start_timestamp,
            valid_until: inputs.start_timestamp + 1,
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

    let response = operation(
        &input,
        &state,
        Message::BuyWeightedExactInput {
            amount_in,
            min_amount_out: 0,
            valid_from: inputs.start_timestamp,
            valid_until: inputs.start_timestamp + 1,
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

// ---------------------------------------------------------------------------
// Guards
// ---------------------------------------------------------------------------

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
            operation(
                &input,
                &state,
                Message::BuyWeightedExactInput {
                    amount_in: 50,
                    min_amount_out: 0,
                    valid_from: now,
                    valid_until: now + 1,
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
fn weighted_buy_rejects_slippage_exact_in() {
    let (input, state) = funded();
    let inputs = weighted_inputs();
    // set min_amount_out to u128::MAX to guarantee slippage rejection
    assert_eq!(
        operation(
            &input,
            &state,
            Message::BuyWeightedExactInput {
                amount_in: 50,
                min_amount_out: u128::MAX,
                valid_from: inputs.start_timestamp,
                valid_until: inputs.start_timestamp + 1,
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
        operation(
            &input,
            &state,
            Message::BuyWeightedExactOutput {
                amount_out: 500,
                max_amount_in: 0,
                valid_from: inputs.start_timestamp,
                valid_until: inputs.start_timestamp + 1,
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
        operation(
            &input,
            &state,
            Message::BuyWeightedExactInput {
                amount_in: 50,
                min_amount_out: 0,
                valid_from: inputs.start_timestamp,
                valid_until: inputs.start_timestamp + 1,
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
        operation(
            &input,
            &state,
            Message::BuyWeightedExactInput {
                amount_in: 50,
                min_amount_out: 0,
                valid_from: inputs.start_timestamp,
                valid_until: inputs.start_timestamp + 1,
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
        operation(
            &input,
            &state,
            Message::BuyWeightedExactInput {
                amount_in: 0,
                min_amount_out: 0,
                valid_from: inputs.start_timestamp,
                valid_until: inputs.start_timestamp + 1,
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
        operation(
            &input,
            &state,
            Message::BuyWeightedExactInput {
                amount_in: 50,
                min_amount_out: 0,
                valid_from: inputs.start_timestamp,
                valid_until: inputs.start_timestamp + 1,
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
        operation(
            &input,
            &state,
            Message::BuyWeightedExactInput {
                amount_in: 50,
                min_amount_out: 0,
                valid_from: inputs.start_timestamp,
                valid_until: inputs.start_timestamp + 1,
            },
            config(100, id(43), false),
            [inputs.collateral_seed, 0, 1_000],
        )
        .err()
        .unwrap(),
        lbp_core::Error::Closed
    );
}

// ---------------------------------------------------------------------------
// Weight shift over time
// ---------------------------------------------------------------------------

#[test]
fn weighted_buy_prices_fall_as_weights_shift() {
    // At t=start: token_weight=95, cheap tokens (high price in collateral per token)
    // At t=end:   token_weight=50, more tokens out (lower price per token)
    // For the same amount_in, amount_out should be LARGER at end than at start.
    let (input, state) = funded();
    let inputs = weighted_inputs();

    let buy = |now: u64| -> u128 {
        let response = operation(
            &input,
            &state,
            Message::BuyWeightedExactInput {
                amount_in: 50,
                min_amount_out: 0,
                valid_from: now,
                valid_until: now + 1,
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

// ---------------------------------------------------------------------------
// Borsh tag ordering
// ---------------------------------------------------------------------------

#[test]
fn weighted_message_tags_follow_fixed_price_variants() {
    // Fixed-price tags 0-6 must be unchanged; weighted variants come after.
    use lbp_core::Message;
    let factory = AccountId::for_public_pda(&id(7), &PdaSeed::new([3; 32]));
    let pool = compute_pool_pda(id(40), id(8), id(41), id(42), factory);
    for (msg, expected_tag) in [
        (
            Message::PrepareWeighted {
                inputs: weighted_inputs(),
            },
            7u8,
        ),
        (Message::ActivateWeighted, 8),
    ] {
        assert_eq!(
            borsh::to_vec(&msg).unwrap()[0],
            expected_tag,
            "wrong tag for {msg:?}"
        );
        let _ = pool;
    }
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
    let quote = |second| {
        operation(
            &input,
            &state,
            Message::BuyWeightedExactInput {
                amount_in: 10,
                min_amount_out: 1,
                valid_from: second,
                valid_until: second + 1,
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
