#![cfg(local_mock_clock)]
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

/// Drive a buy or collect-fees message through all synchronous reply steps.
fn operation_at(
    timestamp_ms: u64,
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
    if !collect {
        let source = response.sync_messages.last().unwrap().to;
        assert_eq!(source, lbp_core::pricing_clock::actor());
        input = ReceiveInput {
            receiver: input.receiver,
            from: Some(source),
            is_authorized: false,
            pre_state: response.post_state.unwrap(),
            message: borsh::to_vec(&lbp_core::pricing_clock::Reply { timestamp_ms }).unwrap(),
        };
        response = handle(&input)?;
    }
    Ok(response)
}
fn operation(
    input: &ReceiveInput,
    state: &lbp_core::PoolAccount,
    message: Message,
    config: Vec<u8>,
    balances: [u128; 3],
) -> Result<lee_core::program::Response, lbp_core::Error> {
    operation_at(1_500_000, input, state, message, config, balances)
}

fn state_of(response: &lee_core::program::Response) -> lbp_core::PoolAccount {
    lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap()
}

#[test]
fn weighted_buys_cannot_bypass_clock_reads_with_fixed_price_messages() {
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
fn weighted_pricing_ignores_unsolicited_vault_transfers() {
    let (input, state) = funded();
    for message in [
        Message::BuyWeightedExactInput {
            amount_in: 10,
            min_amount_out: 1,
        },
        Message::BuyWeightedExactOutput {
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
        },
        Message::BuyWeightedExactOutput {
            amount_out: 500,
            max_amount_in: 1000,
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

    let response = operation_at(
        inputs.start_timestamp * 1000,
        &input,
        &state,
        Message::BuyWeightedExactInput {
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
    // The creator pays fees from the required input. The buyer pays no extra.
    let gross_in = amount_in;

    let response = operation_at(
        inputs.start_timestamp * 1000,
        &input,
        &state,
        Message::BuyWeightedExactOutput {
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
            operation_at(
                now * 1000,
                &input,
                &state,
                Message::BuyWeightedExactInput {
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
        let response = operation_at(
            now * 1000,
            &input,
            &state,
            Message::BuyWeightedExactInput {
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

// ---------------------------------------------------------------------------
// Borsh tag ordering
// ---------------------------------------------------------------------------

#[test]
fn weighted_message_tags_follow_fixed_price_variants() {
    // Fixed-price tags 0-7 must be unchanged; weighted variants come after.
    use lbp_core::Message;
    let factory = AccountId::for_public_pda(&id(7), &PdaSeed::new([3; 32]));
    let pool = compute_pool_pda(id(40), id(8), id(41), id(42), factory);
    for (msg, expected_tag) in [
        (
            Message::PrepareWeighted {
                inputs: weighted_inputs(),
            },
            8u8,
        ),
        (Message::ActivateWeighted, 9),
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
    let quote = |second: u64| {
        operation_at(
            second * 1000,
            &input,
            &state,
            Message::BuyWeightedExactInput {
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
fn fixed_price_settlement_cannot_settle_a_weighted_pool() {
    let (mut input, state) = funded();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SettleFixedPrice {
        creator: id(50),
        now: state.end_timestamp.unwrap(),
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
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
                        amount_in: 50,
                        min_amount_out: out,
                    },
                    out,
                    50,
                ),
                (
                    Message::BuyWeightedExactOutput {
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
        message: borsh::to_vec(&lbp_core::pricing_clock::Reply {
            timestamp_ms: 1_500_000,
        })
        .unwrap(),
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
    input.message = borsh::to_vec(&lbp_core::pricing_clock::Reply {
        timestamp_ms: 1_500_000,
    })
    .unwrap();
    for actor in [
        Actor::new(id(1), lbp_core::pricing_clock::PROGRAM),
        Actor::new(lbp_core::pricing_clock::ACCOUNT, id(1)),
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
                amount_in: 10,
                min_amount_out: 1
            },
            config(0, id(43), false),
            [100, 1_000_000, 1000]
        )
        .err(),
        Some(lbp_core::Error::Arithmetic)
    );
}
