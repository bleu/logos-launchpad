use super::*;

pub(super) fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}

pub(super) fn inputs() -> WeightedInputs {
    WeightedInputs {
        per_block_token_limit: None,
        buy_policy: curve_core::BuyPolicy::Open,
        pause_policy: curve_core::PausePolicy::AuthorityControlled,
        ending_policy: factory_core::EndingPolicy::Scheduled,
        trade_mode: factory_core::TradeMode::BuyOnly,
        sell_fee_bps: 0,
        graduation_hook: None,
        graduation_policy: factory_core::GraduationPolicy::default(),
        namespace: id(40),
        launch_salt: [3; 32],
        name: "Weighted".into(),
        uri: "https://example.test/weighted".into(),
        graduation_reserve: 0,
        sale_inventory: 800,
        creator_allocation: 200,
        collateral_seed: 100,
        token_weight_start: 95 * lbp_core::WEIGHT_TOTAL / 100,
        token_weight_end: lbp_core::WEIGHT_TOTAL / 2,
        start_timestamp: 150,
        end_timestamp: 300,
        allocation_release: CreatorAllocationRelease::AfterEnd,
        namespace_program_id: id(6),
        lbp_program_id: id(8),
        collateral_definition_id: id(41),
        creator: id(42),
    }
}

pub(super) fn creation(value: WeightedInputs) -> ReceiveInput {
    ReceiveInput {
        receiver: Actor::new(
            factory_core::compute_factory_pda(value.namespace, id(7), value.launch_salt),
            id(7),
        ),
        from: None,
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&Message::CreateWeightedLbp { inputs: value }).unwrap(),
    }
}

pub(super) fn config() -> Vec<u8> {
    let input = ReceiveInput {
        receiver: Actor::new(curve_core::compute_config_pda(id(40), id(6)), id(6)),
        from: Some(Actor::new(id(40), id(6))),
        is_authorized: true,
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

pub(super) fn holding(owner: AccountId, definition: AccountId, balance: u128) -> Vec<u8> {
    borsh::to_vec(&TokenState::Holding {
        owner,
        definition,
        kind: TokenKind::Fungible,
        balance,
    })
    .unwrap()
}

fn definition() -> Vec<u8> {
    borsh::to_vec(&TokenState::Definition {
        name: inputs().name,
        kind: TokenKind::Fungible,
        supply: 1000,
        metadata: Some(lez_compat_core::token::Metadata {
            standard: lez_compat_core::token::MetadataStandard::Simple,
            uri: inputs().uri,
            creators: String::new(),
        }),
    })
    .unwrap()
}

pub(super) fn reply(input: &ReceiveInput, response: &Response, value: Vec<u8>) -> ReceiveInput {
    ReceiveInput {
        receiver: input.receiver,
        from: Some(response.sync_messages.last().unwrap().to),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&StateReply {
            state: value.into(),
        })
        .unwrap(),
    }
}

fn finish_clock(input: &ReceiveInput, mut response: Response) -> Result<Response, Error> {
    while response
        .post_state
        .as_ref()
        .is_some_and(|state| state.starts_with(b"LZCLK001"))
    {
        assert_eq!(response.sync_messages.len(), 1);
        assert_eq!(response.sync_messages[0].to, clock_core::clock_actor());
        assert_eq!(
            borsh::from_slice::<clock_core::Message>(&response.sync_messages[0].message).unwrap(),
            clock_core::Message::ReadTimestamp { granularity: 1 }
        );
        response = handle(&ReceiveInput {
            receiver: input.receiver,
            from: Some(clock_core::clock_actor()),
            is_authorized: false,
            pre_state: response.post_state.unwrap(),
            message: borsh::to_vec(&clock_core::Reply::Timestamp(100_000)).unwrap(),
        })?;
    }
    Ok(response)
}

pub(super) fn minted(release: CreatorAllocationRelease) -> (ReceiveInput, Response, Creation) {
    let mut value = inputs();
    value.allocation_release = release;
    minted_with_terms(value)
}

fn minted_with_terms(value: WeightedInputs) -> (ReceiveInput, Response, Creation) {
    let input = creation(value);
    let response = handle(&input).unwrap();
    let setup = Creation::new(&input, &response);
    let input = setup.namespace_reply(&response, config());
    let response = handle(&input).unwrap();
    let input = reply(&input, &response, definition());
    let response = finish_clock(&input, handle(&input).unwrap()).unwrap();
    (input, response, setup)
}

pub(super) fn advance(
    setup: &Creation,
    previous: &Response,
    extra: Vec<Vec<u8>>,
) -> Result<Response, Error> {
    let state = LbpLaunchState::decode(previous.post_state.as_ref().unwrap()).unwrap();
    let mut input = setup.continuation(previous, state.creation_stage);
    let response = handle(&input)?;
    input.pre_state = response.post_state.unwrap();
    input.from = Some(Actor::new(
        state.creator_role_id,
        factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
    ));
    input.message = borsh::to_vec(&lez_compat_core::authority::Authorized {
        role: state.creator_role_id,
        authority: id(42),
    })
    .unwrap();
    let mut response = handle(&input)?;
    let mint = factory_core::compute_mint_pda(
        state.namespace,
        input.receiver.program_account_id,
        state.launch_salt,
    );
    let first = state.creation_stage == LbpCreationStage::Minted;
    let mut values = vec![
        definition(),
        holding(
            mint,
            state.token_definition_id,
            if first { 1000 } else { 0 },
        ),
        holding(
            input.receiver.account_id,
            state.token_definition_id,
            if first { 0 } else { 800 },
        ),
        holding(
            state.creator_escrow_id,
            state.token_definition_id,
            if first { 0 } else { 200 },
        ),
    ];
    values.extend(extra);
    for value in values {
        input = reply(&input, &response, value);
        response = handle(&input)?;
    }
    finish_clock(&input, response)
}

pub(super) fn prepared(
    release: CreatorAllocationRelease,
) -> (ReceiveInput, Response, Vec<u8>, Creation) {
    let mut terms = inputs();
    terms.allocation_release = release;
    prepared_with_terms(terms)
}

pub(super) fn prepared_with_terms(
    terms: WeightedInputs,
) -> (ReceiveInput, Response, Vec<u8>, Creation) {
    let policy = terms.buy_policy.clone();
    let pause_policy = terms.pause_policy;
    let policies = terms.ending_policy;
    let trade_mode = terms.trade_mode;
    let sell_fee_bps = terms.sell_fee_bps;
    let (input, response, setup) = minted_with_terms(terms);
    assert_eq!(
        LbpLaunchState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .pause_policy,
        pause_policy
    );
    assert_eq!(
        LbpLaunchState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        policy
    );
    let allocated = advance(&setup, &response, vec![]).unwrap();
    assert_eq!(
        LbpLaunchState::decode(allocated.post_state.as_ref().unwrap())
            .unwrap()
            .pause_policy,
        pause_policy
    );
    assert_eq!(
        LbpLaunchState::decode(allocated.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        policy
    );
    let prepared = advance(&setup, &allocated, vec![]).unwrap();
    assert_eq!(
        LbpLaunchState::decode(prepared.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        policy
    );
    for response in [&response, &allocated, &prepared] {
        let state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(state.ending_policy, policies);
        assert_eq!(state.trade_mode, trade_mode);
        assert_eq!(state.sell_fee_bps, sell_fee_bps);
    }
    let call = prepared
        .sync_messages
        .iter()
        .find(|m| m.to.program_account_id == id(8))
        .unwrap();
    let mut pool_input = ReceiveInput {
        receiver: call.to,
        from: Some(input.receiver),
        is_authorized: false,
        pre_state: vec![].into(),
        message: call.message.clone(),
    };
    let mut pool = lbp_core::handle(&pool_input).unwrap();
    let state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    for value in [
        config(),
        definition(),
        definition(),
        holding(input.receiver.account_id, state.token_definition_id, 800),
        holding(state.pool_id, state.token_definition_id, 0),
        holding(state.pool_id, state.collateral_definition_id, 0),
    ] {
        pool_input = reply(&pool_input, &pool, value);
        pool = lbp_core::handle(&pool_input).unwrap();
    }
    pool_input = ReceiveInput {
        receiver: pool_input.receiver,
        from: Some(clock_core::clock_actor()),
        is_authorized: false,
        pre_state: pool.post_state.unwrap(),
        message: borsh::to_vec(&clock_core::Reply::Timestamp(100_000)).unwrap(),
    };
    let pool = lbp_core::handle(&pool_input).unwrap();
    (input, prepared, pool.post_state.unwrap().to_vec(), setup)
}

pub(super) fn weighted_authorized(input: &ReceiveInput, response: &Response) -> ReceiveInput {
    let state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    ReceiveInput {
        receiver: input.receiver,
        from: Some(Actor::new(
            state.creator_role_id,
            factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
        )),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&lez_compat_core::authority::Authorized {
            role: state.creator_role_id,
            authority: id(42),
        })
        .unwrap(),
    }
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
