use super::*;

pub(super) fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}

pub(super) fn creation() -> ReceiveInput {
    ReceiveInput {
        receiver: Actor::new(compute_factory_pda(id(40), id(7), [3; 32]), id(7)),
        from: None,
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&Message::CreateFactoryPool {
            buy_policy: curve_core::BuyPolicy::Open,
            pause_policy: curve_core::PausePolicy::Disabled,
            ending_policy: factory_core::EndingPolicy::Either,
            trade_mode: 0,
            sell_fee_bps: 0,
            per_block_token_limit: None,
            graduation_hook_program_id: None,
            graduation_hook_configuration: vec![],
            namespace: id(40),
            launch_salt: [3; 32],
            name: "Launch".into(),
            uri: "uri".into(),
            graduation_reserve: 0,
            sale_reserve: 800,
            graduation_policy: GraduationPolicy {
                launch_bps: 0,
                collateral_bps: 0,
            },
            creator_allocation: 100,
            virtual_token_reserve: 1000,
            virtual_collateral_reserve: 100,
            end_timestamp: Some(400),
            curve_program_id: id(6),
            collateral_definition_id: id(41),
            creator: id(42),
            now: 100,
        })
        .unwrap(),
    }
}

pub(super) fn reply(input: &ReceiveInput, response: &Response, bytes: Vec<u8>) -> ReceiveInput {
    ReceiveInput {
        receiver: input.receiver,
        from: Some(response.sync_messages.last().unwrap().to),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&StateReply {
            state: bytes.into(),
        })
        .unwrap(),
    }
}

pub(super) fn config() -> Vec<u8> {
    curve_core::handle(&ReceiveInput {
        receiver: Actor::new(curve_core::compute_config_pda(id(40), id(6)), id(6)),
        from: Some(Actor::new(id(40), id(6))),
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&curve_core::Message::InitializeNamespace {
            namespace: id(40),
            protocol_fee_bps: 100,
            treasury: id(44),
        })
        .unwrap(),
    })
    .unwrap()
    .post_state
    .unwrap()
    .to_vec()
}

pub(super) fn definition() -> Vec<u8> {
    borsh::to_vec(&TokenState::Definition {
        name: "Collateral".into(),
        kind: TokenKind::Fungible,
        supply: 1000000,
        metadata: None,
    })
    .unwrap()
}

pub(super) fn proof(input: &ReceiveInput, response: &Response, creator: AccountId) -> ReceiveInput {
    ReceiveInput {
        receiver: input.receiver,
        from: Some(Actor::new(
            compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
            AUTHORITY_PROGRAM_ACCOUNT_ID,
        )),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&authority::Authorized {
            role: compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
            authority: creator,
        })
        .unwrap(),
    }
}

pub(super) fn resolved(
    input: &ReceiveInput,
    response: &Response,
    creator: AccountId,
) -> ReceiveInput {
    let role = compute_curve_creator_role_pda(id(40), id(7), [3; 32]);
    let call = response.sync_messages.last().unwrap();
    assert_eq!(call.to, Actor::new(role, AUTHORITY_PROGRAM_ACCOUNT_ID));
    assert_eq!(
        borsh::from_slice::<authority::Message>(&call.message).unwrap(),
        authority::Message::Resolve
    );
    ReceiveInput {
        receiver: input.receiver,
        from: Some(call.to),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&authority::CurrentAuthority {
            role,
            authority: creator,
        })
        .unwrap(),
    }
}

pub(super) fn minted() -> (ReceiveInput, Response, Creation) {
    minted_with_policy(curve_core::BuyPolicy::Open)
}

pub(super) fn minted_with_policy(
    policy: curve_core::BuyPolicy,
) -> (ReceiveInput, Response, Creation) {
    minted_with_terms(policy, false)
}

fn minted_with_terms(
    policy: curve_core::BuyPolicy,
    hook: bool,
) -> (ReceiveInput, Response, Creation) {
    let mut input = creation();
    let mut message: Message = borsh::from_slice(&input.message).unwrap();
    if let Message::CreateFactoryPool {
        buy_policy,
        graduation_hook_program_id,
        graduation_hook_configuration,
        graduation_policy,
        ..
    } = &mut message
    {
        *buy_policy = policy;
        if hook {
            *graduation_hook_program_id = Some(id(9));
            *graduation_hook_configuration = b"route".to_vec();
            *graduation_policy = GraduationPolicy {
                launch_bps: 2500,
                collateral_bps: 3333,
            };
        }
    }
    input.message = borsh::to_vec(&message).unwrap();
    mint_from(&input)
}

pub(super) fn mint_from(input: &ReceiveInput) -> (ReceiveInput, Response, Creation) {
    let response = handle(input).unwrap();
    let setup = Creation::new(input, &response);
    let input = setup.namespace_reply(&response, config());
    let response = handle(&input).unwrap();
    let input = reply(&input, &response, definition());
    let response = handle(&input).unwrap();
    (input, response, setup)
}

pub(super) fn holding(owner: AccountId, definition: AccountId, balance: u128) -> Vec<u8> {
    borsh::to_vec(&TokenState::Holding {
        owner,
        definition,
        kind: TokenKind::Fungible,
        balance,
    })
    .unwrap()
}

fn launch_definition(state: &FactoryState) -> Vec<u8> {
    borsh::to_vec(&TokenState::Definition {
        name: state.name.clone(),
        kind: TokenKind::Fungible,
        supply: state.total_supply,
        metadata: Some(token::Metadata {
            standard: token::MetadataStandard::Simple,
            uri: state.uri.clone(),
            creators: String::new(),
        }),
    })
    .unwrap()
}

pub(super) fn continued(
    setup: &Creation,
    previous: &Response,
    source: u128,
    inventory: u128,
    escrow: u128,
) -> (ReceiveInput, Response) {
    let state = FactoryState::decode(previous.post_state.as_ref().unwrap()).unwrap();
    let stage = match state.creation_stage {
        CreationStage::Minted => LbpCreationStage::Minted,
        CreationStage::Allocated => LbpCreationStage::Allocated,
        CreationStage::PoolPrepared => LbpCreationStage::PoolPrepared,
        CreationStage::Active => unreachable!(),
    };
    let mut input = setup.continuation(previous, stage);
    let mut response = handle(&input).unwrap();
    input = proof(&input, &response, id(42));
    response = handle(&input).unwrap();
    for bytes in [
        launch_definition(&state),
        holding(
            compute_mint_pda(id(40), id(7), [3; 32]),
            state.token_definition_id,
            source,
        ),
        holding(
            input.receiver.account_id,
            state.token_definition_id,
            inventory,
        ),
        holding(state.creator_escrow_id, state.token_definition_id, escrow),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    (input, response)
}

pub(super) fn pool_state(
    state: &FactoryState,
    owner: Actor,
    funded: bool,
) -> curve_core::PoolAccount {
    curve_core::PoolAccount {
        buy_policy: state.buy_policy.clone(),
        trade_control: curve_core::TradeControl::new(state.pause_policy),
        allowed_input_token: match state.trade_mode {
            TradeMode::BuyOnly => Some(pool::TokenSide::Token1),
            TradeMode::BuyAndSell => None,
        },
        funded,
        closure_observed: false,
        block_allocation: curve_core::BlockAllocation::new(state.per_block_token_limit).unwrap(),
        namespace: state.namespace,
        token0_definition_id: state.token_definition_id,
        token1_definition_id: state.collateral_definition_id,
        owner: owner.account_id,
        owner_program: Some((
            owner.program_account_id,
            *compute_factory_seed(state.namespace, state.launch_salt).as_bytes(),
        )),
        pool: pool::Pool::create(pool::PoolCreateParams {
            token0_amount: 800,
            token1_amount: 0,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            ending_policy: state.ending_policy,
            sell_fee_bps: state.sell_fee_bps,
            close_timestamp: state.end_timestamp,
            close_on_depletion: match state.ending_policy {
                EndingPolicy::Scheduled => None,
                _ => Some(pool::TokenSide::Token0),
            },
        })
        .unwrap(),
    }
}

pub(super) fn active() -> (ReceiveInput, Response) {
    active_for_route(false)
}

pub(super) fn active_for_route(hook: bool) -> (ReceiveInput, Response) {
    let (_, mint, setup) = minted_with_terms(curve_core::BuyPolicy::Open, hook);
    let (_, allocated) = continued(&setup, &mint, 900, 0, 0);
    let (_, prepared) = continued(&setup, &allocated, 0, 800, 100);
    let (mut input, mut response) = continued(&setup, &prepared, 0, 800, 100);
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    for bytes in [
        borsh::to_vec(&pool_state(&state, input.receiver, false)).unwrap(),
        holding(state.pool_id, state.token_definition_id, 0),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    (input, response)
}

pub(super) fn settlement(
    input: &ReceiveInput,
    previous: &Response,
    pool: curve_core::PoolAccount,
    tokens: u128,
    collateral: u128,
    hook: bool,
) -> (ReceiveInput, Response) {
    settlement_as(input, previous, pool, tokens, collateral, hook, id(42))
}

pub(super) fn settlement_as(
    input: &ReceiveInput,
    previous: &Response,
    pool: curve_core::PoolAccount,
    tokens: u128,
    collateral: u128,
    hook: bool,
    creator: AccountId,
) -> (ReceiveInput, Response) {
    let state = FactoryState::decode(previous.post_state.as_ref().unwrap()).unwrap();
    let mut input = input.clone();
    input.pre_state = previous.post_state.clone().unwrap();
    input.from = None;
    input.message = borsh::to_vec(&if hook {
        Message::ExecuteGraduationHook {
            creator_hint: Some(creator),
        }
    } else {
        Message::WithdrawFactoryProceeds { creator, now: 400 }
    })
    .unwrap();
    let mut response = handle(&input).unwrap();
    input = if hook {
        resolved(&input, &response, creator)
    } else {
        proof(&input, &response, creator)
    };
    response = handle(&input).unwrap();
    for bytes in [
        borsh::to_vec(&pool).unwrap(),
        holding(input.receiver.account_id, state.token_definition_id, tokens),
        holding(
            input.receiver.account_id,
            state.collateral_definition_id,
            collateral,
        ),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    (input, response)
}

pub(super) fn withdrawn_for_route(
    hook: bool,
    collateral: u128,
) -> (ReceiveInput, Response, curve_core::PoolAccount) {
    let (input, active) = active_for_route(hook);
    let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    let mut saved = pool_state(&state, input.receiver, true);
    saved.pool.real_reserve1 = collateral;
    let (input, ready) = settlement(&input, &active, saved.clone(), 17, 19, hook);
    let (input, response) = settlement(&input, &ready, saved.clone(), 17, 19, hook);
    (input, response, saved)
}

pub(super) fn token_calls(response: &Response) -> Vec<token::Message> {
    response
        .sync_messages
        .iter()
        .filter_map(|call| {
            (call.to.program_account_id == TOKEN_PROGRAM_ACCOUNT_ID)
                .then(|| borsh::from_slice(&call.message).ok())
                .flatten()
        })
        .collect()
}

pub(super) fn adapter_state(factory: Actor, factory_state: &[u8]) -> Vec<u8> {
    let input = ReceiveInput {
        receiver: Actor::new(compute_graduation_pda(id(9), factory.account_id), id(9)),
        from: None,
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&AdapterMessage::Initialize {
            factory_program_id: id(7),
            namespace: id(40),
            launch_salt: [3; 32],
            accepted_intent: GraduationIntentKind::Curve,
        })
        .unwrap(),
    };
    let response = handle_adapter(&input).unwrap();
    let call = response.sync_messages.last().unwrap();
    assert_eq!(call.to, factory);
    assert_eq!(
        borsh::from_slice::<Message>(&call.message).unwrap(),
        Message::ReadState
    );
    let input = reply(&input, &response, factory_state.to_vec());
    handle_adapter(&input).unwrap().post_state.unwrap().to_vec()
}

pub(super) fn launch_events(response: &Response) -> Vec<SaleEventV1> {
    response
        .events
        .iter()
        .map(|event| {
            let payload: SaleEventV1 = borsh::from_slice(&event.data).unwrap();
            assert_eq!(
                event.selector,
                curve_core::sale_event(payload.clone()).selector
            );
            payload
        })
        .collect()
}
