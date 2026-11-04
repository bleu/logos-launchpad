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
            holding(id(50), id(42), balances[2]),
        ]);
    }
    for bytes in values {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle(&input)?;
    }
    Ok(response)
}
fn exact_in(amount_in: u128) -> Message {
    Message::BuyExactInput {
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
    let message = Message::BuyExactInput {
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
            amount_in: 5,
            min_amount_out: 3,
            now: 100,
        },
        Message::BuyExactOutput {
            amount_out: 3,
            max_amount_in: 4,
            now: 100,
        },
        Message::BuyExactOutput {
            amount_out: 0,
            max_amount_in: 100,
            now: 100,
        },
        Message::BuyExactOutput {
            amount_out: 801,
            max_amount_in: 2_000,
            now: 100,
        },
        Message::BuyExactInput {
            amount_in: 5,
            min_amount_out: 0,
            now: 99,
        },
        Message::BuyExactInput {
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
    let response = operation(
        &input,
        &state,
        Message::BuyExactOutput {
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
    assert!(next.proceeds_settled);
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
    assert!(state_of(&response).proceeds_settled);
    assert_eq!(state_of(&response).remaining_inventory, 800);
}

#[test]
fn expiry_burns_only_unsold_sale_inventory_and_leaves_donations_outside_payouts() {
    let (input, mut state) = funded();
    state.remaining_inventory = 797;
    state.recorded_collateral = 4;
    state.accrued_fees = 1;
    let response = settle(&input, &state, 200, config(100, id(43), false), [22, 814]).unwrap();
    assert_eq!(response.sync_messages.len(), 3);
    let burn = &response.sync_messages[1];
    assert_eq!(
        burn.to,
        Actor::new(
            input.receiver.account_id,
            curve_core::TOKEN_PROGRAM_ACCOUNT_ID
        )
    );
    assert_eq!(
        borsh::from_slice::<lez_compat_core::token::Message>(&burn.message).unwrap(),
        lez_compat_core::token::Message::Burn {
            definition: id(41),
            amount: 797
        }
    );
    assert_eq!(
        burn.pda_seeds,
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
fn settlement_retry_cannot_burn_or_pay_again_even_with_empty_vaults() {
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
        let response = settle(&input, &state, 100, config(100, id(43), false), [0, 0]).unwrap();
        assert!(state_of(&response).proceeds_settled);
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
