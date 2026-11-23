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
        per_block_token_limit: None,
        buy_policy: curve_core::BuyPolicy::Open,
        pause_policy: lbp_core::PausePolicy::AuthorityControlled,
        ending_policy: lbp_core::EndingPolicy::Scheduled,
        trade_mode: lbp_core::TradeMode::BuyOnly,
        sell_fee_bps: 0,
        graduation_adapter: None,
        graduation_policy: sale_policy::GraduationPolicy::default(),
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
    input = ReceiveInput {
        receiver: input.receiver,
        from: Some(clock_core::clock_actor()),
        is_authorized: false,
        pre_state: response.post_state.unwrap(),
        message: borsh::to_vec(&clock_core::Reply::Timestamp(1_000_000)).unwrap(),
    };
    response = handle(&input).unwrap();
    (input, response)
}

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
    if response
        .post_state
        .as_ref()
        .unwrap()
        .starts_with(b"LZCLK001")
    {
        input = ReceiveInput {
            receiver: input.receiver,
            from: Some(clock_core::clock_actor()),
            is_authorized: false,
            pre_state: response.post_state.unwrap(),
            message: borsh::to_vec(&clock_core::Reply::Timestamp(1_000_000)).unwrap(),
        };
        response = handle(&input).unwrap();
    }
    let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    input.from = Some(Actor::new(id(50), id(8)));
    (input, state)
}

/// Complete a trade or fee collection with fixture replies.
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
        holding(
            id(50),
            if matches!(
                message,
                Message::SellExactInput { .. } | Message::SellWeightedExactInput { .. }
            ) {
                id(41)
            } else {
                id(42)
            },
            balances[2],
        ),
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
            message: borsh::to_vec(&clock_core::Reply::Timestamp(timestamp_ms)).unwrap(),
        };
        response = handle(&input)?;
    }
    if response
        .sync_messages
        .last()
        .is_some_and(|call| call.to == clock_core::clock_actor())
    {
        assert_eq!(
            borsh::from_slice::<clock_core::Message>(
                &response.sync_messages.last().unwrap().message
            )
            .unwrap(),
            clock_core::Message::ReadBlockId { granularity: 1 }
        );
        input = ReceiveInput {
            receiver: input.receiver,
            from: Some(clock_core::clock_actor()),
            is_authorized: false,
            pre_state: response.post_state.unwrap(),
            message: borsh::to_vec(&clock_core::Reply::BlockId(42)).unwrap(),
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

// ---------------------------------------------------------------------------
// Settlement helper
// ---------------------------------------------------------------------------

/// Complete settlement or fee collection with fixture replies.
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

    input.message = borsh::to_vec(&Message::SetWeightedPaused { paused: true }).unwrap();
    let response = handle(&input).unwrap();
    assert!(!response.events.is_empty(), "first pause should emit event");
    let paused_state =
        lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(paused_state.trade_control.is_paused());

    input.pre_state = response.post_state.unwrap();
    let response = handle(&input).unwrap();
    assert!(
        response.events.is_empty(),
        "repeated pause should not emit event"
    );

    input.pre_state = response.post_state.unwrap();
    input.message = borsh::to_vec(&Message::SetWeightedPaused { paused: false }).unwrap();
    let response = handle(&input).unwrap();
    assert!(!response.events.is_empty(), "resume should emit event");

    input.pre_state = response.post_state.unwrap();
    let response = handle(&input).unwrap();
    assert!(
        response.events.is_empty(),
        "repeated resume should not emit event"
    );
}

#[test]
fn weighted_pause_blocks_both_buy_forms() {
    let (mut input, mut state) = funded();
    state.trade_control.set_paused(true).unwrap();
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
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.message = borsh::to_vec(&message).unwrap();
        // Reject at entry, before any namespace, holding or clock request.
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::Paused));
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
    state.trade_control.set_paused(true).unwrap();
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
    state.settlement_progress = lbp_core::SettlementProgress::ManualComplete;
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

    fixed.ending_policy = lbp_core::EndingPolicy::Either;
    input.from = Some(Actor::new(fixed.factory_owner, fixed.factory_program_id));
    input.pre_state = borsh::to_vec(&fixed).unwrap().into();
    input.message = borsh::to_vec(&Message::SetWeightedPaused { paused: true }).unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
}

// ---------------------------------------------------------------------------
// Settlement payouts and fees
// ---------------------------------------------------------------------------

#[test]
fn weighted_settlement_pays_inventory_and_recorded_collateral_and_collects_fees() {
    let (input, mut state) = funded();
    // Simulate a buy: 50 in, fee 1, net 49 recorded
    state.recorded_collateral = 149; // seed(100) + net(49)
    state.accrued_fees = 1;
    state.remaining_inventory = 999_500;
    if let lbp_core::PoolMode::Weighted {
        pricing_collateral, ..
    } = &mut state.mode
    {
        *pricing_collateral = 149;
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
    assert!(next.settlement_progress.is_complete());
    assert_eq!(next.recorded_collateral, 0);
    assert_eq!(next.accrued_fees, 0);

    let messages = &response.sync_messages;
    assert!(
        messages.len() >= 2,
        "expected fee, inventory and collateral transfers"
    );
}

#[test]
fn weighted_settlement_requires_factory_authorization() {
    let (mut input, state) = funded();
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
    assert!(settled.settlement_progress.is_complete());

    let mut input2 = input.clone();
    input2.from = Some(Actor::new(
        settled.factory_owner,
        settled.factory_program_id,
    ));
    input2.pre_state = borsh::to_vec(&settled).unwrap().into();
    input2.message = borsh::to_vec(&Message::SettleWeighted { creator: id(50) }).unwrap();
    let response2 = handle(&input2).unwrap();
    let still_settled = state_of(&response2);
    assert!(still_settled.settlement_progress.is_complete());
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
    assert!(next.settlement_progress.is_complete());
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
    state.trade_control.set_paused(true).unwrap();
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
    assert!(next.settlement_progress.is_complete());
    // recorded_collateral is zeroed — only the original 100 is paid, not 500
    assert_eq!(next.recorded_collateral, 0);
}

#[test]
fn weighted_fixed_price_settlement_rejects_weighted_pool() {
    let (input, state) = funded();
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

// ---------------------------------------------------------------------------
// Poke and observer checkpoints
// ---------------------------------------------------------------------------

fn poke_at(
    timestamp_ms: u64,
    input: &ReceiveInput,
    state: &lbp_core::PoolAccount,
) -> Result<lee_core::program::Response, lbp_core::Error> {
    let mut input = input.clone();
    input.pre_state = borsh::to_vec(state).unwrap().into();
    input.message = borsh::to_vec(&Message::PokeWeighted).unwrap();
    // Anyone can send poke — use a random sender
    input.from = Some(Actor::new(id(99), id(8)));
    let response = handle(&input)?;
    let source = response.sync_messages.last().unwrap().to;
    assert_eq!(source, lbp_core::pricing_clock::actor());
    let input = ReceiveInput {
        receiver: input.receiver,
        from: Some(source),
        is_authorized: false,
        pre_state: response.post_state.unwrap(),
        message: borsh::to_vec(&clock_core::Reply::Timestamp(timestamp_ms)).unwrap(),
    };
    handle(&input)
}

fn checkpoint(state: &lbp_core::PoolAccount) -> (u64, u128) {
    let lbp_core::PoolMode::Weighted {
        checkpoint_time,
        checkpoint_token_weight,
        ..
    } = state.mode
    else {
        panic!("expected weighted state");
    };
    (checkpoint_time, checkpoint_token_weight)
}

fn checkpoint_events(
    response: &lee_core::program::Response,
) -> Vec<&lee_core::program::ProgramEvent> {
    response
        .events
        .iter()
        .filter(|event| event.selector == [0xae, 0x2f, 0x6d, 0x45, 0x7f, 0xb3, 0x79, 0xcf])
        .collect()
}
fn assert_checkpoint_events_match(
    actual: &lee_core::program::Response,
    expected: &lee_core::program::Response,
) {
    let actual = checkpoint_events(actual);
    let expected = checkpoint_events(expected);
    assert_eq!(actual.len(), 1);
    assert_eq!(expected.len(), 1);
    assert_eq!(actual[0].selector, expected[0].selector);
    assert_eq!(actual[0].data, expected[0].data);
}

#[test]
fn poke_is_permissionless_and_records_checkpoint() {
    let (input, state) = funded();
    assert_eq!(checkpoint(&state), (0, 0));
    let response = poke_at(1_500_000, &input, &state).unwrap();
    let after = state_of(&response);
    let (time, token_w) = checkpoint(&after);
    assert_eq!(time, 1500);
    // At midpoint of 1000..3000, token weight interpolates between 95% and 50%
    let expected = interpolate_weight(weight(95), weight(50), 1000, 3000, 1500);
    assert_eq!(token_w, expected);
    assert_eq!(checkpoint_events(&response).len(), 1);
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let hash = Impl::hash_bytes(b"lbp::WeightedCheckpoint");
    assert_eq!(
        checkpoint_events(&response)[0].selector,
        hash.as_bytes()[..8]
    );
    let mut payload = 1500u64.to_le_bytes().to_vec();
    payload.extend_from_slice(&expected.to_le_bytes());
    assert_eq!(checkpoint_events(&response)[0].data, payload);
}

#[test]
fn poke_authenticates_upstream_replies_and_rejects_replayed_replies() {
    let (mut input, state) = funded();
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::PokeWeighted).unwrap();
    let pending = handle(&input).unwrap();
    assert_eq!(pending.sync_messages.len(), 1);
    assert_eq!(
        pending.sync_messages[0].message,
        borsh::to_vec(&clock_core::Message::ReadTimestamp { granularity: 1 }).unwrap()
    );
    input.pre_state = pending.post_state.unwrap();
    input.from = Some(clock_core::clock_actor());
    input.message = borsh::to_vec(&clock_core::Reply::Timestamp(1_500_999)).unwrap();
    let valid = input.clone();
    let clock = clock_core::clock_actor();
    for sender in [
        None,
        Some(Actor::new(id(99), clock.program_account_id)),
        Some(Actor::new(clock.account_id, id(99))),
    ] {
        input.from = sender;
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
    }
    for bytes in [
        vec![],
        vec![255],
        borsh::to_vec(&clock_core::Reply::BlockId(1500)).unwrap(),
    ] {
        input = valid.clone();
        input.message = bytes;
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
    }
    let completed = handle(&valid).unwrap();
    assert_eq!(checkpoint(&state_of(&completed)).0, 1500);
    input = valid;
    input.pre_state = completed.post_state.unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
}

#[test]
fn poke_at_start_records_start_weight() {
    let (input, state) = funded();
    let response = poke_at(1_000_000, &input, &state).unwrap();
    let (time, w) = checkpoint(&state_of(&response));
    assert_eq!(time, 1000);
    assert_eq!(w, weight(95));
}

#[test]
fn poke_at_end_records_end_weight() {
    let (input, state) = funded();
    // At end_timestamp or beyond, interpolate_weight clamps to end
    let response = poke_at(3_000_000, &input, &state).unwrap();
    let (time, w) = checkpoint(&state_of(&response));
    assert_eq!(time, 3000);
    assert_eq!(w, weight(50));
}

#[test]
fn repeated_poke_at_same_time_is_idempotent_no_event() {
    let (input, state) = funded();
    let first = poke_at(1_500_000, &input, &state).unwrap();
    assert_eq!(checkpoint_events(&first).len(), 1);
    let after_first = state_of(&first);
    let second = poke_at(1_500_000, &input, &after_first).unwrap();
    assert_eq!(checkpoint_events(&second).len(), 0);
    assert_eq!(checkpoint(&state_of(&second)), checkpoint(&after_first));
}

#[test]
fn poke_rejects_unfunded_pool() {
    let (_, prepared) = prepared();
    let state = lbp_core::PoolAccount::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    let factory = AccountId::for_public_pda(&id(7), &PdaSeed::new([3; 32]));
    let input = ReceiveInput {
        receiver: Actor::new(
            compute_pool_pda(id(40), id(8), id(41), id(42), factory),
            id(8),
        ),
        from: Some(Actor::new(id(99), id(8))),
        is_authorized: false,
        pre_state: borsh::to_vec(&state).unwrap().into(),
        message: borsh::to_vec(&Message::PokeWeighted).unwrap(),
    };
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::PendingFunding));
}

#[test]
fn buy_updates_checkpoint_on_new_timestamp() {
    let (input, state) = funded();
    assert_eq!(checkpoint(&state), (0, 0));
    let response = operation_at(
        1_500_000,
        &input,
        &state,
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        config(0, id(43), false),
        [100, 1_000_000, 1000],
    )
    .unwrap();
    let after = state_of(&response);
    let (time, w) = checkpoint(&after);
    assert_eq!(time, 1500);
    assert!(w > 0);
    let poked = poke_at(1_500_000, &input, &state).unwrap();
    assert_checkpoint_events_match(&response, &poked);
    let repeated = poke_at(1_500_000, &input, &after).unwrap();
    assert!(checkpoint_events(&repeated).is_empty());
}

#[test]
fn buy_at_same_timestamp_does_not_change_checkpoint() {
    let (input, state) = funded();
    let first = operation_at(
        1_500_000,
        &input,
        &state,
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        config(0, id(43), false),
        [100, 1_000_000, 1000],
    )
    .unwrap();
    let after_first = state_of(&first);
    let cp1 = checkpoint(&after_first);
    assert_eq!(cp1.0, 1500);
    let second = operation_at(
        1_500_000,
        &input,
        &after_first,
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        config(0, id(43), false),
        [110, after_first.remaining_inventory, 1000],
    )
    .unwrap();
    let cp2 = checkpoint(&state_of(&second));
    assert_eq!(cp1, cp2);
    assert!(checkpoint_events(&second).is_empty());
}

#[test]
fn buy_at_different_timestamp_updates_checkpoint() {
    let (input, state) = funded();
    let first = operation_at(
        1_500_000,
        &input,
        &state,
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        config(0, id(43), false),
        [100, 1_000_000, 1000],
    )
    .unwrap();
    let after_first = state_of(&first);
    let cp1 = checkpoint(&after_first);
    let second = operation_at(
        2_000_000,
        &input,
        &after_first,
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        config(0, id(43), false),
        [110, after_first.remaining_inventory, 1000],
    )
    .unwrap();
    let cp2 = checkpoint(&state_of(&second));
    assert_eq!(cp2.0, 2000);
    assert_ne!(cp1, cp2);
    let poked = poke_at(2_000_000, &input, &after_first).unwrap();
    assert_checkpoint_events_match(&second, &poked);
}

#[test]
fn exact_output_checkpoint_event_matches_poke_with_fractional_weights() {
    let (input, mut state) = funded();
    for (start, end) in [
        (weight(95) + 1, weight(50) + 7),
        (weight(50) + 7, weight(95) + 1),
    ] {
        if let lbp_core::PoolMode::Weighted {
            token_weight_start,
            token_weight_end,
            ..
        } = &mut state.mode
        {
            *token_weight_start = start;
            *token_weight_end = end;
        }
        let time_ms = 1_501_000;
        let response = operation_at(
            time_ms,
            &input,
            &state,
            Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 500,
                max_amount_in: 1000,
            },
            config(0, id(43), false),
            [100, 1_000_000, 1000],
        )
        .unwrap();
        let poked = poke_at(time_ms, &input, &state).unwrap();
        assert_eq!(
            checkpoint(&state_of(&response)),
            checkpoint(&state_of(&poked))
        );
        assert_checkpoint_events_match(&response, &poked);
        let coll_w = interpolate_weight(
            lbp_core::WEIGHT_TOTAL - start,
            lbp_core::WEIGHT_TOTAL - end,
            1000,
            3000,
            1501,
        );
        assert_eq!(
            checkpoint(&state_of(&response)).1,
            lbp_core::WEIGHT_TOTAL - coll_w
        );
        let expected_in =
            calc_in_given_out(100, coll_w, 1_000_000, lbp_core::WEIGHT_TOTAL - coll_w, 500);
        assert_eq!(
            state_of(&response).recorded_collateral,
            state.recorded_collateral + expected_in
        );
    }
}

#[test]
fn failed_buy_does_not_change_checkpoint() {
    let (input, state) = funded();
    let poked = poke_at(1_500_000, &input, &state).unwrap();
    let after_poke = state_of(&poked);
    let cp = checkpoint(&after_poke);
    let result = operation_at(
        2_000_000,
        &input,
        &after_poke,
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: u128::MAX, // impossible slippage
        },
        config(0, id(43), false),
        [100, 1_000_000, 1000],
    );
    assert!(result.is_err());
    // Original state is unchanged (the pre_state would be restored by the VM)
    assert_eq!(checkpoint(&after_poke), cp);
}

#[test]
fn poke_on_settled_pool_still_works() {
    let (input, mut state) = funded();
    state.settlement_progress = lbp_core::SettlementProgress::ManualComplete;
    state.recorded_collateral = 0;
    let response = poke_at(1_500_000, &input, &state).unwrap();
    let (time, _) = checkpoint(&state_of(&response));
    assert_eq!(time, 1500);
}

#[test]
fn common_weighted_controls_match_legacy_events_and_keep_the_scheduled_end() {
    let (mut input, mut state) = funded();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    state.remaining_inventory = 0;
    for paused in [false, true, true, false, false] {
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.message = borsh::to_vec(&Message::SetWeightedPaused { paused }).unwrap();
        let legacy = handle(&input).unwrap();
        input.message = borsh::to_vec(&Message::SetPaused { paused }).unwrap();
        let common = handle(&input).unwrap();
        assert_eq!(common.post_state, legacy.post_state);
        assert_eq!(common.events.len(), legacy.events.len());
        let changed = state.trade_control.is_paused() != paused;
        assert_eq!(common.events.len(), 2 * usize::from(changed));
        if changed {
            let selector = [109, 140, 24, 3, 232, 43, 58, 157];
            let common_event = common
                .events
                .iter()
                .find(|e| e.selector == selector)
                .unwrap();
            let legacy_event = legacy
                .events
                .iter()
                .find(|e| e.selector == selector)
                .unwrap();
            assert_eq!(common_event.data, borsh::to_vec(&paused).unwrap());
            assert_eq!(common_event.data, legacy_event.data);
        }
        assert_eq!(common.validity.timestamps.start(), None);
        assert_eq!(common.validity.timestamps.end(), Some(3_000_000));
        state = state_of(&common);
        assert_eq!(
            state.trade_control.policy(),
            lbp_core::PausePolicy::AuthorityControlled
        );
        for from in [
            None,
            Some(Actor::new(id(50), id(8))),
            Some(Actor::new(id(99), state.factory_program_id)),
            Some(Actor::new(state.factory_owner, id(99))),
        ] {
            let mut forged = input.clone();
            forged.from = from;
            forged.pre_state = common.post_state.clone().unwrap();
            forged.is_authorized = true;
            assert_eq!(handle(&forged).err(), Some(lbp_core::Error::Unauthorized));
        }
    }
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), true),
        [100, 0],
    )
    .unwrap();
    assert_eq!(response.validity.timestamps.start(), Some(3_000_000));
}

#[test]
fn weighted_controls_reject_disabled_unfunded_settled_and_overflow_even_on_noops() {
    let (mut input, original) = funded();
    input.from = Some(Actor::new(
        original.factory_owner,
        original.factory_program_id,
    ));
    for legacy in [false, true] {
        for paused in [false, true] {
            for case in 0..4 {
                let mut state = original.clone();
                state.trade_control.set_paused(paused).unwrap();
                let expected = match case {
                    0 => {
                        state.trade_control = lbp_core::TradeControl::Unrestricted;
                        lbp_core::Error::PauseDisabled
                    }
                    1 => {
                        state.funded = false;
                        lbp_core::Error::PendingFunding
                    }
                    2 => {
                        state.settlement_progress = lbp_core::SettlementProgress::ManualComplete;
                        lbp_core::Error::Closed
                    }
                    _ => {
                        state.end_timestamp = Some(u64::MAX);
                        lbp_core::Error::InvalidTerms
                    }
                };
                input.pre_state = borsh::to_vec(&state).unwrap().into();
                input.message = borsh::to_vec(&if legacy {
                    Message::SetWeightedPaused { paused }
                } else {
                    Message::SetPaused { paused }
                })
                .unwrap();
                assert_eq!(handle(&input).err(), Some(expected));
            }
        }
    }
}

#[test]
fn common_weighted_resume_keeps_namespace_freeze_for_both_buys() {
    let (mut input, state) = funded();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SetPaused { paused: true }).unwrap();
    let response = handle(&input).unwrap();
    input.pre_state = response.post_state.unwrap();
    input.message = borsh::to_vec(&Message::SetPaused { paused: false }).unwrap();
    let resumed = state_of(&handle(&input).unwrap());
    input.from = Some(Actor::new(id(50), id(8)));
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
        assert_eq!(
            operation(
                &input,
                &resumed,
                message.clone(),
                config(0, id(43), true),
                [100, 1_000_000, 1000]
            )
            .err(),
            Some(lbp_core::Error::Frozen)
        );
        assert!(
            operation(
                &input,
                &resumed,
                message,
                config(0, id(43), false),
                [100, 1_000_000, 1000]
            )
            .is_ok()
        );
    }
}

#[test]
fn weighted_shared_closing_keeps_the_schedule_after_depletion_and_payout() {
    let (mut input, original) = funded();
    for remaining in [0, original.remaining_inventory] {
        let mut state = original.clone();
        state.remaining_inventory = remaining;
        assert_eq!(state.close_time_bound().unwrap(), Some(3000));
        let response = settle(
            &input,
            &state,
            Message::SettleWeighted { creator: id(50) },
            config(0, id(43), true),
            [117, remaining + 7],
        )
        .unwrap();
        let next = state_of(&response);
        assert_eq!(next.remaining_inventory, remaining);
        assert_eq!(next.close_time_bound().unwrap(), Some(3000));
        assert_eq!(response.validity.timestamps.start(), Some(3_000_000));
        assert_eq!(response.validity.timestamps.end(), None);
        input.from = Some(Actor::new(next.factory_owner, next.factory_program_id));
        input.pre_state = borsh::to_vec(&next).unwrap().into();
        input.message = borsh::to_vec(&Message::SettleWeighted { creator: id(50) }).unwrap();
        let retry = handle(&input).unwrap();
        assert!(retry.sync_messages.is_empty());
        assert_eq!(retry.validity.timestamps.start(), Some(3_000_000));
        assert_eq!(state_of(&retry), next);
    }
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

#[test]
fn pending_poke_rechecks_saved_policy_and_schedule_before_clock_reply() {
    let (input, original) = funded();
    for case in 0..4 {
        let mut state = original.clone();
        match case {
            0 => state.ending_policy = lbp_core::EndingPolicy::Manual,
            1 => state.ending_policy = lbp_core::EndingPolicy::Depletion,
            2 => state.end_timestamp = None,
            _ => state.start_timestamp = u64::MAX,
        }
        let expected = if case < 2 {
            lbp_core::Error::InvalidState
        } else {
            lbp_core::Error::InvalidTerms
        };
        let callback = ReceiveInput {
            receiver: input.receiver,
            from: Some(lbp_core::pricing_clock::actor()),
            is_authorized: false,
            pre_state: borsh::to_vec(&(*b"LZLBPOKE", state)).unwrap().into(),
            message: borsh::to_vec(&clock_core::Reply::Timestamp(1_500_000)).unwrap(),
        };
        assert_eq!(handle(&callback).err(), Some(expected));
    }
}

#[test]
fn weighted_either_settlement_requires_depletion_or_the_schedule_and_keeps_retry_evidence() {
    let (input, original) = funded();
    for ending in [
        lbp_core::EndingPolicy::Scheduled,
        lbp_core::EndingPolicy::Either,
    ] {
        for remaining in [0, 1] {
            let mut state = original.clone();
            state.ending_policy = ending;
            state.remaining_inventory = remaining;
            let expected = if ending == lbp_core::EndingPolicy::Either && remaining == 0 {
                None
            } else {
                Some(3_000_000)
            };
            let response = settle(
                &input,
                &state,
                Message::SettleWeighted { creator: id(50) },
                config(100, id(43), true),
                [100, remaining],
            )
            .unwrap();
            assert_eq!(response.validity.timestamps.start(), expected);
            let next = state_of(&response);
            assert!(next.settlement_progress.is_complete());
            assert_eq!(next.remaining_inventory, remaining);
            assert_eq!(
                next.close_time_bound().unwrap(),
                expected.map(|time| time / 1000)
            );
            let mut retry = input.clone();
            retry.from = Some(Actor::new(next.factory_owner, next.factory_program_id));
            retry.pre_state = borsh::to_vec(&next).unwrap().into();
            retry.message = borsh::to_vec(&Message::SettleWeighted { creator: id(50) }).unwrap();
            let repeated = handle(&retry).unwrap();
            assert_eq!(repeated.validity.timestamps.start(), expected);
            assert!(repeated.sync_messages.is_empty());
        }
    }
}

#[test]
fn weighted_collection_and_settlement_reject_unbacked_pricing_or_reduced_seed() {
    let (input, original) = funded();
    for (recorded, pricing) in [(99, 99), (100, 101), (101, 100)] {
        let mut state = original.clone();
        state.recorded_collateral = recorded;
        if let lbp_core::PoolMode::Weighted {
            pricing_collateral, ..
        } = &mut state.mode
        {
            *pricing_collateral = pricing;
        }
        assert_eq!(
            operation(
                &input,
                &state,
                Message::CollectFees,
                config(100, id(43), false),
                [1000, 1_000_000, 1000]
            )
            .err(),
            Some(lbp_core::Error::InvalidState)
        );
        assert_eq!(
            settle(
                &input,
                &state,
                Message::SettleWeighted { creator: id(50) },
                config(100, id(43), false),
                [1000, 1_000_000]
            )
            .err(),
            Some(lbp_core::Error::InvalidState)
        );
    }
}

#[test]
fn weighted_atomic_settlement_pays_seed_in_recorded_collateral_once() {
    use lbp_core::SettlementProgress;
    use lez_compat_core::token::Message as TokenMessage;
    let (input, original) = funded();
    for (proceeds, fees, inventory) in [
        (100, 0, 1_000_000),
        (100, 1, 999_500),
        (149, 1, 999_500),
        (100, 0, 0),
        (149, 1, 0),
    ] {
        let mut state = original.clone();
        state.recorded_collateral = proceeds;
        state.accrued_fees = fees;
        state.remaining_inventory = inventory;
        if let lbp_core::PoolMode::Weighted {
            pricing_collateral, ..
        } = &mut state.mode
        {
            *pricing_collateral = proceeds;
        }
        let response = settle(
            &input,
            &state,
            Message::SettleWeighted { creator: id(50) },
            config(9999, id(43), true),
            [proceeds + fees + 17, inventory + 7],
        )
        .unwrap();
        let mut expected = state.clone();
        expected.recorded_collateral = 0;
        expected.accrued_fees = 0;
        expected.closure_observed = true;
        expected.settlement_progress = SettlementProgress::ManualComplete;
        assert_eq!(state_of(&response), expected);
        assert_eq!(
            response.post_state.as_ref().unwrap().to_vec(),
            borsh::to_vec(&expected).unwrap()
        );
        assert_eq!(response.validity.timestamps.start(), Some(3_000_000));
        assert_eq!(response.validity.timestamps.end(), None);
        let mut calls = vec![];
        if fees != 0 {
            calls.push(TokenMessage::Transfer {
                definition: id(42),
                recipient: id(43),
                amount: fees,
            });
        }
        if inventory != 0 {
            calls.push(TokenMessage::Transfer {
                definition: id(41),
                recipient: id(50),
                amount: inventory,
            });
        }
        calls.push(TokenMessage::Transfer {
            definition: id(42),
            recipient: id(50),
            amount: proceeds,
        });
        assert_eq!(
            response
                .sync_messages
                .iter()
                .map(|call| borsh::from_slice::<TokenMessage>(&call.message).unwrap())
                .collect::<Vec<_>>(),
            calls
        );
        let mut retry = input.clone();
        retry.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
        retry.pre_state = response.post_state.clone().unwrap();
        retry.message = borsh::to_vec(&Message::SettleWeighted { creator: id(50) }).unwrap();
        let repeated = handle(&retry).unwrap();
        assert_eq!(repeated.post_state, response.post_state);
        assert_eq!(repeated.validity.timestamps.start(), Some(3_000_000));
        assert!(repeated.sync_messages.is_empty());
        assert!(repeated.events.is_empty());
    }
}

#[test]
fn weighted_settlement_rejects_recorded_seed_shortfall_even_with_vault_surplus() {
    let (input, original) = funded();
    for recorded in [0, 1, 99] {
        for inventory in [0, original.remaining_inventory] {
            let mut state = original.clone();
            state.recorded_collateral = recorded;
            state.remaining_inventory = inventory;
            assert_eq!(
                settle(
                    &input,
                    &state,
                    Message::SettleWeighted { creator: id(50) },
                    config(100, id(43), false),
                    [1000, inventory + 7]
                )
                .err(),
                Some(lbp_core::Error::InvalidState)
            );
        }
    }
}

#[test]
fn weighted_completed_retries_still_check_factory_mode_and_recipient() {
    use lbp_core::SettlementProgress;
    let (mut input, mut state) = funded();
    for progress in [
        SettlementProgress::ManualComplete,
        SettlementProgress::Graduated,
    ] {
        state.settlement_progress = progress;
        state.recorded_collateral = 0;
        state.remaining_inventory = 0;
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.message = borsh::to_vec(&Message::SettleWeighted { creator: id(50) }).unwrap();
        for from in [
            None,
            Some(Actor::new(id(50), id(8))),
            Some(Actor::new(id(99), state.factory_program_id)),
            Some(Actor::new(state.factory_owner, id(99))),
        ] {
            input.from = from;
            assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
        }
        input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
        let retry = handle(&input).unwrap();
        assert_eq!(state_of(&retry), state);
        assert!(retry.sync_messages.is_empty());
        assert_eq!(retry.validity.timestamps.start(), Some(3_000_000));
        input.message = borsh::to_vec(&Message::SettleWeighted {
            creator: AccountId::default(),
        })
        .unwrap();
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
        input.message = borsh::to_vec(&Message::SettleFixedPrice {
            creator: id(50),
            now: 3000,
        })
        .unwrap();
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
    }
}

#[test]
fn weighted_completed_progress_blocks_controls_and_both_buy_forms() {
    use lbp_core::SettlementProgress;
    let (mut input, mut state) = funded();
    for progress in [
        SettlementProgress::ManualComplete,
        SettlementProgress::Graduated,
    ] {
        state.settlement_progress = progress;
        input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        for message in [
            Message::SetPaused { paused: false },
            Message::SetWeightedPaused { paused: false },
        ] {
            input.message = borsh::to_vec(&message).unwrap();
            assert_eq!(handle(&input).err(), Some(lbp_core::Error::Closed));
        }
        input.from = Some(Actor::new(id(50), id(8)));
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
                    config(100, id(43), false),
                    [100, state.remaining_inventory, 1000]
                )
                .err(),
                Some(lbp_core::Error::Closed)
            );
        }
    }
}

#[test]
fn receipt_settlement_keeps_selected_ending_and_net_collateral_for_both_routes() {
    let (input, original) = funded();
    for ending in [
        lbp_core::EndingPolicy::Scheduled,
        lbp_core::EndingPolicy::Either,
    ] {
        for remaining in [0, 1] {
            for route in [
                lbp_core::SettlementRoute::Manual,
                lbp_core::SettlementRoute::Graduation,
            ] {
                let mut state = original.clone();
                state.ending_policy = ending;
                state.remaining_inventory = remaining;
                state.recorded_collateral = 149;
                state.accrued_fees = 1;
                state.graduation_adapter = Some(Actor::new(id(70), id(71)));
                if let lbp_core::PoolMode::Weighted {
                    pricing_collateral, ..
                } = &mut state.mode
                {
                    *pricing_collateral = 149;
                }
                if route == lbp_core::SettlementRoute::Manual {
                    state.graduation_adapter = None;
                }
                let adapter = state.graduation_adapter;
                let message = Message::Settle {
                    route,
                    creator: id(50),
                    adapter,
                };
                let response = settle(
                    &input,
                    &state,
                    message.clone(),
                    config(100, id(43), false),
                    [150, remaining],
                )
                .unwrap();
                let expected = if ending == lbp_core::EndingPolicy::Either && remaining == 0 {
                    None
                } else {
                    Some(3_000_000)
                };
                assert_eq!(response.validity.timestamps.start(), expected);
                let lbp_core::SettlementReply::Applied { receipt } =
                    borsh::from_slice(&response.sync_messages.last().unwrap().message).unwrap()
                else {
                    panic!("expected applied receipt")
                };
                assert_eq!(receipt.eligible.collateral, 149);
                assert_eq!(
                    receipt.payouts.creator.collateral + receipt.payouts.adapter.collateral,
                    149
                );
                let mut finalize = input.clone();
                finalize.from = Some(receipt.factory);
                finalize.pre_state = response.post_state.unwrap();
                finalize.message = borsh::to_vec(&Message::FinalizeSettlement {
                    receipt_hash: receipt.hash(),
                })
                .unwrap();
                let completed = handle(&finalize).unwrap();
                let next = state_of(&completed);
                assert_eq!(next.remaining_inventory, remaining);
                assert_eq!(next.settlement_progress, receipt.progress);
                assert_eq!(completed.validity.timestamps.start(), expected);
                if let lbp_core::PoolMode::Weighted {
                    pricing_collateral, ..
                } = &mut state.mode
                {
                    *pricing_collateral = 150;
                }
                assert_eq!(
                    settle(
                        &input,
                        &state,
                        message,
                        config(100, id(43), false),
                        [150, remaining]
                    )
                    .err(),
                    Some(lbp_core::Error::InvalidState)
                );
            }
        }
    }
}

fn sale_events(response: &lee_core::program::Response) -> Vec<curve_core::events::SaleEventV1> {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let hash = Impl::hash_bytes(curve_core::events::SaleEventV1::SELECTOR_NAME.as_bytes());
    response
        .events
        .iter()
        .filter(|e| e.selector == hash.as_bytes()[..8])
        .map(|e| borsh::from_slice(&e.data).unwrap())
        .collect()
}

#[test]
fn weighted_trade_events_use_net_pricing_input_and_trusted_time_for_both_forms() {
    use curve_core::events::*;
    let (input, state) = funded();
    for (message, form) in [
        (
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: 1,
            },
            TradeFormV1::ExactInput,
        ),
        (
            Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 500,
                max_amount_in: 1000,
            },
            TradeFormV1::ExactOutput,
        ),
    ] {
        let response = operation_at(
            1_500_999,
            &input,
            &state,
            message,
            config(100, id(43), false),
            [100, state.remaining_inventory, 1000],
        )
        .unwrap();
        let next = state_of(&response);
        let events = sale_events(&response);
        assert_eq!(events.len(), 1);
        let fees = next.accrued_fees - state.accrued_fees;
        let gross = next.recorded_collateral - state.recorded_collateral + fees;
        assert_eq!(events[0].engine, PricingEngineV1::Weighted);
        assert_eq!(
            events[0].kind,
            SaleEventKindV1::Trade {
                trader: *id(50).value(),
                form,
                token_in: *id(42).value(),
                token_out: *id(41).value(),
                amount_in: gross,
                pricing_input: gross - fees,
                gross_output: state.remaining_inventory - next.remaining_inventory,
                amount_out: state.remaining_inventory - next.remaining_inventory,
                fee_definition: *id(42).value(),
                fee_amount: fees,

                fee_bps: 100,

                retained_fee: 0,

                sell_fee_bps: 0,
                pricing_time: Some(1500)
            }
        );
        assert_eq!(checkpoint_events(&response).len(), 1);
        assert!(!next.closure_observed);
    }
}

#[test]
fn weighted_settlement_events_report_both_assets_and_scheduled_closure() {
    use curve_core::events::*;
    let (input, original) = funded();
    for proceeds in [100, 149] {
        let mut state = original.clone();
        state.recorded_collateral = proceeds;
        state.accrued_fees = 2;
        if let lbp_core::PoolMode::Weighted {
            pricing_collateral, ..
        } = &mut state.mode
        {
            *pricing_collateral = proceeds;
        }
        let response = settle(
            &input,
            &state,
            Message::SettleWeighted { creator: id(50) },
            config(100, id(43), false),
            [1000, state.remaining_inventory],
        )
        .unwrap();
        let next = state_of(&response);
        assert!(next.closure_observed);
        assert_eq!(response.validity.timestamps.start(), Some(3_000_000));
        let mut expected = vec![
            SaleEventKindV1::Closed {
                reason: ClosureReasonV1::ScheduledEnd { timestamp: 3000 },
                inclusion_bounds: EventTimeBoundsV1 {
                    at_least: Some(3000),
                    before: None,
                },
            },
            SaleEventKindV1::FeesCollected {
                definition: *id(42).value(),
                treasury: *id(43).value(),
                liability_discharged: 2,
                tokens_transferred: 2,
            },
            SaleEventKindV1::Settlement {
                step: SettlementStepV1::Payout,
                route: Some(SettlementRouteV1::Manual),
                effects: vec![SettlementEffectV1::Payout {
                    definition: *id(41).value(),
                    recipient: *id(50).value(),
                    purpose: PayoutPurposeV1::CreatorLaunch,
                    liability_discharged: state.remaining_inventory,
                    tokens_transferred: state.remaining_inventory,
                }],
            },
            SaleEventKindV1::Settlement {
                step: SettlementStepV1::Payout,
                route: Some(SettlementRouteV1::Manual),
                effects: vec![SettlementEffectV1::Payout {
                    definition: *id(42).value(),
                    recipient: *id(50).value(),
                    purpose: PayoutPurposeV1::CreatorProceeds,
                    liability_discharged: proceeds,
                    tokens_transferred: proceeds,
                }],
            },
        ];
        expected.push(SaleEventKindV1::Settlement {
            step: SettlementStepV1::Completed,
            route: Some(SettlementRouteV1::Manual),
            effects: vec![],
        });
        assert_eq!(
            sale_events(&response)
                .into_iter()
                .map(|e| e.kind)
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(next.remaining_inventory, state.remaining_inventory);
    }
}

#[test]
fn weighted_depletion_waits_for_schedule_and_zero_manual_completion_blocks_hooks() {
    use curve_core::events::*;
    let (input, mut state) = funded();
    state.remaining_inventory = 0;
    state.graduation_adapter = None;
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(100, id(43), false),
        [100, 0],
    )
    .unwrap();
    let next = state_of(&response);
    assert!(matches!(
        &sale_events(&response)[0].kind,
        SaleEventKindV1::Closed {
            reason: ClosureReasonV1::ScheduledEnd { timestamp: 3000 },
            ..
        }
    ));
    assert_eq!(response.validity.timestamps.start(), Some(3_000_000));
    assert_eq!(
        next.settlement_progress,
        lbp_core::SettlementProgress::ManualComplete
    );
    assert_eq!(
        sale_events(&response).last().unwrap().kind,
        SaleEventKindV1::Settlement {
            step: SettlementStepV1::Completed,
            route: Some(SettlementRouteV1::Manual),
            effects: vec![],
        }
    );
    assert_eq!(
        settle(
            &input,
            &next,
            Message::Settle {
                route: lbp_core::SettlementRoute::Graduation,
                creator: id(50),
                adapter: next.graduation_adapter,
            },
            config(100, id(43), false),
            [900, 0],
        )
        .err(),
        Some(lbp_core::Error::InvalidTerms)
    );
    let manual_retry = handle(&ReceiveInput {
        receiver: input.receiver,
        from: Some(Actor::new(next.factory_owner, next.factory_program_id)),
        is_authorized: false,
        pre_state: borsh::to_vec(&next).unwrap().into(),
        message: borsh::to_vec(&Message::SettleWeighted { creator: id(50) }).unwrap(),
    })
    .unwrap();
    assert!(sale_events(&manual_retry).is_empty());
    assert_eq!(state_of(&manual_retry), next);
    state.graduation_adapter = Some(Actor::new(id(70), id(71)));
    let hook = settle(
        &input,
        &state,
        Message::Settle {
            route: lbp_core::SettlementRoute::Graduation,
            creator: id(50),
            adapter: state.graduation_adapter,
        },
        config(100, id(43), false),
        [100, 0],
    )
    .unwrap();
    assert_eq!(sale_events(&hook).len(), sale_events(&response).len() - 1);
    assert_eq!(hook.validity.timestamps.start(), Some(3_000_000));
    let lbp_core::SettlementReply::Applied { receipt } =
        borsh::from_slice(&hook.sync_messages.last().unwrap().message).unwrap()
    else {
        panic!("expected receipt")
    };
    let done = handle(&ReceiveInput {
        receiver: input.receiver,
        from: Some(receipt.factory),
        is_authorized: false,
        pre_state: hook.post_state.unwrap(),
        message: borsh::to_vec(&Message::FinalizeSettlement {
            receipt_hash: receipt.hash(),
        })
        .unwrap(),
    })
    .unwrap();
    assert_eq!(sale_events(&done).len(), 1);
    assert_eq!(
        sale_events(&done)[0].kind,
        SaleEventKindV1::Settlement {
            step: SettlementStepV1::Completed,
            route: Some(SettlementRouteV1::Graduation),
            effects: vec![]
        }
    );
    let completed = state_of(&done);
    let repeated = handle(&ReceiveInput {
        receiver: input.receiver,
        from: Some(Actor::new(
            completed.factory_owner,
            completed.factory_program_id,
        )),
        is_authorized: false,
        pre_state: borsh::to_vec(&completed).unwrap().into(),
        message: borsh::to_vec(&Message::SettleWeighted { creator: id(51) }).unwrap(),
    });
    assert_eq!(repeated.err(), Some(lbp_core::Error::InvalidTerms));
}

#[test]
fn weighted_setup_events_preserve_configured_seed_and_activated_accounting() {
    use curve_core::events::*;
    let (mut input, prepared) = prepared();
    let state = state_of(&prepared);
    let events = sale_events(&prepared);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].engine, PricingEngineV1::Weighted);
    assert_eq!(
        events[0].kind,
        SaleEventKindV1::Prepared {
            owner: *state.factory_owner.value(),
            token0_definition: *id(41).value(),
            token1_definition: *id(42).value(),
            reserve0: 1_000_000,
            reserve1: 100,
            policies: EventPoliciesV1 {
                ending: lbp_core::EndingPolicy::Scheduled,
                pause: lbp_core::PausePolicy::AuthorityControlled,
                sell_fee_bps: 0,
            }
        }
    );
    assert!(!state.closure_observed);
    input.pre_state = prepared.post_state.unwrap();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.message = borsh::to_vec(&Message::ActivateWeighted).unwrap();
    let mut response = handle(&input).unwrap();
    assert!(sale_events(&response).is_empty());
    for bytes in [
        namespace_state(),
        definition(),
        definition(),
        holding(state.factory_owner, id(41), 0),
        holding(input.receiver.account_id, id(41), 1_000_000),
        holding(input.receiver.account_id, id(42), 100),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle(&input).unwrap();
    }
    assert!(sale_events(&response).is_empty());
    assert_eq!(response.sync_messages[0].to, clock_core::clock_actor());
    input.pre_state = response.post_state.unwrap();
    input.from = Some(clock_core::clock_actor());
    input.message = borsh::to_vec(&clock_core::Reply::Timestamp(1_000_000)).unwrap();
    response = handle(&input).unwrap();
    assert_eq!(sale_events(&response).len(), 1);
    assert_eq!(
        sale_events(&response)[0].kind,
        SaleEventKindV1::Activated {
            reserve0: 1_000_000,
            reserve1: 100,
            collateral_seed: 100
        }
    );
    assert!(!state_of(&response).closure_observed);
}

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
