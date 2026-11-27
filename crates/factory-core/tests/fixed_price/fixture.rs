use super::*;

pub(super) fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}

pub(super) fn inputs() -> FixedPriceInputs {
    FixedPriceInputs {
        per_block_token_limit: None,
        buy_policy: curve_core::BuyPolicy::Open,
        pause_policy: curve_core::PausePolicy::Disabled,
        ending_policy: factory_core::EndingPolicy::Either,
        trade_mode: factory_core::TradeMode::BuyOnly,
        sell_fee_bps: 0,
        graduation_hook: None,
        graduation_policy: factory_core::GraduationPolicy::default(),
        namespace: id(40),
        launch_salt: [3; 32],
        name: "Launch".into(),
        uri: "https://example.test/launch.json".into(),
        graduation_reserve: 0,
        sale_inventory: 800,
        creator_allocation: 200,
        price: FixedPrice {
            collateral_units: 3,
            token_units: 2,
        },
        start: SaleStart::Now { timestamp: 0 },
        end_timestamp: Some(200),
        allocation_release: CreatorAllocationRelease::AfterEnd,
        namespace_program_id: id(6),
        lbp_program_id: id(8),
        collateral_definition_id: id(41),
        creator: id(42),
        now: 100,
    }
}

pub(super) fn creation() -> ReceiveInput {
    ReceiveInput {
        receiver: Actor::new(compute_factory_pda(id(40), id(7), [3; 32]), id(7)),
        from: None,
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: inputs() }).unwrap(),
    }
}

pub(super) fn holding(owner: AccountId, definition: AccountId, balance: u128) -> Vec<u8> {
    borsh::to_vec(&lez_compat_core::token::TokenState::Holding {
        definition,
        owner,
        kind: lez_compat_core::token::TokenKind::Fungible,
        balance,
    })
    .unwrap()
}

pub(super) fn continue_sale(
    setup: &Creation,
    previous: &Response,
    stage: factory_core::FixedPriceCreationStage,
    source: u128,
    inventory: u128,
    escrow: u128,
) -> (ReceiveInput, Response) {
    let mut input = setup.continuation(previous, stage);
    let mut response = handle(&input).unwrap();
    let state =
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
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
    response = handle(&input).unwrap();
    let definition = lez_compat_core::token::TokenState::Definition {
        name: state.name.clone(),
        kind: lez_compat_core::token::TokenKind::Fungible,
        supply: 1000,
        metadata: Some(lez_compat_core::token::Metadata {
            standard: lez_compat_core::token::MetadataStandard::Simple,
            uri: state.uri.clone(),
            creators: String::new(),
        }),
    };
    for bytes in [
        borsh::to_vec(&definition).unwrap(),
        holding(
            factory_core::compute_mint_pda(id(40), id(7), [3; 32]),
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

pub(super) fn prepared_pool(
    state: &factory_core::FixedPriceLaunchState,
    factory: Actor,
) -> Vec<u8> {
    let inputs = lbp_core::FixedPricePoolInputs {
        per_block_token_limit: state.per_block_token_limit,
        buy_policy: state.buy_policy.clone(),
        pause_policy: state.pause_policy,
        ending_policy: state.ending_policy,
        trade_mode: state.trade_mode,
        sell_fee_bps: state.sell_fee_bps,
        graduation_policy: state.graduation_policy,
        graduation_adapter: state.graduation_hook.as_ref().map(|hook| {
            Actor::new(
                factory_core::compute_graduation_pda(hook.program_id, factory.account_id),
                hook.program_id,
            )
        }),
        namespace: state.namespace,
        namespace_program_id: state.namespace_program_id,
        factory_program_id: factory.program_account_id,
        factory_seed: *factory_core::compute_factory_seed(state.namespace, state.launch_salt)
            .as_bytes(),
        token_definition_id: state.token_definition_id,
        collateral_definition_id: state.collateral_definition_id,
        sale_inventory: state.sale_inventory,
        creator_allocation: state.creator_allocation,
        price: state.price,
        start_timestamp: state.start_timestamp,
        end_timestamp: state.end_timestamp,
        creator_allocation_release: state.allocation_release,
    };
    let mut input = ReceiveInput {
        receiver: Actor::new(state.pool_id, state.lbp_program_id),
        from: Some(factory),
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&lbp_core::Message::PrepareFixedPrice { inputs }).unwrap(),
    };
    let mut response = lbp_core::handle(&input).unwrap();
    for bytes in [
        namespace_state(),
        collateral(),
        collateral(),
        holding(factory.account_id, state.token_definition_id, 800),
        holding(state.pool_id, state.token_definition_id, 0),
        holding(state.pool_id, state.collateral_definition_id, 0),
    ] {
        input = reply(&input, &response, bytes);
        response = lbp_core::handle(&input).unwrap()
    }
    response.post_state.unwrap().to_vec()
}

pub(super) fn completed() -> (ReceiveInput, factory_core::FixedPriceLaunchState) {
    let (input, response, _) = minted();
    let mut state =
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = factory_core::FixedPriceCreationStage::Complete;
    (input, state)
}

pub(super) fn authorized(
    input: &ReceiveInput,
    response: &lee_core::program::Response,
    creator: AccountId,
) -> ReceiveInput {
    let state =
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
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
            authority: creator,
        })
        .unwrap(),
    }
}

pub(super) fn claim_allocation(
    input: &ReceiveInput,
    state: &factory_core::FixedPriceLaunchState,
    pool: &lbp_core::PoolAccount,
    now: u64,
    escrow_balance: u128,
) -> Result<lee_core::program::Response, Error> {
    let mut input = input.clone();
    input.pre_state = borsh::to_vec(state).unwrap().into();
    input.from = None;
    input.message = borsh::to_vec(&Message::ClaimCreatorAllocation {
        creator: id(42),
        now,
    })
    .unwrap();
    let response = handle(&input)?;
    input = authorized(&input, &response, id(42));
    let mut response = handle(&input)?;
    if response.sync_messages.is_empty() {
        return Ok(response);
    }
    let mut values = vec![borsh::to_vec(pool).unwrap()];
    if state.creator_allocation != 0 {
        values.push(holding(
            state.creator_escrow_id,
            state.token_definition_id,
            escrow_balance,
        ));
    }
    for bytes in values {
        input = reply(&input, &response, bytes);
        response = handle(&input)?;
    }
    Ok(response)
}

pub(super) fn active_pool(
    state: &factory_core::FixedPriceLaunchState,
    factory: Actor,
) -> lbp_core::PoolAccount {
    let mut pool = lbp_core::PoolAccount::decode(&prepared_pool(state, factory)).unwrap();
    pool.funded = true;
    pool.remaining_inventory = 797;
    pool.recorded_collateral = 4;
    pool.accrued_fees = 1;
    pool
}

pub(super) fn reply(
    input: &ReceiveInput,
    response: &lee_core::program::Response,
    bytes: Vec<u8>,
) -> ReceiveInput {
    ReceiveInput {
        receiver: input.receiver,
        from: Some(response.sync_messages.last().unwrap().to),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&lee_core::program::StateReply {
            state: bytes.into(),
        })
        .unwrap(),
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

pub(super) fn collateral() -> Vec<u8> {
    borsh::to_vec(&lez_compat_core::token::TokenState::Definition {
        name: "Collateral".into(),
        kind: lez_compat_core::token::TokenKind::Fungible,
        supply: 1000000,
        metadata: None,
    })
    .unwrap()
}

pub(super) fn minted() -> (ReceiveInput, Response, Creation) {
    mint_from(&creation())
}

pub(super) fn mint_from(input: &ReceiveInput) -> (ReceiveInput, Response, Creation) {
    let response = handle(input).unwrap();
    let setup = Creation::new(input, &response);
    let input = setup.namespace_reply(&response, namespace_state());
    let response = handle(&input).unwrap();
    let input = reply(&input, &response, collateral());
    let response = handle(&input).unwrap();
    (input, response, setup)
}

pub(super) fn allowlisted_prepared() -> (ReceiveInput, Response, Creation) {
    let mut input = creation();
    let mut terms = inputs();
    let pool = lbp_core::compute_pool_pda(
        terms.namespace,
        terms.lbp_program_id,
        factory_core::compute_definition_pda(terms.namespace, id(7), terms.launch_salt),
        terms.collateral_definition_id,
        input.receiver.account_id,
    );
    let policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *terms.namespace.value(),
        sale_policy_id: *pool.value(),
        root: [9; 32],
    });
    terms.buy_policy = policy.clone();
    input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
    let initial = handle(&input).unwrap();
    let setup = Creation::new(&input, &initial);
    let namespace_reply = setup.namespace_reply(&initial, namespace_state());
    let checked = handle(&namespace_reply).unwrap();
    let mut response = handle(&reply(&namespace_reply, &checked, collateral())).unwrap();
    for response in [&checked, &response] {
        assert_eq!(
            factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap())
                .unwrap()
                .buy_policy,
            policy
        );
    }
    for (stage, source, inventory, escrow) in [
        (factory_core::FixedPriceCreationStage::Minted, 1000, 0, 0),
        (
            factory_core::FixedPriceCreationStage::Allocated,
            0,
            800,
            200,
        ),
    ] {
        (_, response) = continue_sale(&setup, &response, stage, source, inventory, escrow);
        assert_eq!(
            factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap())
                .unwrap()
                .buy_policy,
            policy
        );
    }
    let call = response
        .sync_messages
        .iter()
        .find(|call| call.to.account_id == pool)
        .unwrap();
    let lbp_core::Message::PrepareFixedPrice { inputs } = borsh::from_slice(&call.message).unwrap()
    else {
        unreachable!()
    };
    assert_eq!(inputs.buy_policy, policy);
    (input, response, setup)
}

pub(super) fn launch_events(response: &lee_core::program::Response) -> Vec<SaleEventV1> {
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
