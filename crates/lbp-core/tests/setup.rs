use lbp_core::{
    CreatorAllocationRelease, FixedPrice, FixedPricePoolInputs, Message, compute_pool_pda, handle,
};
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, ReceiveInput},
};

fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}
fn prepare() -> ReceiveInput {
    let factory = AccountId::for_public_pda(&id(7), &PdaSeed::new([3; 32]));
    let inputs = FixedPricePoolInputs {
        per_block_token_limit: None,
        buy_policy: curve_core::BuyPolicy::Open,
        pause_policy: lbp_core::PausePolicy::Disabled,
        ending_policy: lbp_core::EndingPolicy::Either,
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
        sale_inventory: 800,
        creator_allocation: 200,
        price: FixedPrice {
            collateral_units: 3,
            token_units: 2,
        },
        start_timestamp: 100,
        end_timestamp: Some(200),
        creator_allocation_release: CreatorAllocationRelease::AfterSetup,
    };
    ReceiveInput {
        receiver: Actor::new(
            compute_pool_pda(id(40), id(8), id(41), id(42), factory),
            id(8),
        ),
        from: Some(Actor::new(factory, id(7))),
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&Message::PrepareFixedPrice { inputs }).unwrap(),
    }
}

#[test]
fn preparation_checks_the_saved_namespace_through_its_actor() {
    let input = prepare();
    let response = handle(&input).unwrap();
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(curve_core::compute_config_pda(id(40), id(6)), id(6))
    );
    assert_eq!(
        borsh::from_slice::<curve_core::Message>(&response.sync_messages[0].message).unwrap(),
        curve_core::Message::ReadNamespace { namespace: id(40) }
    );
}

#[test]
fn setup_accepts_only_the_saved_factory_program_and_pda() {
    for from in [
        None,
        Some(Actor::new(id(99), id(7))),
        Some(Actor::new(prepare().from.unwrap().account_id, id(99))),
    ] {
        let mut input = prepare();
        input.from = from;
        assert!(matches!(handle(&input), Err(lbp_core::Error::Unauthorized)));
    }
}

#[test]
fn preparation_requires_fresh_canonical_pool_and_positive_distinct_terms() {
    for case in 0..8 {
        let mut input = prepare();
        let Message::PrepareFixedPrice { mut inputs } = borsh::from_slice(&input.message).unwrap()
        else {
            unreachable!()
        };
        let expected = match case {
            0 => {
                input.pre_state = vec![1].into();
                lbp_core::Error::AlreadyInitialized
            }
            1 => {
                input.receiver.account_id = id(99);
                lbp_core::Error::WrongAccount
            }
            2 => {
                inputs.sale_inventory = 0;
                lbp_core::Error::InvalidTerms
            }
            3 => {
                inputs.price.collateral_units = 0;
                lbp_core::Error::InvalidTerms
            }
            4 => {
                inputs.price.token_units = 0;
                lbp_core::Error::InvalidTerms
            }
            5 => {
                inputs.end_timestamp = Some(100);
                lbp_core::Error::InvalidTerms
            }
            6 => {
                inputs.collateral_definition_id = inputs.token_definition_id;
                lbp_core::Error::InvalidTerms
            }
            _ => {
                inputs.namespace = AccountId::default();
                lbp_core::Error::InvalidTerms
            }
        };
        input.message = borsh::to_vec(&Message::PrepareFixedPrice { inputs }).unwrap();
        assert!(
            matches!(handle(&input),Err(e) if e==expected),
            "case {case}"
        );
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
#[test]
fn namespace_reply_must_come_from_the_saved_program_and_config_address() {
    let input = prepare();
    let response = handle(&input).unwrap();
    let source = response.sync_messages[0].to;
    for from in [
        Actor::new(id(99), source.program_account_id),
        Actor::new(source.account_id, id(99)),
    ] {
        assert!(matches!(
            handle(&reply(&input, &response, from, namespace_state())),
            Err(lbp_core::Error::Unauthorized)
        ));
    }
    let accepted = handle(&reply(&input, &response, source, namespace_state())).unwrap();
    assert_eq!(
        accepted.sync_messages[0].to,
        Actor::new(id(41), curve_core::TOKEN_PROGRAM_ACCOUNT_ID)
    );
}

fn definition() -> Vec<u8> {
    borsh::to_vec(&lez_compat_core::token::TokenState::Definition {
        name: "Token".into(),
        kind: lez_compat_core::token::TokenKind::Fungible,
        supply: 1000,
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
    let mut input = prepare();
    let mut response = handle(&input).unwrap();
    for bytes in [
        namespace_state(),
        definition(),
        definition(),
        holding(input.from.unwrap().account_id, id(41), 800),
        holding(input.receiver.account_id, id(41), 0),
        holding(input.receiver.account_id, id(42), 0),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle(&input).unwrap();
    }
    (input, response)
}
#[test]
fn preparation_saves_pending_terms_and_initializes_canonical_vaults() {
    let (input, response) = prepared();
    let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(!state.funded);
    assert_eq!(state.recorded_collateral, 0);
    assert_eq!(
        (state.start_timestamp, state.end_timestamp),
        (100, Some(200))
    );
    assert_eq!((state.sale_inventory, state.creator_allocation), (800, 200));
    assert_eq!(
        state.price,
        FixedPrice {
            collateral_units: 3,
            token_units: 2
        }
    );
    assert_eq!(state.factory_owner, prepare().from.unwrap().account_id);
    assert_eq!(
        input.receiver.account_id,
        compute_pool_pda(id(40), id(8), id(41), id(42), state.factory_owner)
    );
    assert!(response.async_messages.is_empty());
}

#[test]
fn preparation_rejects_nonzero_inventory_or_collateral_in_pool_vaults() {
    for slot in [4, 5] {
        for amount in [1, 800, u128::MAX] {
            let mut input = prepare();
            let mut response = handle(&input).unwrap();
            let values = [
                namespace_state(),
                definition(),
                definition(),
                holding(input.from.unwrap().account_id, id(41), 800),
                holding(
                    input.receiver.account_id,
                    id(41),
                    if slot == 4 { amount } else { 0 },
                ),
                holding(
                    input.receiver.account_id,
                    id(42),
                    if slot == 5 { amount } else { 0 },
                ),
            ];
            for (index, bytes) in values.into_iter().enumerate() {
                let source = response.sync_messages.last().unwrap().to;
                input = reply(&input, &response, source, bytes);
                let result = handle(&input);
                if index == slot {
                    assert!(matches!(result, Err(lbp_core::Error::InvalidCustody)));
                    break;
                }
                response = result.unwrap();
            }
        }
    }
}

fn activation(collateral: u128) -> (ReceiveInput, lee_core::program::Response) {
    let (mut input, prepared) = prepared();
    input.pre_state = prepared.post_state.unwrap();
    input.from = prepare().from;
    input.message = borsh::to_vec(&Message::ActivateFixedPrice).unwrap();
    let mut response = handle(&input).unwrap();
    for bytes in [
        namespace_state(),
        definition(),
        definition(),
        holding(input.from.unwrap().account_id, id(41), 0),
        holding(input.receiver.account_id, id(41), 800),
        holding(input.receiver.account_id, id(42), collateral),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle(&input).unwrap();
    }
    (input, response)
}
#[test]
fn activation_funds_exact_inventory_and_keeps_donations_outside_recorded_collateral() {
    for donation in [0, 1, 800, u128::MAX] {
        let (mut input, response) = activation(donation);
        let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert!(state.funded);
        assert_eq!(state.recorded_collateral, 0);
        assert_eq!(state.sale_inventory, 800);
        input.pre_state = response.post_state.unwrap();
        input.from = prepare().from;
        input.message = borsh::to_vec(&Message::ActivateFixedPrice).unwrap();
        assert!(matches!(
            handle(&input),
            Err(lbp_core::Error::AlreadyFunded)
        ));
    }
}

#[test]
fn activation_requires_zero_recorded_collateral_and_exact_credited_inventory() {
    let (mut input, response) = prepared();
    let mut state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    state.recorded_collateral = 1;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.from = prepare().from;
    input.message = borsh::to_vec(&Message::ActivateFixedPrice).unwrap();
    assert!(matches!(
        handle(&input),
        Err(lbp_core::Error::RecordedCollateral)
    ));
    for (source, vault) in [(1, 800), (800, 0), (0, 799), (0, 801), (0, u128::MAX)] {
        let (mut input, response) = prepared();
        input.pre_state = response.post_state.unwrap();
        input.from = prepare().from;
        input.message = borsh::to_vec(&Message::ActivateFixedPrice).unwrap();
        let mut response = handle(&input).unwrap();
        for (index, bytes) in [
            namespace_state(),
            definition(),
            definition(),
            holding(input.from.unwrap().account_id, id(41), source),
            holding(input.receiver.account_id, id(41), vault),
        ]
        .into_iter()
        .enumerate()
        {
            let from = response.sync_messages.last().unwrap().to;
            input = reply(&input, &response, from, bytes);
            let result = handle(&input);
            if index == if source != 0 { 3 } else { 4 } {
                assert!(matches!(result, Err(lbp_core::Error::InvalidCustody)));
                break;
            }
            response = result.unwrap();
        }
    }
}

#[test]
fn setup_rejects_substituted_token_actor_sources_and_invalid_canonical_holdings() {
    for slot in 1..6 {
        let mut input = prepare();
        let mut response = handle(&input).unwrap();
        let values = [
            namespace_state(),
            definition(),
            definition(),
            holding(input.from.unwrap().account_id, id(41), 800),
            holding(input.receiver.account_id, id(41), 0),
            holding(input.receiver.account_id, id(42), 0),
        ];
        for (index, bytes) in values.into_iter().enumerate() {
            let source = response.sync_messages.last().unwrap().to;
            input = reply(&input, &response, source, bytes);
            if index == slot {
                for from in [
                    Actor::new(id(99), source.program_account_id),
                    Actor::new(source.account_id, id(99)),
                ] {
                    let mut wrong = input.clone();
                    wrong.from = Some(from);
                    assert!(matches!(handle(&wrong), Err(lbp_core::Error::Unauthorized)));
                }
                let invalid = if slot < 3 {
                    borsh::to_vec(&lez_compat_core::token::TokenState::Definition {
                        name: "NFT".into(),
                        kind: lez_compat_core::token::TokenKind::NftMaster,
                        supply: 1,
                        metadata: None,
                    })
                    .unwrap()
                } else {
                    holding(id(99), id(99), 0)
                };
                let wrong = reply(&input, &response, source, invalid);
                assert!(handle(&wrong).is_err());
            }
            response = handle(&input).unwrap();
        }
    }
}

fn config(fee: u16, treasury: AccountId, frozen: bool) -> Vec<u8> {
    let mut config = curve_core::Config::decode(&namespace_state()).unwrap();
    config.protocol_fee_bps = fee;
    config.treasury = treasury;
    config.emergency.is_frozen = frozen;
    borsh::to_vec(&config).unwrap()
}
fn funded() -> (ReceiveInput, lbp_core::PoolAccount) {
    let (mut input, response) = activation(0);
    let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    input.from = Some(Actor::new(id(50), id(8)));
    (input, state)
}
fn operation(
    input: &ReceiveInput,
    state: &lbp_core::PoolAccount,
    message: Message,
    config: Vec<u8>,
    balances: [u128; 3],
) -> Result<lee_core::program::Response, lbp_core::Error> {
    operation_in_block(input, state, message, config, balances, 42)
}
fn operation_in_block(
    input: &ReceiveInput,
    state: &lbp_core::PoolAccount,
    message: Message,
    config: Vec<u8>,
    balances: [u128; 3],
    block: u64,
) -> Result<lee_core::program::Response, lbp_core::Error> {
    let mut input = input.clone();
    input.pre_state = borsh::to_vec(state).unwrap().into();
    input.message = borsh::to_vec(&message).unwrap();
    let mut response = handle(&input)?;
    let mut values = vec![
        config,
        holding(input.receiver.account_id, id(42), balances[0]),
    ];
    if !matches!(message, Message::CollectFees) {
        values.extend([
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
        ]);
    }
    for bytes in values {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
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
            message: borsh::to_vec(&clock_core::Reply::BlockId(block)).unwrap(),
        };
        response = handle(&input)?;
    }
    Ok(response)
}
fn exact_in(amount_in: u128) -> Message {
    Message::BuyExactInput {
        membership: None,
        amount_in,
        min_amount_out: 0,
        now: 100,
    }
}
fn state_of(response: &lee_core::program::Response) -> lbp_core::PoolAccount {
    lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap()
}
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
fn actor_message_tags_keep_setup_and_read_state_order() {
    for (message, tag) in [
        (Message::ActivateFixedPrice, 1),
        (Message::ReadState, 2),
        (exact_in(5), 3),
        (
            Message::BuyExactOutput {
                membership: None,
                amount_out: 3,
                max_amount_in: 5,
                now: 100,
            },
            4,
        ),
        (Message::CollectFees, 5),
        (
            Message::Execute {
                pool: id(50),
                instruction: Box::new(exact_in(5)),
            },
            6,
        ),
    ] {
        assert_eq!(borsh::to_vec(&message).unwrap()[0], tag);
    }
}
fn settle(
    input: &ReceiveInput,
    state: &lbp_core::PoolAccount,
    now: u64,
    namespace: Vec<u8>,
    balances: [u128; 2],
) -> Result<lee_core::program::Response, lbp_core::Error> {
    let mut input = input.clone();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(state).unwrap().into();
    input.message = borsh::to_vec(&Message::SettleFixedPrice {
        creator: id(60),
        now,
    })
    .unwrap();
    let mut response = handle(&input)?;
    if response.sync_messages.is_empty() {
        return Ok(response);
    }
    for bytes in [
        namespace,
        holding(input.receiver.account_id, id(42), balances[0]),
        holding(input.receiver.account_id, id(41), balances[1]),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle(&input)?;
    }
    Ok(response)
}

#[test]
fn sellout_settlement_collects_fees_and_pays_all_recorded_creator_proceeds() {
    let (input, mut state) = funded();
    state.remaining_inventory = 0;
    state.recorded_collateral = 1_200;
    state.accrued_fees = 13;
    let response = settle(&input, &state, 100, config(100, id(43), false), [1_230, 17]).unwrap();
    let next = state_of(&response);
    assert!(next.settlement_progress.is_complete());
    assert_eq!((next.recorded_collateral, next.accrued_fees), (0, 0));
    assert_eq!(response.sync_messages.len(), 2);
    for (call, recipient, amount) in [
        (&response.sync_messages[0], id(43), 13),
        (&response.sync_messages[1], id(60), 1_200),
    ] {
        assert_eq!(
            call.to,
            Actor::new(
                input.receiver.account_id,
                curve_core::TOKEN_PROGRAM_ACCOUNT_ID
            )
        );
        assert_eq!(
            borsh::from_slice::<lez_compat_core::token::Message>(&call.message).unwrap(),
            lez_compat_core::token::Message::Transfer {
                definition: id(42),
                recipient,
                amount,
            }
        );
        assert_eq!(
            call.pda_seeds,
            [lbp_core::compute_pool_pda_seed(
                state.namespace,
                id(41),
                id(42),
                state.factory_owner,
            )]
            .into_iter()
            .collect()
        );
    }
}

#[test]
fn expiry_settlement_requires_inclusion_at_the_inclusive_end() {
    let (input, state) = funded();
    let response = settle(&input, &state, 200, config(100, id(43), false), [0, 800]).unwrap();
    assert_eq!(response.validity.timestamps.start(), Some(200_000));
    assert_eq!(response.validity.timestamps.end(), None);
    assert!(state_of(&response).settlement_progress.is_complete());
    assert_eq!(state_of(&response).remaining_inventory, 800);
}

#[test]
fn expiry_pays_unsold_inventory_to_creator_and_excludes_donations() {
    let (input, mut state) = funded();
    state.remaining_inventory = 797;
    state.recorded_collateral = 4;
    state.accrued_fees = 1;
    let response = settle(&input, &state, 200, config(100, id(43), false), [22, 814]).unwrap();
    assert_eq!(response.sync_messages.len(), 3);
    let launch_payout = &response.sync_messages[1];
    assert_eq!(
        launch_payout.to,
        Actor::new(
            input.receiver.account_id,
            curve_core::TOKEN_PROGRAM_ACCOUNT_ID
        )
    );
    assert_eq!(
        borsh::from_slice::<lez_compat_core::token::Message>(&launch_payout.message).unwrap(),
        lez_compat_core::token::Message::Transfer {
            definition: id(41),
            recipient: id(60),
            amount: 797
        }
    );
    assert_eq!(
        launch_payout.pda_seeds,
        [lbp_core::compute_pool_pda_seed(
            state.namespace,
            id(41),
            id(42),
            state.factory_owner
        )]
        .into_iter()
        .collect()
    );
    assert_eq!(
        borsh::from_slice::<lez_compat_core::token::Message>(&response.sync_messages[2].message)
            .unwrap(),
        lez_compat_core::token::Message::Transfer {
            definition: id(42),
            recipient: id(60),
            amount: 4
        }
    );
    assert_eq!(state_of(&response).remaining_inventory, 797);
}

#[test]
fn settlement_retry_cannot_pay_again_even_with_empty_vaults() {
    let (input, mut state) = funded();
    state.remaining_inventory = 797;
    state.recorded_collateral = 4;
    state.accrued_fees = 1;
    let response = settle(&input, &state, 200, config(100, id(43), false), [5, 797]).unwrap();
    let settled = state_of(&response);
    let repeated = settle(&input, &settled, 0, config(100, id(70), true), [0, 0]).unwrap();
    assert_eq!(state_of(&repeated), settled);
    assert!(repeated.sync_messages.is_empty());
}

#[test]
fn settlement_rejects_vaults_that_cannot_cover_all_proceeds_fees_and_unsold_inventory() {
    let (input, mut state) = funded();
    state.remaining_inventory = 797;
    state.recorded_collateral = 4;
    state.accrued_fees = 1;
    for balances in [[4, 797], [5, 796]] {
        assert_eq!(
            settle(&input, &state, 200, config(100, id(43), false), balances).err(),
            Some(lbp_core::Error::Underfunded)
        );
    }
}

#[test]
fn settled_expiry_cannot_reopen_buys_with_a_backdated_timestamp_or_donated_tokens() {
    let (input, state) = funded();
    let response = settle(&input, &state, 200, config(100, id(43), false), [0, 800]).unwrap();
    let settled = state_of(&response);
    assert_eq!(
        operation(
            &input,
            &settled,
            exact_in(5),
            config(100, id(43), false),
            [0, 800, 100]
        )
        .err(),
        Some(lbp_core::Error::Closed)
    );
}

#[test]
fn settlement_is_factory_only_and_cannot_be_forwarded_through_buyer_execute() {
    let (mut input, mut state) = funded();
    state.remaining_inventory = 0;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    let command = Message::SettleFixedPrice {
        creator: id(60),
        now: 100,
    };
    input.message = borsh::to_vec(&command).unwrap();
    for from in [
        None,
        Some(Actor::new(id(60), input.receiver.program_account_id)),
        Some(Actor::new(id(99), state.factory_program_id)),
        Some(Actor::new(state.factory_owner, id(99))),
    ] {
        input.from = from;
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
    }
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.message = borsh::to_vec(&Message::SettleFixedPrice {
        creator: AccountId::default(),
        now: 100,
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
    input.pre_state = vec![].into();
    input.receiver.account_id = id(60);
    input.is_authorized = true;
    input.message = borsh::to_vec(&Message::Execute {
        pool: prepare().receiver.account_id,
        instruction: Box::new(command),
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
}

#[test]
fn prior_fee_collection_preserves_proceeds_and_leaves_collected_self_treasury_fees_in_custody() {
    let (mut input, mut state) = funded();
    state.remaining_inventory = 0;
    state.recorded_collateral = 1_200;
    state.accrued_fees = 13;
    input.from = None;
    for (treasury, balance_after_collection) in
        [(id(70), 1_217), (input.receiver.account_id, 1_230)]
    {
        let collected = operation(
            &input,
            &state,
            Message::CollectFees,
            config(100, treasury, true),
            [1_230, 0, 0],
        )
        .unwrap();
        let collected = state_of(&collected);
        assert_eq!(
            (collected.recorded_collateral, collected.accrued_fees),
            (1_200, 0)
        );
        let response = settle(
            &input,
            &collected,
            100,
            config(9_999, id(71), true),
            [balance_after_collection, 17],
        )
        .unwrap();
        assert_eq!(response.sync_messages.len(), 1);
        assert_eq!(
            borsh::from_slice::<lez_compat_core::token::Message>(
                &response.sync_messages[0].message
            )
            .unwrap(),
            lez_compat_core::token::Message::Transfer {
                definition: id(42),
                recipient: id(60),
                amount: 1_200
            }
        );
        let collected_after_close = operation(
            &input,
            &state_of(&response),
            Message::CollectFees,
            config(100, id(71), true),
            [17, 0, 0],
        )
        .unwrap();
        assert!(collected_after_close.sync_messages.is_empty());
    }
}

#[test]
fn settlement_uses_only_end_or_sellout_and_checks_time_conversion() {
    let (input, mut state) = funded();
    let response = settle(&input, &state, 199, config(100, id(43), false), [0, 800]).unwrap();
    assert_eq!(response.validity.timestamps.start(), Some(200_000));
    state.end_timestamp = None;
    state.ending_policy = lbp_core::EndingPolicy::Depletion;
    assert_eq!(
        settle(
            &input,
            &state,
            u64::MAX,
            config(100, id(43), false),
            [0, 800]
        )
        .err(),
        Some(lbp_core::Error::NotClosed)
    );
    state.end_timestamp = Some(u64::MAX);
    assert_eq!(
        settle(
            &input,
            &state,
            u64::MAX,
            config(100, id(43), false),
            [0, 800]
        )
        .err(),
        Some(lbp_core::Error::InvalidTerms)
    );
    state.remaining_inventory = 0;
    for end in [None, Some(200)] {
        state.end_timestamp = end;
        state.ending_policy = if end.is_some() {
            lbp_core::EndingPolicy::Either
        } else {
            lbp_core::EndingPolicy::Depletion
        };
        let response = settle(&input, &state, 100, config(100, id(43), false), [0, 0]).unwrap();
        assert!(state_of(&response).settlement_progress.is_complete());
        assert!(response.sync_messages.is_empty());
    }
}

#[test]
fn terminal_fees_use_the_current_treasury_without_repricing_proceeds_while_frozen() {
    let (input, mut state) = funded();
    state.remaining_inventory = 0;
    state.recorded_collateral = 1_200;
    state.accrued_fees = 13;
    for treasury in [id(70), input.receiver.account_id] {
        let response = settle(
            &input,
            &state,
            100,
            config(9_999, treasury, true),
            [1_230, 17],
        )
        .unwrap();
        let calls: Vec<lez_compat_core::token::Message> = response
            .sync_messages
            .iter()
            .map(|call| borsh::from_slice(&call.message).unwrap())
            .collect();
        let creator_payout = lez_compat_core::token::Message::Transfer {
            definition: id(42),
            recipient: id(60),
            amount: 1_200,
        };
        let expected = if treasury == input.receiver.account_id {
            vec![creator_payout]
        } else {
            vec![
                lez_compat_core::token::Message::Transfer {
                    definition: id(42),
                    recipient: id(70),
                    amount: 13,
                },
                creator_payout,
            ]
        };
        assert_eq!(calls, expected);
        assert_eq!(
            (
                state_of(&response).recorded_collateral,
                state_of(&response).accrued_fees
            ),
            (0, 0)
        );
    }
}

#[test]
fn settlement_callbacks_require_the_saved_namespace_and_canonical_vaults() {
    let (mut input, state) = funded();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SettleFixedPrice {
        creator: id(60),
        now: 200,
    })
    .unwrap();
    let mut response = handle(&input).unwrap();
    for (step, bytes) in [
        config(100, id(43), false),
        holding(input.receiver.account_id, id(42), 0),
        holding(input.receiver.account_id, id(41), 800),
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
            assert_eq!(handle(&forged).err(), Some(lbp_core::Error::Unauthorized));
        }
        let mut wrong_pool = input.clone();
        wrong_pool.receiver.account_id = id(99);
        assert_eq!(
            handle(&wrong_pool).err(),
            Some(lbp_core::Error::WrongAccount)
        );
        let invalid = if step == 0 {
            let mut namespace = curve_core::Config::decode(&namespace_state()).unwrap();
            namespace.namespace = id(99);
            borsh::to_vec(&namespace).unwrap()
        } else {
            holding(id(99), id(99), 800)
        };
        assert!(handle(&reply(&input, &response, source, invalid)).is_err());
        response = handle(&input).unwrap();
    }
}

#[test]
fn setup_requires_inclusion_before_the_saved_end() {
    for (_, response) in [prepared(), activation(17)] {
        assert_eq!(response.validity.timestamps.start(), None);
        assert_eq!(response.validity.timestamps.end(), Some(200_000));
    }
}

#[test]
fn fixed_price_preparation_keeps_the_bound_buy_policy() {
    let mut input = prepare();
    let Message::PrepareFixedPrice { mut inputs } = borsh::from_slice(&input.message).unwrap()
    else {
        unreachable!()
    };
    let policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *inputs.namespace.value(),
        sale_policy_id: *input.receiver.account_id.value(),
        root: [9; 32],
    });
    inputs.buy_policy = policy.clone();
    input.message = borsh::to_vec(&Message::PrepareFixedPrice { inputs }).unwrap();
    let response = handle(&input).unwrap();
    assert_eq!(
        lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        policy
    );
}

#[test]
fn fixed_price_policy_binding_is_checked_before_setup_queries_and_on_state_reads() {
    for case in 0..3 {
        let mut input = prepare();
        let Message::PrepareFixedPrice { mut inputs } = borsh::from_slice(&input.message).unwrap()
        else {
            unreachable!()
        };
        inputs.buy_policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
            version: if case == 0 { 2 } else { 1 },
            namespace: if case == 1 {
                [99; 32]
            } else {
                *inputs.namespace.value()
            },
            sale_policy_id: if case == 2 {
                [99; 32]
            } else {
                *input.receiver.account_id.value()
            },
            root: [9; 32],
        });
        input.message = borsh::to_vec(&Message::PrepareFixedPrice {
            inputs: inputs.clone(),
        })
        .unwrap();
        assert!(
            matches!(handle(&input), Err(lbp_core::Error::InvalidTerms)),
            "case {case}"
        );
        let (_, mut state) = funded();
        state.buy_policy = inputs.buy_policy;
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.message = borsh::to_vec(&Message::ReadState).unwrap();
        assert!(
            matches!(handle(&input), Err(lbp_core::Error::InvalidTerms)),
            "stored case {case}"
        );
    }
}

#[test]
fn fixed_price_allowlisted_buys_require_membership_before_external_queries() {
    let (mut input, mut state) = funded();
    state.buy_policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *state.namespace.value(),
        sale_policy_id: *input.receiver.account_id.value(),
        root: [9; 32],
    });
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    for message in [
        Message::BuyExactInput {
            membership: None,
            amount_in: 30,
            min_amount_out: 0,
            now: 100,
        },
        Message::BuyExactOutput {
            membership: None,
            amount_out: 10,
            max_amount_in: 30,
            now: 100,
        },
    ] {
        input.message = borsh::to_vec(&message).unwrap();
        assert!(matches!(handle(&input), Err(lbp_core::Error::Unauthorized)));
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
fn fixed_price_membership_can_be_reused_in_both_buy_forms_without_changing_economics() {
    let (input, mut state) = funded();
    let (policy, path) = membership_for(state.namespace, input.receiver.account_id, id(50));
    state.buy_policy = policy.clone();
    for message in [
        Message::BuyExactInput {
            membership: Some(path.clone()),
            amount_in: 30,
            min_amount_out: 19,
            now: 100,
        },
        Message::BuyExactOutput {
            membership: Some(path),
            amount_out: 10,
            max_amount_in: 16,
            now: 100,
        },
    ] {
        let response = operation(
            &input,
            &state,
            message,
            config(100, id(43), false),
            [
                state.recorded_collateral + state.accrued_fees,
                state.remaining_inventory,
                100,
            ],
        )
        .unwrap();
        state = state_of(&response);
        assert_eq!(state.buy_policy, policy);
        assert!(response.sync_messages.iter().any(|call| borsh::from_slice::<lez_compat_core::token::Message>(&call.message).is_ok_and(|message| matches!(message, lez_compat_core::token::Message::Transfer { recipient, .. } if recipient == id(50)))));
    }
    assert_eq!(
        (
            state.remaining_inventory,
            state.recorded_collateral,
            state.accrued_fees
        ),
        (771, 44, 2)
    );
}

#[test]
fn fixed_price_rejects_copied_and_malformed_membership_in_both_buy_forms() {
    let (mut input, mut state) = funded();
    let (policy, path) = membership_for(state.namespace, input.receiver.account_id, id(50));
    for case in 0..7 {
        state.buy_policy = policy.clone();
        let mut evidence = path.clone();
        input.from = Some(Actor::new(id(50), id(8)));
        match case {
            0 => input.from = Some(Actor::new(id(51), id(8))),
            1 => evidence.salt = [99; 32],
            2 => evidence.index = 1 << 17,
            3 => evidence.siblings[4] = [99; 32],
            4 => {
                let curve_core::BuyPolicy::Allowlist(policy) = &mut state.buy_policy else {
                    unreachable!()
                };
                policy.root = [99; 32];
            }
            5 => input.from = Some(Actor::new(id(50), id(99))),
            _ => input.from = Some(input.receiver),
        }
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        for message in [
            Message::BuyExactInput {
                membership: Some(evidence.clone()),
                amount_in: 30,
                min_amount_out: 0,
                now: 100,
            },
            Message::BuyExactOutput {
                membership: Some(evidence.clone()),
                amount_out: 10,
                max_amount_in: 30,
                now: 100,
            },
        ] {
            input.message = borsh::to_vec(&message).unwrap();
            assert_eq!(
                handle(&input).err(),
                Some(lbp_core::Error::Unauthorized),
                "case {case}"
            );
        }
    }
    input.from = Some(Actor::new(id(50), id(8)));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::BuyExactInput {
        membership: Some(path),
        amount_in: 30,
        min_amount_out: 0,
        now: 100,
    })
    .unwrap();
    input.message.pop();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
}

#[test]
fn fixed_price_policy_survives_every_prepare_and_activation_callback() {
    let mut input = prepare();
    let Message::PrepareFixedPrice { mut inputs } = borsh::from_slice(&input.message).unwrap()
    else {
        unreachable!()
    };
    let policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *inputs.namespace.value(),
        sale_policy_id: *input.receiver.account_id.value(),
        root: [9; 32],
    });
    inputs.buy_policy = policy.clone();
    inputs.trade_mode = lbp_core::TradeMode::BuyAndSell;
    inputs.sell_fee_bps = 250;
    inputs.graduation_adapter = Some(Actor::new(id(70), id(71)));
    input.message = borsh::to_vec(&Message::PrepareFixedPrice { inputs }).unwrap();
    let factory = input.from;
    let mut response = handle(&input).unwrap();
    for activate in [false, true] {
        if activate {
            input.pre_state = response.post_state.clone().unwrap();
            input.from = factory;
            input.message = borsh::to_vec(&Message::ActivateFixedPrice).unwrap();
            response = handle(&input).unwrap();
        }
        for bytes in [
            namespace_state(),
            definition(),
            definition(),
            holding(
                factory.unwrap().account_id,
                id(41),
                if activate { 0 } else { 800 },
            ),
            holding(
                input.receiver.account_id,
                id(41),
                if activate { 800 } else { 0 },
            ),
            holding(input.receiver.account_id, id(42), 0),
        ] {
            let source = response.sync_messages.last().unwrap().to;
            input = reply(&input, &response, source, bytes);
            response = handle(&input).unwrap();
            let saved =
                lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
            assert_eq!(saved.buy_policy, policy);
            assert_eq!(saved.trade_mode, lbp_core::TradeMode::BuyAndSell);
            assert_eq!(saved.sell_fee_bps, 250);
            assert_eq!(saved.graduation_adapter, Some(Actor::new(id(70), id(71))));
            assert_eq!(
                saved.settlement_progress,
                lbp_core::SettlementProgress::Unselected
            );
            assert_eq!(saved.ending_policy, lbp_core::EndingPolicy::Either);
        }
        assert_eq!(
            lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap())
                .unwrap()
                .funded,
            activate
        );
    }
}

fn control(
    input: &ReceiveInput,
    state: &lbp_core::PoolAccount,
    paused: bool,
) -> Result<lee_core::program::Response, lbp_core::Error> {
    let mut input = input.clone();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(state).unwrap().into();
    input.message = borsh::to_vec(&Message::SetPaused { paused }).unwrap();
    handle(&input)
}

#[test]
fn fixed_controls_preserve_policy_and_emit_only_changes_before_start() {
    let mut input = prepare();
    let Message::PrepareFixedPrice { mut inputs } = borsh::from_slice(&input.message).unwrap()
    else {
        unreachable!()
    };
    for policy in [
        lbp_core::PausePolicy::Disabled,
        lbp_core::PausePolicy::AuthorityControlled,
    ] {
        inputs.pause_policy = policy;
        input.message = borsh::to_vec(&Message::PrepareFixedPrice {
            inputs: inputs.clone(),
        })
        .unwrap();
        let state = state_of(&handle(&input).unwrap());
        assert_eq!(state.trade_control.policy(), policy);
        assert!(!state.trade_control.is_paused());
    }
    let (input, mut state) = funded();
    state.trade_control = lbp_core::TradeControl::new(lbp_core::PausePolicy::AuthorityControlled);
    let original = state.clone();
    for paused in [false, true, true, false, false] {
        let response = control(&input, &state, paused).unwrap();
        assert_eq!(response.validity.timestamps.start(), None);
        assert_eq!(response.validity.timestamps.end(), Some(200_000));
        assert_eq!(
            response.events.len(),
            usize::from(state.trade_control.is_paused() != paused)
        );
        if state.trade_control.is_paused() != paused {
            assert_eq!(
                sale_events(&response)[0].kind,
                curve_core::events::SaleEventKindV1::PauseChanged { paused }
            );
        }
        assert!(response.sync_messages.is_empty());
        state = state_of(&response);
        assert_eq!(state.trade_control.is_paused(), paused);
        assert_eq!(
            state.trade_control.policy(),
            lbp_core::PausePolicy::AuthorityControlled
        );
        let mut unchanged = state.clone();
        unchanged.trade_control = original.trade_control;
        assert_eq!(unchanged, original);
    }
}

#[test]
fn fixed_control_noops_authenticate_and_reject_disabled_unfunded_and_ended_sales() {
    let (mut input, mut state) = funded();
    for paused in [false, true] {
        assert_eq!(
            control(&input, &state, paused).err(),
            Some(lbp_core::Error::PauseDisabled)
        );
    }
    state.trade_control = lbp_core::TradeControl::new(lbp_core::PausePolicy::AuthorityControlled);
    for paused in [false, true] {
        state.trade_control.set_paused(paused).unwrap();
        input.message = borsh::to_vec(&Message::SetPaused { paused }).unwrap();
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        for from in [
            None,
            Some(Actor::new(id(60), id(8))),
            Some(Actor::new(id(99), state.factory_program_id)),
            Some(Actor::new(state.factory_owner, id(99))),
        ] {
            input.from = from;
            input.is_authorized = true;
            assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
        }
        let mut unfunded = state.clone();
        unfunded.funded = false;
        assert_eq!(
            control(&input, &unfunded, paused).err(),
            Some(lbp_core::Error::PendingFunding)
        );
        for end in [None, Some(200)] {
            let mut sold = state.clone();
            sold.end_timestamp = end;
            sold.ending_policy = if end.is_some() {
                lbp_core::EndingPolicy::Either
            } else {
                lbp_core::EndingPolicy::Depletion
            };
            sold.remaining_inventory = 0;
            assert_eq!(
                control(&input, &sold, paused).err(),
                Some(lbp_core::Error::Closed)
            );
        }
        let mut settled = state.clone();
        settled.settlement_progress = lbp_core::SettlementProgress::ManualComplete;
        assert_eq!(
            control(&input, &settled, paused).err(),
            Some(lbp_core::Error::Closed)
        );
        let mut overflow = state.clone();
        overflow.end_timestamp = Some(u64::MAX);
        assert_eq!(
            control(&input, &overflow, paused).err(),
            Some(lbp_core::Error::InvalidTerms)
        );
        let mut open = state.clone();
        open.end_timestamp = None;
        open.ending_policy = lbp_core::EndingPolicy::Depletion;
        let response = control(&input, &open, paused).unwrap();
        assert_eq!(response.validity.timestamps.end(), None);
    }
    input.pre_state = vec![].into();
    input.receiver.account_id = id(60);
    input.message = borsh::to_vec(&Message::Execute {
        pool: prepare().receiver.account_id,
        instruction: Box::new(Message::SetPaused { paused: true }),
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
}

#[test]
fn fixed_pause_gates_both_buys_and_resume_keeps_namespace_freeze() {
    let (input, mut state) = funded();
    state.trade_control = lbp_core::TradeControl::new(lbp_core::PausePolicy::AuthorityControlled);
    state = state_of(&control(&input, &state, true).unwrap());
    for message in [
        exact_in(5),
        Message::BuyExactOutput {
            membership: None,
            amount_out: 3,
            max_amount_in: 6,
            now: 100,
        },
    ] {
        assert_eq!(
            operation(
                &input,
                &state,
                message.clone(),
                config(100, id(43), false),
                [0, 800, 100]
            )
            .err(),
            Some(lbp_core::Error::Paused)
        );
        let resumed = state_of(&control(&input, &state, false).unwrap());
        assert_eq!(
            operation(
                &input,
                &resumed,
                message.clone(),
                config(100, id(43), true),
                [0, 800, 100]
            )
            .err(),
            Some(lbp_core::Error::Frozen)
        );
        assert!(
            operation(
                &input,
                &resumed,
                message,
                config(100, id(43), false),
                [0, 800, 100]
            )
            .is_ok()
        );
    }
}

#[test]
fn fixed_pause_and_freeze_leave_collection_and_safe_settlement_available() {
    let (input, mut state) = funded();
    state.trade_control = lbp_core::TradeControl::new(lbp_core::PausePolicy::AuthorityControlled);
    state = state_of(&control(&input, &state, true).unwrap());
    state.accrued_fees = 1;
    state.recorded_collateral = 4;
    let collected = operation(
        &input,
        &state,
        Message::CollectFees,
        config(100, id(43), true),
        [5, 800, 0],
    )
    .unwrap();
    assert_eq!(state_of(&collected).accrued_fees, 0);
    let response = settle(&input, &state, 100, config(100, id(43), true), [5, 800]).unwrap();
    assert_eq!(response.validity.timestamps.start(), Some(200_000));
    assert!(state_of(&response).settlement_progress.is_complete());
    assert!(state_of(&response).trade_control.is_paused());
    state.end_timestamp = None;
    state.ending_policy = lbp_core::EndingPolicy::Depletion;
    assert_eq!(
        settle(
            &input,
            &state,
            u64::MAX,
            config(100, id(43), true),
            [5, 800]
        )
        .err(),
        Some(lbp_core::Error::NotClosed)
    );
    state.remaining_inventory = 0;
    let response = settle(&input, &state, 100, config(100, id(43), true), [5, 0]).unwrap();
    assert_eq!(response.validity.timestamps.start(), None);
    assert!(state_of(&response).settlement_progress.is_complete());
}

#[test]
fn fixed_policy_matrix_rejects_unsupported_terms_before_setup_queries() {
    use lbp_core::EndingPolicy;
    for ending in [
        EndingPolicy::Manual,
        EndingPolicy::Scheduled,
        EndingPolicy::Depletion,
        EndingPolicy::Either,
    ] {
        for end in [None, Some(200)] {
            let mut input = prepare();
            let Message::PrepareFixedPrice { mut inputs } =
                borsh::from_slice(&input.message).unwrap()
            else {
                unreachable!()
            };
            inputs.ending_policy = ending;
            inputs.end_timestamp = end;
            input.message = borsh::to_vec(&Message::PrepareFixedPrice { inputs }).unwrap();
            let result = handle(&input);
            if (ending == EndingPolicy::Depletion && end.is_none())
                || (matches!(ending, EndingPolicy::Scheduled | EndingPolicy::Either)
                    && end.is_some())
            {
                let state = state_of(&result.unwrap());
                assert_eq!(state.ending_policy, ending);
                assert_eq!(state.end_timestamp, end);
            } else {
                assert_eq!(result.err(), Some(lbp_core::Error::InvalidTerms));
            }
        }
    }
}

#[test]
fn fixed_preparation_checks_both_timestamp_conversions_before_queries() {
    let last = u64::MAX / 1000;
    for (start, end, valid) in [
        (0, None, true),
        (last, None, true),
        (last - 1, Some(last), true),
        (last + 1, None, false),
        (0, Some(last + 1), false),
        (100, Some(100), false),
        (101, Some(100), false),
    ] {
        let mut input = prepare();
        let Message::PrepareFixedPrice { mut inputs } = borsh::from_slice(&input.message).unwrap()
        else {
            unreachable!()
        };
        inputs.start_timestamp = start;
        inputs.end_timestamp = end;
        inputs.ending_policy = if end.is_some() {
            lbp_core::EndingPolicy::Either
        } else {
            lbp_core::EndingPolicy::Depletion
        };
        input.message = borsh::to_vec(&Message::PrepareFixedPrice { inputs }).unwrap();
        let result = handle(&input);
        if valid {
            assert!(result.is_ok());
        } else {
            assert_eq!(result.err(), Some(lbp_core::Error::InvalidTerms));
        }
    }
}

#[test]
fn malformed_fixed_policies_cannot_bypass_pool_entry_or_setup_replies() {
    for case in 0..5 {
        let (mut input, state) = funded();
        let mut malformed = state.clone();
        match case {
            0 => malformed.ending_policy = lbp_core::EndingPolicy::Manual,
            1 => malformed.ending_policy = lbp_core::EndingPolicy::Depletion,
            2 => malformed.ending_policy = lbp_core::EndingPolicy::Manual,
            3 => malformed.start_timestamp = u64::MAX,
            _ => malformed.end_timestamp = Some(100),
        }
        let expected = if case == 0 || case == 2 {
            lbp_core::Error::InvalidState
        } else {
            lbp_core::Error::InvalidTerms
        };
        input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
        input.pre_state = borsh::to_vec(&malformed).unwrap().into();
        for message in [
            Message::ReadState,
            Message::ActivateFixedPrice,
            exact_in(5),
            Message::BuyExactOutput {
                membership: None,
                amount_out: 3,
                max_amount_in: 6,
                now: 100,
            },
            Message::SetPaused { paused: false },
            Message::SettleFixedPrice {
                creator: id(60),
                now: 200,
            },
            Message::CollectFees,
        ] {
            input.message = borsh::to_vec(&message).unwrap();
            assert_eq!(handle(&input).err(), Some(expected));
        }
        let setup = prepare();
        let response = handle(&setup).unwrap();
        let mut pending = state_of(&response);
        pending.ending_policy = lbp_core::EndingPolicy::Manual;

        let mut callback = reply(
            &setup,
            &response,
            response.sync_messages[0].to,
            namespace_state(),
        );
        callback.pre_state = borsh::to_vec(&pending).unwrap().into();
        assert_eq!(handle(&callback).err(), Some(lbp_core::Error::InvalidState));
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

#[test]
fn fixed_inventory_payout_keeps_expiry_evidence_after_settlement() {
    let (input, state) = funded();
    let response = settle(&input, &state, 0, config(100, id(43), true), [0, 817]).unwrap();
    let next = state_of(&response);
    assert_eq!(next.remaining_inventory, 800);
    assert_eq!(next.close_time_bound().unwrap(), Some(200));
    assert_eq!(
        next.ending_policy
            .settlement(next.end_timestamp, next.remaining_inventory)
            .unwrap()
            .reason(),
        sale_policy::EndingReason::ScheduledEnd
    );
}

#[test]
fn fixed_selected_ending_separates_empty_inventory_from_scheduled_close() {
    let (input, original) = funded();
    for ending in [
        lbp_core::EndingPolicy::Scheduled,
        lbp_core::EndingPolicy::Depletion,
        lbp_core::EndingPolicy::Either,
    ] {
        let mut state = original.clone();
        state.ending_policy = ending;
        state.end_timestamp = if ending == lbp_core::EndingPolicy::Depletion {
            None
        } else {
            Some(200)
        };
        state.remaining_inventory = 0;
        let response = settle(&input, &state, 100, config(100, id(43), false), [0, 0]).unwrap();
        let expected = if ending == lbp_core::EndingPolicy::Scheduled {
            Some(200_000)
        } else {
            None
        };
        assert_eq!(response.validity.timestamps.start(), expected);
        assert_eq!(
            state_of(&response).close_time_bound().unwrap(),
            expected.map(|time| time / 1000)
        );
        assert_eq!(
            operation(
                &input,
                &state,
                exact_in(5),
                config(100, id(43), false),
                [0, 0, 100]
            )
            .err(),
            Some(lbp_core::Error::Closed)
        );
        state.remaining_inventory = 1;
        let close = state.close_time_bound();
        if ending == lbp_core::EndingPolicy::Depletion {
            assert_eq!(close.err(), Some(lbp_core::Error::NotClosed));
        } else {
            assert_eq!(close.unwrap(), Some(200));
        }
    }
}

#[test]
fn fixed_atomic_settlement_records_accounted_balances_once() {
    use lbp_core::SettlementProgress;
    use lez_compat_core::token::Message as TokenMessage;
    let (input, original) = funded();
    for (proceeds, fees, inventory) in [(0, 0, 800), (0, 1, 800), (4, 1, 797), (4, 0, 0), (0, 0, 0)]
    {
        let mut state = original.clone();
        state.recorded_collateral = proceeds;
        state.accrued_fees = fees;
        state.remaining_inventory = inventory;
        let response = settle(
            &input,
            &state,
            0,
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
        assert_eq!(
            response.validity.timestamps.start(),
            if inventory == 0 { None } else { Some(200_000) }
        );
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
                recipient: id(60),
                amount: inventory,
            });
        }
        if proceeds != 0 {
            calls.push(TokenMessage::Transfer {
                definition: id(42),
                recipient: id(60),
                amount: proceeds,
            });
        }
        assert_eq!(
            response
                .sync_messages
                .iter()
                .map(|call| borsh::from_slice::<TokenMessage>(&call.message).unwrap())
                .collect::<Vec<_>>(),
            calls
        );
        let retry = settle(&input, &expected, 0, config(0, id(70), false), [0, 0]).unwrap();
        assert_eq!(retry.post_state, response.post_state);
        assert!(retry.sync_messages.is_empty());
        assert!(retry.events.is_empty());
    }
}

#[test]
fn fixed_completed_retries_still_check_factory_mode_and_recipient() {
    use lbp_core::SettlementProgress;
    let (mut input, mut state) = funded();
    for progress in [
        SettlementProgress::ManualComplete,
        SettlementProgress::Graduated,
    ] {
        state.settlement_progress = progress;
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.message = borsh::to_vec(&Message::SettleFixedPrice {
            creator: id(60),
            now: 0,
        })
        .unwrap();
        for from in [
            None,
            Some(Actor::new(id(60), id(8))),
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
        input.message = borsh::to_vec(&Message::SettleFixedPrice {
            creator: AccountId::default(),
            now: 0,
        })
        .unwrap();
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
        input.message = borsh::to_vec(&Message::SettleWeighted { creator: id(60) }).unwrap();
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
    }
}

#[test]
fn fixed_completed_progress_blocks_controls_and_both_buy_forms() {
    use lbp_core::SettlementProgress;
    let (input, mut state) = funded();
    state.trade_control = lbp_core::TradeControl::new(lbp_core::PausePolicy::AuthorityControlled);
    for progress in [
        SettlementProgress::ManualComplete,
        SettlementProgress::Graduated,
    ] {
        state.settlement_progress = progress;
        assert_eq!(
            control(&input, &state, false).err(),
            Some(lbp_core::Error::Closed)
        );
        for message in [
            exact_in(5),
            Message::BuyExactOutput {
                membership: None,
                amount_out: 3,
                max_amount_in: 6,
                now: 100,
            },
        ] {
            assert_eq!(
                operation(
                    &input,
                    &state,
                    message,
                    config(100, id(43), false),
                    [0, 800, 100]
                )
                .err(),
                Some(lbp_core::Error::Closed)
            );
        }
    }
}

#[test]
fn receipt_settlement_requires_saved_namespace_and_canonical_vault_replies() {
    let (mut input, mut state) = funded();
    state.graduation_adapter = Some(Actor::new(id(70), id(71)));
    state.recorded_collateral = 49;
    state.accrued_fees = 2;
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::Settle {
        route: lbp_core::SettlementRoute::Graduation,
        creator: id(60),
        adapter: state.graduation_adapter,
    })
    .unwrap();
    let mut response = handle(&input).unwrap();
    for (step, bytes) in [
        config(100, id(43), false),
        holding(input.receiver.account_id, id(42), 1000),
        holding(input.receiver.account_id, id(41), 900),
    ]
    .into_iter()
    .enumerate()
    {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        for from in [
            None,
            Some(Actor::new(id(99), source.program_account_id)),
            Some(Actor::new(source.account_id, id(99))),
        ] {
            let mut wrong = input.clone();
            wrong.from = from;
            assert_eq!(handle(&wrong).err(), Some(lbp_core::Error::Unauthorized));
        }
        if step > 0 {
            for (owner, definition) in [
                (id(99), if step == 1 { id(42) } else { id(41) }),
                (input.receiver.account_id, id(99)),
            ] {
                let wrong = reply(&input, &response, source, holding(owner, definition, 1000));
                assert_eq!(
                    handle(&wrong).err(),
                    Some(lbp_core::Error::InvalidDefinition)
                );
            }
        }
        response = handle(&input).unwrap();
    }
    assert!(lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).is_err());
    let lbp_core::SettlementReply::Applied { receipt } =
        borsh::from_slice(&response.sync_messages.last().unwrap().message).unwrap()
    else {
        panic!("expected receipt")
    };
    assert_eq!(receipt.eligible.collateral, 49);
    assert_eq!(receipt.pool, input.receiver);
    assert_eq!(receipt.adapter, state.graduation_adapter);
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
fn fixed_setup_events_describe_saved_policies_and_reserves() {
    use curve_core::events::*;
    let first = handle(&prepare()).unwrap();
    assert!(sale_events(&first).is_empty());
    let (input, response) = prepared();
    let events = sale_events(&response);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].version, 1);
    assert_eq!(events[0].namespace, *id(40).value());
    assert_eq!(events[0].engine, PricingEngineV1::FixedPrice);
    assert_eq!(
        events[0].subject,
        EventSubjectV1::Pool {
            pool: *input.receiver.account_id.value()
        }
    );
    assert_eq!(
        events[0].kind,
        SaleEventKindV1::Prepared {
            owner: *prepare().from.unwrap().account_id.value(),
            token0_definition: *id(41).value(),
            token1_definition: *id(42).value(),
            reserve0: 800,
            reserve1: 0,
            policies: EventPoliciesV1 {
                ending: lbp_core::EndingPolicy::Either,
                pause: lbp_core::PausePolicy::Disabled,
                sell_fee_bps: 0,
            }
        }
    );
    let (_, response) = activation(0);
    assert_eq!(
        sale_events(&response)[0].kind,
        SaleEventKindV1::Activated {
            reserve0: 800,
            reserve1: 0,
            collateral_seed: 0
        }
    );
    assert!(!state_of(&response).closure_observed);
}

#[test]
fn fixed_trade_events_match_both_transfer_forms_and_saved_fee_accounting() {
    use curve_core::events::*;
    let (input, state) = funded();
    for (message, form) in [
        (exact_in(5), TradeFormV1::ExactInput),
        (
            Message::BuyExactOutput {
                membership: None,
                amount_out: 3,
                max_amount_in: 6,
                now: 100,
            },
            TradeFormV1::ExactOutput,
        ),
    ] {
        let response = operation(
            &input,
            &state,
            message,
            config(100, id(43), false),
            [0, 800, 1000],
        )
        .unwrap();
        let next = state_of(&response);
        let events = sale_events(&response);
        assert_eq!(events.len(), 1);
        let gross = next.recorded_collateral + next.accrued_fees;
        assert_eq!(
            events[0].kind,
            SaleEventKindV1::Trade {
                trader: *id(50).value(),
                form,
                token_in: *id(42).value(),
                token_out: *id(41).value(),
                amount_in: gross,
                pricing_input: next.recorded_collateral,
                gross_output: 800 - next.remaining_inventory,
                amount_out: 800 - next.remaining_inventory,
                fee_definition: *id(42).value(),
                fee_amount: next.accrued_fees,

                fee_bps: 100,

                retained_fee: 0,

                sell_fee_bps: 0,
                pricing_time: None
            }
        );
    }
}

#[test]
fn fixed_sellout_events_follow_selected_ending_without_fabricated_depletion() {
    use curve_core::events::*;
    let (input, original) = funded();
    for ending in [
        lbp_core::EndingPolicy::Scheduled,
        lbp_core::EndingPolicy::Depletion,
        lbp_core::EndingPolicy::Either,
    ] {
        let mut state = original.clone();
        state.ending_policy = ending;
        state.remaining_inventory = 2;
        if ending == lbp_core::EndingPolicy::Depletion {
            state.end_timestamp = None;
        }
        let response = operation(
            &input,
            &state,
            exact_in(5),
            config(100, id(43), false),
            [0, 2, 1000],
        )
        .unwrap();
        let next = state_of(&response);
        let depleted = ending != lbp_core::EndingPolicy::Scheduled;
        assert_eq!(next.closure_observed, depleted);
        let events = sale_events(&response);
        assert_eq!(events.len(), 1 + usize::from(depleted));
        assert!(matches!(&events[0].kind, SaleEventKindV1::Trade { .. }));
        if depleted {
            assert_eq!(
                events[1].kind,
                SaleEventKindV1::Closed {
                    reason: ClosureReasonV1::Depleted {
                        definition: *id(41).value()
                    },
                    inclusion_bounds: EventTimeBoundsV1 {
                        at_least: None,
                        before: None
                    }
                }
            );
        }
        let settled = settle(&input, &next, 0, config(100, id(43), false), [5, 0]).unwrap();
        let close_events: Vec<_> = sale_events(&settled)
            .into_iter()
            .filter(|e| matches!(e.kind, SaleEventKindV1::Closed { .. }))
            .collect();
        assert_eq!(close_events.len(), usize::from(!depleted));
        assert_eq!(
            settled.validity.timestamps.start(),
            if depleted { None } else { Some(200_000) }
        );
        if !depleted {
            assert!(matches!(
                close_events[0].kind,
                SaleEventKindV1::Closed {
                    reason: ClosureReasonV1::ScheduledEnd { timestamp: 200 },
                    ..
                }
            ));
        }
    }
}

#[test]
fn fee_events_distinguish_liability_from_self_recipient_token_movement() {
    use curve_core::events::*;
    let (input, mut state) = funded();
    state.accrued_fees = 3;
    for treasury in [id(43), input.receiver.account_id] {
        let response = operation(
            &input,
            &state,
            Message::CollectFees,
            config(100, treasury, false),
            [20, 800, 0],
        )
        .unwrap();
        assert_eq!(
            sale_events(&response)[0].kind,
            SaleEventKindV1::FeesCollected {
                definition: *id(42).value(),
                treasury: *treasury.value(),
                liability_discharged: 3,
                tokens_transferred: if treasury == input.receiver.account_id {
                    0
                } else {
                    3
                }
            }
        );
        let next = state_of(&response);
        let repeated = operation(
            &input,
            &next,
            Message::CollectFees,
            config(100, treasury, false),
            [17, 800, 0],
        )
        .unwrap();
        assert!(sale_events(&repeated).is_empty());
    }
}

#[test]
fn legacy_fixed_settlement_events_preserve_end_bound_and_inventory_record() {
    use curve_core::events::*;
    let (input, mut state) = funded();
    state.recorded_collateral = 49;
    state.accrued_fees = 2;
    let response = settle(&input, &state, 0, config(100, id(43), true), [1000, 900]).unwrap();
    let next = state_of(&response);
    assert!(next.closure_observed);
    assert_eq!(next.remaining_inventory, 800);
    assert_eq!(response.validity.timestamps.start(), Some(200_000));
    let kinds: Vec<_> = sale_events(&response).into_iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            SaleEventKindV1::Closed {
                reason: ClosureReasonV1::ScheduledEnd { timestamp: 200 },
                inclusion_bounds: EventTimeBoundsV1 {
                    at_least: Some(200),
                    before: None
                }
            },
            SaleEventKindV1::FeesCollected {
                definition: *id(42).value(),
                treasury: *id(43).value(),
                liability_discharged: 2,
                tokens_transferred: 2
            },
            SaleEventKindV1::Settlement {
                step: SettlementStepV1::Payout,
                route: Some(SettlementRouteV1::Manual),
                effects: vec![SettlementEffectV1::Payout {
                    definition: *id(41).value(),
                    recipient: *id(60).value(),
                    purpose: PayoutPurposeV1::CreatorLaunch,
                    liability_discharged: 800,
                    tokens_transferred: 800
                }]
            },
            SaleEventKindV1::Settlement {
                step: SettlementStepV1::Payout,
                route: Some(SettlementRouteV1::Manual),
                effects: vec![SettlementEffectV1::Payout {
                    definition: *id(42).value(),
                    recipient: *id(60).value(),
                    purpose: PayoutPurposeV1::CreatorProceeds,
                    liability_discharged: 49,
                    tokens_transferred: 49
                }]
            },
            SaleEventKindV1::Settlement {
                step: SettlementStepV1::Completed,
                route: Some(SettlementRouteV1::Manual),
                effects: vec![]
            },
        ]
    );
    let repeated = settle(&input, &next, 0, config(100, id(43), true), [0, 0]).unwrap();
    assert!(sale_events(&repeated).is_empty());
}

#[test]
fn guarded_settlement_emits_completion_only_after_exact_factory_finalization() {
    use curve_core::events::*;
    let (mut input, mut state) = funded();
    state.recorded_collateral = 49;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.message = borsh::to_vec(&Message::Settle {
        route: lbp_core::SettlementRoute::Manual,
        creator: id(60),
        adapter: None,
    })
    .unwrap();
    let mut response = handle(&input).unwrap();
    assert!(sale_events(&response).is_empty());
    for bytes in [
        config(100, id(43), false),
        holding(input.receiver.account_id, id(42), 1000),
        holding(input.receiver.account_id, id(41), 900),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle(&input).unwrap();
    }
    assert!(!sale_events(&response).iter().any(|e| matches!(
        &e.kind,
        SaleEventKindV1::Settlement {
            step: SettlementStepV1::Completed,
            ..
        }
    )));
    let lbp_core::SettlementReply::Applied { receipt } =
        borsh::from_slice(&response.sync_messages.last().unwrap().message).unwrap()
    else {
        panic!("expected receipt")
    };
    input.pre_state = response.post_state.unwrap();
    input.from = Some(receipt.factory);
    input.message = borsh::to_vec(&Message::FinalizeSettlement {
        receipt_hash: [0; 32],
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidState));
    input.message = borsh::to_vec(&Message::FinalizeSettlement {
        receipt_hash: receipt.hash(),
    })
    .unwrap();
    let done = handle(&input).unwrap();
    assert_eq!(sale_events(&done).len(), 1);
    assert_eq!(
        sale_events(&done)[0].kind,
        SaleEventKindV1::Settlement {
            step: SettlementStepV1::Completed,
            route: Some(SettlementRouteV1::Manual),
            effects: vec![]
        }
    );
    assert!(state_of(&done).closure_observed);
}

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

#[test]
fn fixed_pool_preparation_validates_saved_graduation_percentages() {
    for has_adapter in [false, true] {
        for policy in [
            lbp_core::GraduationPolicy::default(),
            lbp_core::GraduationPolicy {
                launch_bps: 10_000,
                collateral_bps: 0,
            },
            lbp_core::GraduationPolicy {
                launch_bps: 0,
                collateral_bps: 10_000,
            },
            lbp_core::GraduationPolicy {
                launch_bps: 10_001,
                collateral_bps: 0,
            },
            lbp_core::GraduationPolicy {
                launch_bps: 0,
                collateral_bps: 10_001,
            },
        ] {
            let mut input = prepare();
            let Message::PrepareFixedPrice { mut inputs } =
                borsh::from_slice(&input.message).unwrap()
            else {
                unreachable!()
            };
            inputs.graduation_adapter = has_adapter.then(|| Actor::new(id(70), id(71)));
            inputs.graduation_policy = policy;
            input.message = borsh::to_vec(&Message::PrepareFixedPrice { inputs }).unwrap();
            let result = handle(&input);
            if policy.validate(has_adapter).is_err() {
                assert_eq!(result.err(), Some(lbp_core::Error::InvalidTerms));
            } else {
                assert_eq!(state_of(&result.unwrap()).graduation_policy, policy);
            }
        }
    }
}
