use lbp_core::{CreatorAllocationRelease, Message, WeightedPoolInputs, compute_pool_pda, handle};
use lee_core::account::{AccountId, Actor};
use lee_core::program::{PdaSeed, ReceiveInput};

pub(super) fn weight(percent: u128) -> u128 {
    percent * lbp_core::WEIGHT_TOTAL / 100
}

pub(super) fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}

pub(super) fn weighted_inputs() -> WeightedPoolInputs {
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

pub(super) fn namespace_state() -> Vec<u8> {
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

pub(super) fn config(fee: u16, treasury: AccountId, frozen: bool) -> Vec<u8> {
    let mut config = curve_core::Config::decode(&namespace_state()).unwrap();
    config.protocol_fee_bps = fee;
    config.treasury = treasury;
    config.emergency.is_frozen = frozen;
    borsh::to_vec(&config).unwrap()
}

pub(super) fn reply(
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

pub(super) fn definition() -> Vec<u8> {
    borsh::to_vec(&lez_compat_core::token::TokenState::Definition {
        name: "Token".into(),
        kind: lez_compat_core::token::TokenKind::Fungible,
        supply: 2_000_000,
        metadata: None,
    })
    .unwrap()
}

pub(super) fn holding(owner: AccountId, definition: AccountId, balance: u128) -> Vec<u8> {
    borsh::to_vec(&lez_compat_core::token::TokenState::Holding {
        owner,
        definition,
        kind: lez_compat_core::token::TokenKind::Fungible,
        balance,
    })
    .unwrap()
}

pub(super) fn prepared() -> (ReceiveInput, lee_core::program::Response) {
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

pub(super) fn funded() -> (ReceiveInput, lbp_core::PoolAccount) {
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
pub(super) fn operation_at(
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

pub(super) fn operation(
    input: &ReceiveInput,
    state: &lbp_core::PoolAccount,
    message: Message,
    config: Vec<u8>,
    balances: [u128; 3],
) -> Result<lee_core::program::Response, lbp_core::Error> {
    operation_at(1_500_000, input, state, message, config, balances)
}

pub(super) fn state_of(response: &lee_core::program::Response) -> lbp_core::PoolAccount {
    lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap()
}

pub(super) fn pricing_collateral(state: &lbp_core::PoolAccount) -> u128 {
    let lbp_core::PoolMode::Weighted {
        pricing_collateral, ..
    } = state.mode
    else {
        panic!("expected weighted state");
    };
    pricing_collateral
}

/// Complete settlement or fee collection with fixture replies.
pub(super) fn settle(
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

pub(super) fn checkpoint_events(
    response: &lee_core::program::Response,
) -> Vec<&lee_core::program::ProgramEvent> {
    response
        .events
        .iter()
        .filter(|event| event.selector == [0xae, 0x2f, 0x6d, 0x45, 0x7f, 0xb3, 0x79, 0xcf])
        .collect()
}

pub(super) fn sale_events(
    response: &lee_core::program::Response,
) -> Vec<curve_core::events::SaleEventV1> {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let hash = Impl::hash_bytes(curve_core::events::SaleEventV1::SELECTOR_NAME.as_bytes());
    response
        .events
        .iter()
        .filter(|e| e.selector == hash.as_bytes()[..8])
        .map(|e| borsh::from_slice(&e.data).unwrap())
        .collect()
}
