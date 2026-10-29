use factory_core::{
    CreatorAllocationRelease, Error, LbpCreationStage, LbpLaunchState, LbpSetupMode, Message,
    WeightedInputs, handle,
};
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response, StateReply},
};
use lez_compat_core::token::{TokenKind, TokenState};

fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}
fn inputs() -> WeightedInputs {
    WeightedInputs {
        namespace: id(40),
        launch_salt: [3; 32],
        name: "Weighted".into(),
        uri: "https://example.test/weighted".into(),
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
fn creation(value: WeightedInputs) -> ReceiveInput {
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
fn config() -> Vec<u8> {
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
fn holding(owner: AccountId, definition: AccountId, balance: u128) -> Vec<u8> {
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
fn reply(input: &ReceiveInput, response: &Response, value: Vec<u8>) -> ReceiveInput {
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
fn minted(release: CreatorAllocationRelease) -> (ReceiveInput, Response) {
    let mut value = inputs();
    value.allocation_release = release;
    let mut input = creation(value);
    let mut response = handle(&input).unwrap();
    for bytes in [config(), definition()] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap();
    }
    (input, response)
}
fn advance(
    input: &ReceiveInput,
    previous: &Response,
    extra: Vec<Vec<u8>>,
) -> Result<Response, Error> {
    let state = LbpLaunchState::decode(previous.post_state.as_ref().unwrap()).unwrap();
    let mut input = input.clone();
    input.pre_state = previous.post_state.clone().unwrap();
    input.from = None;
    input.message = borsh::to_vec(&Message::ContinueWeightedCreation {
        expected_stage: state.creation_stage,
        creator: id(42),
    })
    .unwrap();
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
    Ok(response)
}
fn prepared(release: CreatorAllocationRelease) -> (ReceiveInput, Response, Vec<u8>) {
    let (input, response) = minted(release);
    let allocated = advance(&input, &response, vec![]).unwrap();
    let prepared = advance(&input, &allocated, vec![]).unwrap();
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
    (input, prepared, pool.post_state.unwrap().to_vec())
}

#[test]
fn weighted_creation_validates_terms_and_keeps_a_separate_mode() {
    for case in 0..9 {
        let mut value = inputs();
        match case {
            0 => value.collateral_seed = 0,
            1 => value.token_weight_start = lbp_core::MIN_WEIGHT - 1,
            2 => value.token_weight_end = lbp_core::WEIGHT_TOTAL - lbp_core::MIN_WEIGHT + 1,
            3 => value.end_timestamp = value.start_timestamp,
            4 => value.creator = AccountId::default(),
            5 => value.name.clear(),
            6 => value.uri.clear(),
            7 => value.creator_allocation = u128::MAX,
            _ => {
                value.collateral_definition_id =
                    factory_core::compute_definition_pda(value.namespace, id(7), value.launch_salt)
            }
        }
        assert_eq!(
            handle(&creation(value)).err(),
            Some(if case == 7 {
                Error::Overflow
            } else {
                Error::InvalidTerms
            })
        );
    }
    let (input, response) = minted(CreatorAllocationRelease::AfterEnd);
    let state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(matches!(
        state.mode,
        LbpSetupMode::Weighted {
            collateral_seed: 100,
            ..
        }
    ));
    assert_eq!(state.total_supply, 1000);
    let clock = response
        .sync_messages
        .iter()
        .find(|m| m.to.account_id == clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID)
        .unwrap();
    assert_eq!(
        borsh::from_slice::<clock_core::Message>(&clock.message).unwrap(),
        clock_core::Message::AssertTimestamp {
            at_least: 0,
            at_most: 150_000
        }
    );
    let mut request = input.clone();
    request.pre_state = response.post_state.unwrap();
    request.message = borsh::to_vec(&Message::ContinueFixedPriceCreation {
        expected_stage: LbpCreationStage::Minted,
        creator: id(42),
        now: 100,
    })
    .unwrap();
    assert_eq!(handle(&request).err(), Some(Error::InvalidState));
}

#[test]
fn weighted_final_setup_requires_creator_seed_and_keeps_surplus_separate() {
    for release in [
        CreatorAllocationRelease::AfterEnd,
        CreatorAllocationRelease::AfterSetup,
    ] {
        let (input, previous, pool) = prepared(release);
        let state = LbpLaunchState::decode(previous.post_state.as_ref().unwrap()).unwrap();
        let extra = |seed_balance, vault_balance| {
            vec![
                pool.clone(),
                holding(state.pool_id, state.token_definition_id, 0),
                holding(state.pool_id, state.collateral_definition_id, vault_balance),
                holding(id(42), state.collateral_definition_id, seed_balance),
            ]
        };
        assert_eq!(
            advance(&input, &previous, extra(99, 41)).err(),
            Some(Error::InvalidCustody)
        );
        assert_eq!(
            advance(&input, &previous, extra(100, u128::MAX)).err(),
            Some(Error::Overflow)
        );
        let response = advance(&input, &previous, extra(100, 41)).unwrap();
        let next = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(next.creation_stage, LbpCreationStage::Complete);
        assert_eq!(
            next.creator_allocation_released,
            release == CreatorAllocationRelease::AfterSetup
        );
        assert!(response.sync_messages.iter().any(|call| call.to
            == Actor::new(id(42), factory_core::TOKEN_PROGRAM_ACCOUNT_ID)
            && borsh::from_slice::<lez_compat_core::token::Message>(&call.message).unwrap()
                == lez_compat_core::token::Message::Transfer {
                    definition: state.collateral_definition_id,
                    recipient: state.pool_id,
                    amount: 100,
                }));
        assert!(response.sync_messages.iter().any(|call| call.to
            == Actor::new(state.pool_id, state.lbp_program_id)
            && borsh::from_slice::<lbp_core::Message>(&call.message).unwrap()
                == lbp_core::Message::ActivateWeighted));
        let mut changed = lbp_core::PoolAccount::decode(&pool).unwrap();
        if let lbp_core::PoolMode::Weighted {
            collateral_seed, ..
        } = &mut changed.mode
        {
            *collateral_seed += 1;
        }
        let mut wrong = extra(100, 41);
        wrong[0] = borsh::to_vec(&changed).unwrap();
        assert_eq!(
            advance(&input, &previous, wrong).err(),
            Some(Error::InvalidState)
        );
    }
}
