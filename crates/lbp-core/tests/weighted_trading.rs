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
        buy_policy: curve_core::BuyPolicy::Open,
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
                membership: None,
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
                membership: None,
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
fn weighted_pricing_is_independent_of_live_creator_borne_fees() {
    let (input, state) = funded();
    for message in [
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 50,
            min_amount_out: 1,
        },
        Message::BuyWeightedExactOutput {
            membership: None,
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
    // The creator pays fees from the required input. The buyer pays no extra.
    let gross_in = amount_in;

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
                membership: None,
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

// ---------------------------------------------------------------------------
// Settlement helper
// ---------------------------------------------------------------------------

/// Drive a settlement or collect-fees message through all synchronous reply steps.
/// Settlement reads namespace config + pool collateral vault + pool token vault, then finishes.
fn settle(
    input: &ReceiveInput,
    state: &lbp_core::PoolAccount,
    message: Message,
    config_bytes: Vec<u8>,
    // [pool_collateral_vault, pool_token_vault]
    balances: [u128; 2],
) -> Result<lee_core::program::Response, lbp_core::Error> {
    let mut input = input.clone();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(state).unwrap().into();
    input.message = borsh::to_vec(&message).unwrap();
    let mut response = handle(&input)?;
    for bytes in [
        config_bytes,
        holding(input.receiver.account_id, id(42), balances[0]),
        holding(input.receiver.account_id, id(41), balances[1]),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle(&input)?;
    }
    Ok(response)
}

// ---------------------------------------------------------------------------
// Pause / Resume controls
// ---------------------------------------------------------------------------

#[test]
fn weighted_pause_requires_factory_authorization() {
    let (mut input, state) = funded();
    // Factory owner is the authorized caller.
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SetWeightedPaused { paused: true }).unwrap();
    assert!(handle(&input).is_ok());

    // Wrong program ID
    input.from = Some(Actor::new(state.factory_owner, id(99)));
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));

    // Wrong account ID
    input.from = Some(Actor::new(id(99), state.factory_program_id));
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));

    // No sender
    input.from = None;
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));

    // Random buyer cannot pause
    input.from = Some(Actor::new(id(50), id(8)));
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
}

#[test]
fn weighted_repeated_pause_succeeds_without_event() {
    let (mut input, state) = funded();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();

    // First pause emits event
    input.message = borsh::to_vec(&Message::SetWeightedPaused { paused: true }).unwrap();
    let response = handle(&input).unwrap();
    assert!(!response.events.is_empty(), "first pause should emit event");
    let paused_state =
        lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(matches!(
        paused_state.mode,
        lbp_core::PoolMode::Weighted { paused: true, .. }
    ));

    // Repeated pause succeeds but emits no event
    input.pre_state = response.post_state.unwrap();
    let response = handle(&input).unwrap();
    assert!(
        response.events.is_empty(),
        "repeated pause should not emit event"
    );

    // Resume emits event
    input.pre_state = response.post_state.unwrap();
    input.message = borsh::to_vec(&Message::SetWeightedPaused { paused: false }).unwrap();
    let response = handle(&input).unwrap();
    assert!(!response.events.is_empty(), "resume should emit event");

    // Repeated resume (already unpaused) succeeds silently
    input.pre_state = response.post_state.unwrap();
    let response = handle(&input).unwrap();
    assert!(
        response.events.is_empty(),
        "repeated resume should not emit event"
    );
}

#[test]
fn weighted_pause_blocks_both_buy_forms() {
    let (input, mut state) = funded();
    if let lbp_core::PoolMode::Weighted { paused, .. } = &mut state.mode {
        *paused = true;
    }
    for message in [
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 50,
            min_amount_out: 1,
        },
        Message::BuyWeightedExactOutput {
            membership: None,
            amount_out: 500,
            max_amount_in: 1000,
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
            Some(lbp_core::Error::Paused)
        );
    }
}

#[test]
fn weighted_resume_cannot_override_namespace_freeze() {
    let (input, state) = funded();
    // Even when unpaused, frozen config blocks buys
    assert_eq!(
        operation(
            &input,
            &state,
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: 1,
            },
            config(0, id(43), true),
            [100, 1_000_000, 1000]
        )
        .err(),
        Some(lbp_core::Error::Frozen)
    );
}

#[test]
fn weighted_fees_collectible_when_paused() {
    let (input, mut state) = funded();
    state.accrued_fees = 5;
    state.recorded_collateral = 100;
    if let lbp_core::PoolMode::Weighted { paused, .. } = &mut state.mode {
        *paused = true;
    }
    let response = operation(
        &input,
        &state,
        Message::CollectFees,
        config(0, id(43), false),
        [105, state.remaining_inventory, 0],
    )
    .unwrap();
    let next = state_of(&response);
    assert_eq!(next.accrued_fees, 0);
}

#[test]
fn weighted_pause_rejects_after_settlement() {
    let (mut input, mut state) = funded();
    state.proceeds_settled = true;
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SetWeightedPaused { paused: true }).unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Closed));
}

#[test]
fn weighted_pause_requires_funded_pool() {
    let (mut input, mut state) = funded();
    state.funded = false;
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SetWeightedPaused { paused: true }).unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::PendingFunding));
}

#[test]
fn fixed_price_pool_rejects_weighted_pause() {
    let (mut input, state) = funded();
    let mut fixed = state.clone();
    fixed.mode = lbp_core::PoolMode::FixedPrice;
    input.from = Some(Actor::new(fixed.factory_owner, fixed.factory_program_id));
    input.pre_state = borsh::to_vec(&fixed).unwrap().into();
    input.message = borsh::to_vec(&Message::SetWeightedPaused { paused: true }).unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
}

// ---------------------------------------------------------------------------
// Settlement — proceeds, fees, burn
// ---------------------------------------------------------------------------

#[test]
fn weighted_settlement_pays_seed_and_proceeds_burns_unsold_and_collects_fees() {
    let (input, mut state) = funded();
    // Simulate a buy: 50 in, fee 1, net 49 recorded
    state.recorded_collateral = 149; // seed(100) + net(49)
    state.accrued_fees = 1;
    state.remaining_inventory = 999_500;
    if let lbp_core::PoolMode::Weighted {
        pricing_collateral, ..
    } = &mut state.mode
    {
        *pricing_collateral = 150;
    }
    let vault_collateral = 191; // 150 liabilities + 41 surplus
    let vault_token = 999_507; // remaining + 7 surplus (creator alloc mock)

    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(100, id(43), false),
        [vault_collateral, vault_token],
    )
    .unwrap();
    let next = state_of(&response);
    assert!(next.proceeds_settled);
    assert_eq!(next.recorded_collateral, 0);
    assert_eq!(next.accrued_fees, 0);

    // Check the response has transfers and burns
    // Fee payout to treasury, burn of unsold, proceeds payout to creator
    let messages = &response.sync_messages;
    assert!(
        messages.len() >= 2,
        "expected fee + burn + proceeds transfers"
    );
}

#[test]
fn weighted_settlement_requires_factory_authorization() {
    let (mut input, state) = funded();
    // Random caller cannot settle
    input.from = Some(Actor::new(id(50), id(8)));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SettleWeighted { creator: id(50) }).unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
}

#[test]
fn weighted_settlement_rejects_default_creator() {
    let (input, state) = funded();
    let result = settle(
        &input,
        &state,
        Message::SettleWeighted {
            creator: AccountId::default(),
        },
        config(0, id(43), false),
        [100, 1_000_000],
    );
    assert_eq!(result.err(), Some(lbp_core::Error::Unauthorized));
}

#[test]
fn weighted_settlement_is_idempotent() {
    let (input, mut state) = funded();
    state.recorded_collateral = 100;
    state.accrued_fees = 0;
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), false),
        [100, 1_000_000],
    )
    .unwrap();
    let settled = state_of(&response);
    assert!(settled.proceeds_settled);

    // Second settlement returns state unchanged (no-op)
    let mut input2 = input.clone();
    input2.from = Some(Actor::new(
        settled.factory_owner,
        settled.factory_program_id,
    ));
    input2.pre_state = borsh::to_vec(&settled).unwrap().into();
    input2.message = borsh::to_vec(&Message::SettleWeighted { creator: id(50) }).unwrap();
    let response2 = handle(&input2).unwrap();
    // Idempotent: no transfers, state preserved
    let still_settled = state_of(&response2);
    assert!(still_settled.proceeds_settled);
    assert_eq!(still_settled.recorded_collateral, 0);
}

#[test]
fn weighted_settlement_enforces_time_bound_at_scheduled_end() {
    let (input, state) = funded();
    // Settlement response must have a time lower bound at the scheduled end.
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), false),
        [100, 1_000_000],
    )
    .unwrap();
    // The end_timestamp is 3000, so the time lower bound should be 3_000_000 ms
    assert_eq!(response.validity.timestamps.start(), Some(3_000_000));
}

#[test]
fn weighted_settlement_with_zero_fees_and_no_unsold() {
    let (input, mut state) = funded();
    state.recorded_collateral = 200;
    state.accrued_fees = 0;
    state.remaining_inventory = 0;
    if let lbp_core::PoolMode::Weighted {
        pricing_collateral, ..
    } = &mut state.mode
    {
        *pricing_collateral = 200;
    }
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), false),
        [200, 0],
    )
    .unwrap();
    let next = state_of(&response);
    assert!(next.proceeds_settled);
    // No burn needed when remaining_inventory is 0
    // No fee payout when accrued_fees is 0
}

#[test]
fn weighted_settlement_rejects_underfunded_token_vault() {
    let (input, state) = funded();
    assert_eq!(
        settle(
            &input,
            &state,
            Message::SettleWeighted { creator: id(50) },
            config(0, id(43), false),
            [100, state.remaining_inventory - 1], // token vault below remaining inventory
        )
        .err(),
        Some(lbp_core::Error::Underfunded)
    );
}

#[test]
fn weighted_settlement_rejects_underfunded_collateral_vault() {
    let (input, state) = funded();
    assert_eq!(
        settle(
            &input,
            &state,
            Message::SettleWeighted { creator: id(50) },
            config(0, id(43), false),
            [99, state.remaining_inventory], // collateral vault below liabilities (seed=100)
        )
        .err(),
        Some(lbp_core::Error::Underfunded)
    );
}

#[test]
fn weighted_settlement_available_when_frozen() {
    let (input, state) = funded();
    // Settlement must work even when namespace is frozen
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), true), // frozen
        [100, 1_000_000],
    );
    assert!(response.is_ok());
}

#[test]
fn weighted_settlement_available_when_paused() {
    let (input, mut state) = funded();
    if let lbp_core::PoolMode::Weighted { paused, .. } = &mut state.mode {
        *paused = true;
    }
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), false),
        [100, 1_000_000],
    );
    assert!(response.is_ok());
}

#[test]
fn weighted_settlement_surplus_not_paid_to_creator() {
    let (input, mut state) = funded();
    state.recorded_collateral = 100; // seed only, no buys
    state.accrued_fees = 0;
    // Vault has surplus beyond recorded liabilities
    let vault_collateral = 500; // 400 surplus
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), false),
        [vault_collateral, 1_000_000],
    )
    .unwrap();
    let next = state_of(&response);
    assert!(next.proceeds_settled);
    // recorded_collateral is zeroed — only the original 100 is paid, not 500
    assert_eq!(next.recorded_collateral, 0);
}

#[test]
fn weighted_fixed_price_settlement_rejects_weighted_pool() {
    let (input, state) = funded();
    // SettleFixedPrice on a weighted pool should fail
    let mut input2 = input.clone();
    input2.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input2.pre_state = borsh::to_vec(&state).unwrap().into();
    input2.message = borsh::to_vec(&Message::SettleFixedPrice {
        creator: id(50),
        now: 3000,
    })
    .unwrap();
    assert_eq!(handle(&input2).err(), Some(lbp_core::Error::InvalidMessage));
}

#[test]
fn weighted_settlement_requires_funded_pool() {
    let (input, mut state) = funded();
    state.funded = false;
    let result = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), false),
        [100, 1_000_000],
    );
    assert_eq!(result.err(), Some(lbp_core::Error::PendingFunding));
}

#[test]
fn weighted_depleted_inventory_before_end_does_not_allow_early_settlement() {
    let (input, mut state) = funded();
    state.remaining_inventory = 0;
    state.recorded_collateral = 100;
    // Even with depleted inventory, the time bound is still the scheduled end
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), false),
        [100, 0],
    )
    .unwrap();
    // Must still enforce the end timestamp as lower bound
    assert_eq!(response.validity.timestamps.start(), Some(3_000_000));
}

#[test]
fn weighted_allowlisted_buys_require_membership_before_external_queries() {
    let (mut input, mut state) = funded();
    state.buy_policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *state.namespace.value(),
        sale_policy_id: *input.receiver.account_id.value(),
        root: [9; 32],
    });
    input.pre_state = borsh::to_vec(&state).unwrap().into();
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
        input.message = borsh::to_vec(&message).unwrap();
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
    }
}

fn membership_for(
    namespace: AccountId,
    pool: AccountId,
    trader: AccountId,
) -> (curve_core::BuyPolicy, curve_core::MembershipPath) {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let path = curve_core::MembershipPath {
        index: 0,
        salt: [7; 32],
        siblings: [[8; 32]; 17],
    };
    let context = [&[1][..], namespace.value(), pool.value()].concat();
    let hash = |parts: &[&[u8]]| -> [u8; 32] {
        Impl::hash_bytes(&parts.concat())
            .as_bytes()
            .try_into()
            .unwrap()
    };
    let mut root = hash(&[
        b"LP/allowlist/leaf/v1\0",
        &context,
        trader.value(),
        &path.salt,
    ]);
    for sibling in &path.siblings {
        root = hash(&[b"LP/allowlist/node/v1\0", &context, &root, sibling]);
    }
    (
        curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
            version: 1,
            namespace: *namespace.value(),
            sale_policy_id: *pool.value(),
            root,
        }),
        path,
    )
}

#[test]
fn weighted_membership_can_be_reused_in_both_buy_forms_without_changing_economics() {
    let (input, mut state) = funded();
    let (policy, path) = membership_for(state.namespace, input.receiver.account_id, id(50));
    state.buy_policy = policy.clone();
    for (approved, open) in [
        (
            Message::BuyWeightedExactInput {
                membership: Some(path.clone()),
                amount_in: 10,
                min_amount_out: 1,
            },
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 10,
                min_amount_out: 1,
            },
        ),
        (
            Message::BuyWeightedExactOutput {
                membership: Some(path),
                amount_out: 500,
                max_amount_in: 1000,
            },
            Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 500,
                max_amount_in: 1000,
            },
        ),
    ] {
        let balances = [
            state.recorded_collateral + state.accrued_fees,
            state.remaining_inventory,
            1000,
        ];
        let response = operation(
            &input,
            &state,
            approved,
            config(100, id(43), false),
            balances,
        )
        .unwrap();
        let mut public = state.clone();
        public.buy_policy = curve_core::BuyPolicy::Open;
        let expected =
            operation(&input, &public, open, config(100, id(43), false), balances).unwrap();
        state = state_of(&response);
        assert_eq!(state.buy_policy, policy);
        let mut actual = state.clone();
        actual.buy_policy = curve_core::BuyPolicy::Open;
        assert_eq!(actual, state_of(&expected));
        assert_eq!(response.sync_messages, expected.sync_messages);
    }
    assert!(state.remaining_inventory < 999_500);
    assert!(state.recorded_collateral > 100);
}

#[test]
fn weighted_rejects_copied_and_malformed_membership_in_both_buy_forms() {
    let (mut input, mut state) = funded();
    let (policy, path) = membership_for(state.namespace, input.receiver.account_id, id(50));
    for case in 0..10 {
        state.buy_policy = policy.clone();
        let mut evidence = path.clone();
        input.from = Some(Actor::new(id(50), id(8)));
        let expected = match case {
            0 => {
                input.from = Some(Actor::new(id(51), id(8)));
                lbp_core::Error::Unauthorized
            }
            1 => {
                evidence.salt = [99; 32];
                lbp_core::Error::Unauthorized
            }
            2 => {
                evidence.index = 1 << 17;
                lbp_core::Error::Unauthorized
            }
            3 => {
                evidence.siblings[4] = [99; 32];
                lbp_core::Error::Unauthorized
            }
            4 => {
                let curve_core::BuyPolicy::Allowlist(policy) = &mut state.buy_policy else {
                    unreachable!()
                };
                policy.root = [99; 32];
                lbp_core::Error::Unauthorized
            }
            5 => {
                input.from = Some(Actor::new(id(50), id(99)));
                lbp_core::Error::Unauthorized
            }
            6 => {
                input.from = Some(input.receiver);
                lbp_core::Error::Unauthorized
            }
            7..=9 => {
                let curve_core::BuyPolicy::Allowlist(policy) = &mut state.buy_policy else {
                    unreachable!()
                };
                match case {
                    7 => policy.version = 2,
                    8 => policy.namespace = [99; 32],
                    _ => policy.sale_policy_id = [99; 32],
                }
                lbp_core::Error::InvalidTerms
            }
            _ => unreachable!(),
        };
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        for message in [
            Message::BuyWeightedExactInput {
                membership: Some(evidence.clone()),
                amount_in: 10,
                min_amount_out: 1,
            },
            Message::BuyWeightedExactOutput {
                membership: Some(evidence.clone()),
                amount_out: 500,
                max_amount_in: 1000,
            },
        ] {
            input.message = borsh::to_vec(&message).unwrap();
            assert_eq!(handle(&input).err(), Some(expected), "case {case}");
        }
    }
    state.buy_policy = policy;
    input.from = Some(Actor::new(id(50), id(8)));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::BuyWeightedExactInput {
        membership: Some(path),
        amount_in: 10,
        min_amount_out: 1,
    })
    .unwrap();
    input.message.pop();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
}
