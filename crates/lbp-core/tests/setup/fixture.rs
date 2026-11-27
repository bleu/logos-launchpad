use lbp_core::{
    CreatorAllocationRelease, FixedPrice, FixedPricePoolInputs, Message, compute_pool_pda, handle,
};
use lee_core::account::{AccountId, Actor};
use lee_core::program::{PdaSeed, ReceiveInput};

pub(super) fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}

pub(super) fn prepare() -> ReceiveInput {
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
        supply: 1000,
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

pub(super) fn activation(collateral: u128) -> (ReceiveInput, lee_core::program::Response) {
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

pub(super) fn config(fee: u16, treasury: AccountId, frozen: bool) -> Vec<u8> {
    let mut config = curve_core::Config::decode(&namespace_state()).unwrap();
    config.protocol_fee_bps = fee;
    config.treasury = treasury;
    config.emergency.is_frozen = frozen;
    borsh::to_vec(&config).unwrap()
}

pub(super) fn funded() -> (ReceiveInput, lbp_core::PoolAccount) {
    let (mut input, response) = activation(0);
    let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    input.from = Some(Actor::new(id(50), id(8)));
    (input, state)
}

pub(super) fn operation(
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

pub(super) fn exact_in(amount_in: u128) -> Message {
    Message::BuyExactInput {
        membership: None,
        amount_in,
        min_amount_out: 0,
        now: 100,
    }
}

pub(super) fn state_of(response: &lee_core::program::Response) -> lbp_core::PoolAccount {
    lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap()
}

pub(super) fn settle(
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

pub(super) fn control(
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
