use factory_core::{
    CreatorAllocationRelease, Error, FixedPrice, FixedPriceInputs, Message, SaleStart,
    compute_factory_pda, handle,
};
use lee_core::{
    account::{AccountId, Actor},
    program::ReceiveInput,
};
fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}
fn inputs() -> FixedPriceInputs {
    FixedPriceInputs {
        namespace: id(40),
        launch_salt: [3; 32],
        name: "Launch".into(),
        uri: "https://example.test/launch.json".into(),
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
fn creation() -> ReceiveInput {
    ReceiveInput {
        receiver: Actor::new(compute_factory_pda(id(40), id(7), [3; 32]), id(7)),
        from: None,
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: inputs() }).unwrap(),
    }
}
#[test]
fn creation_rejects_a_factory_account_outside_the_namespace_and_salt() {
    let mut input = creation();
    input.receiver.account_id = id(99);
    assert!(matches!(handle(&input), Err(Error::WrongAccount)));
}
#[test]
fn creation_rejects_invalid_or_reused_sale_terms() {
    for case in 0..10 {
        let mut value = inputs();
        let mut input = creation();
        let error = match case {
            0 => {
                input.pre_state = vec![1].into();
                Error::AlreadyInitialized
            }
            1 => {
                value.sale_inventory = 0;
                Error::InvalidTerms
            }
            2 => {
                value.creator_allocation = u128::MAX;
                Error::Overflow
            }
            3 => {
                value.price.collateral_units = 0;
                Error::InvalidTerms
            }
            4 => {
                value.price.token_units = 0;
                Error::InvalidTerms
            }
            5 => {
                value.name.clear();
                Error::InvalidTerms
            }
            6 => {
                value.uri.clear();
                Error::InvalidTerms
            }
            7 => {
                value.end_timestamp = Some(0);
                Error::InvalidTerms
            }
            8 => {
                value.creator = AccountId::default();
                Error::InvalidTerms
            }
            _ => {
                value.collateral_definition_id =
                    factory_core::compute_definition_pda(id(40), id(7), [3; 32]);
                Error::InvalidTerms
            }
        };
        input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: value }).unwrap();
        assert!(matches!(handle(&input),Err(e) if e==error), "case {case}");
    }
}

#[test]
fn allocation_checks_creator_role_and_splits_the_exact_mint() {
    let (mut input, mint) = minted();
    input.pre_state = mint.post_state.unwrap();
    input.from = None;
    input.message = borsh::to_vec(&Message::ContinueFixedPriceCreation {
        expected_stage: factory_core::FixedPriceCreationStage::Minted,
        creator: id(42),
        now: 100,
    })
    .unwrap();
    let mut response = handle(&input).unwrap();
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(id(42), factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID)
    );
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
    let mint_owner = factory_core::compute_mint_pda(id(40), id(7), [3; 32]);
    for bytes in [
        borsh::to_vec(&definition).unwrap(),
        holding(mint_owner, state.token_definition_id, 1000),
        holding(input.receiver.account_id, state.token_definition_id, 0),
        holding(state.creator_escrow_id, state.token_definition_id, 0),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    let allocated =
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        allocated.creation_stage,
        factory_core::FixedPriceCreationStage::Allocated
    );
    let transfers: Vec<_> = response
        .sync_messages
        .iter()
        .filter_map(|m| borsh::from_slice::<lez_compat_core::token::Message>(&m.message).ok())
        .filter(|m| matches!(m, lez_compat_core::token::Message::Transfer { .. }))
        .collect();
    assert_eq!(
        transfers,
        vec![
            lez_compat_core::token::Message::Transfer {
                definition: state.token_definition_id,
                recipient: input.receiver.account_id,
                amount: 800
            },
            lez_compat_core::token::Message::Transfer {
                definition: state.token_definition_id,
                recipient: state.creator_escrow_id,
                amount: 200
            }
        ]
    );
}
fn holding(owner: AccountId, definition: AccountId, balance: u128) -> Vec<u8> {
    borsh::to_vec(&lez_compat_core::token::TokenState::Holding {
        definition,
        owner,
        kind: lez_compat_core::token::TokenKind::Fungible,
        balance,
    })
    .unwrap()
}

#[test]
fn scheduled_creation_uses_an_inclusive_upper_clock_bound() {
    let mut input = creation();
    let mut value = inputs();
    value.start = SaleStart::At { timestamp: 150 };
    input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: value }).unwrap();
    let mut response = handle(&input).unwrap();
    for bytes in [namespace_state(), collateral()] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
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
}

fn continue_sale(
    input: &ReceiveInput,
    previous: &lee_core::program::Response,
    stage: factory_core::FixedPriceCreationStage,
    source: u128,
    inventory: u128,
    escrow: u128,
) -> (ReceiveInput, lee_core::program::Response) {
    let mut input = input.clone();
    input.pre_state = previous.post_state.clone().unwrap();
    input.from = None;
    input.message = borsh::to_vec(&Message::ContinueFixedPriceCreation {
        expected_stage: stage,
        creator: id(42),
        now: 100,
    })
    .unwrap();
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
#[test]
fn pool_preparation_uses_saved_terms_and_separate_setup_stage() {
    let (input, mint) = minted();
    let (input, allocated) = continue_sale(
        &input,
        &mint,
        factory_core::FixedPriceCreationStage::Minted,
        1000,
        0,
        0,
    );
    let (input, response) = continue_sale(
        &input,
        &allocated,
        factory_core::FixedPriceCreationStage::Allocated,
        0,
        800,
        200,
    );
    let state =
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.creation_stage,
        factory_core::FixedPriceCreationStage::PoolPrepared
    );
    let call = response
        .sync_messages
        .iter()
        .find(|m| m.to == Actor::new(state.pool_id, id(8)))
        .unwrap();
    let lbp_core::Message::PrepareFixedPrice { inputs } = borsh::from_slice(&call.message).unwrap()
    else {
        panic!("pool preparation")
    };
    assert_eq!(inputs.sale_inventory, 800);
    assert_eq!(inputs.start_timestamp, 0);
    assert_eq!(inputs.end_timestamp, Some(200));
    assert_eq!(inputs.factory_program_id, input.receiver.program_account_id);
    let clock = response
        .sync_messages
        .iter()
        .find(|m| m.to.account_id == clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID)
        .unwrap();
    assert_eq!(
        borsh::from_slice::<clock_core::Message>(&clock.message).unwrap(),
        clock_core::Message::AssertTimestamp {
            at_least: 0,
            at_most: 199_999
        }
    );
}

fn prepared_pool(state: &factory_core::FixedPriceLaunchState, factory: Actor) -> Vec<u8> {
    let inputs = lbp_core::FixedPricePoolInputs {
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
#[test]
fn final_setup_funds_inventory_and_keeps_after_end_allocation_locked() {
    let (input, mint) = minted();
    let (input, allocated) = continue_sale(
        &input,
        &mint,
        factory_core::FixedPriceCreationStage::Minted,
        1000,
        0,
        0,
    );
    let (input, prepared) = continue_sale(
        &input,
        &allocated,
        factory_core::FixedPriceCreationStage::Allocated,
        0,
        800,
        200,
    );
    let (mut input, mut response) = continue_sale(
        &input,
        &prepared,
        factory_core::FixedPriceCreationStage::PoolPrepared,
        0,
        800,
        200,
    );
    let state =
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    for bytes in [
        prepared_pool(&state, input.receiver),
        holding(state.pool_id, state.token_definition_id, 0),
        holding(state.pool_id, state.collateral_definition_id, 17),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    let complete =
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        complete.creation_stage,
        factory_core::FixedPriceCreationStage::Complete
    );
    assert!(!complete.creator_allocation_released);
    let funding = response
        .sync_messages
        .iter()
        .position(|m| {
            m.to == Actor::new(
                input.receiver.account_id,
                factory_core::TOKEN_PROGRAM_ACCOUNT_ID,
            )
        })
        .unwrap();
    assert_eq!(
        borsh::from_slice::<lez_compat_core::token::Message>(
            &response.sync_messages[funding].message
        )
        .unwrap(),
        lez_compat_core::token::Message::Transfer {
            definition: state.token_definition_id,
            recipient: state.pool_id,
            amount: 800
        }
    );
    let activate = response
        .sync_messages
        .iter()
        .position(|m| m.to == Actor::new(state.pool_id, state.lbp_program_id))
        .unwrap();
    assert!(funding < activate);
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&response.sync_messages[activate].message).unwrap(),
        lbp_core::Message::ActivateFixedPrice
    );
}

#[test]
fn final_setup_releases_after_setup_allocation_to_the_current_creator() {
    let mut input = creation();
    let mut terms = inputs();
    terms.allocation_release = CreatorAllocationRelease::AfterSetup;
    input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
    let mut response = handle(&input).unwrap();
    for bytes in [namespace_state(), collateral()] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    let (input, allocated) = continue_sale(
        &input,
        &response,
        factory_core::FixedPriceCreationStage::Minted,
        1000,
        0,
        0,
    );
    let (input, prepared) = continue_sale(
        &input,
        &allocated,
        factory_core::FixedPriceCreationStage::Allocated,
        0,
        800,
        200,
    );
    let (mut input, mut response) = continue_sale(
        &input,
        &prepared,
        factory_core::FixedPriceCreationStage::PoolPrepared,
        0,
        800,
        200,
    );
    let state =
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    for bytes in [
        prepared_pool(&state, input.receiver),
        holding(state.pool_id, state.token_definition_id, 0),
        holding(state.pool_id, state.collateral_definition_id, 0),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    assert!(
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .creator_allocation_released
    );
    let payout = response
        .sync_messages
        .iter()
        .find(|m| {
            m.to == Actor::new(
                state.creator_escrow_id,
                factory_core::TOKEN_PROGRAM_ACCOUNT_ID,
            )
        })
        .unwrap();
    assert_eq!(
        borsh::from_slice::<lez_compat_core::token::Message>(&payout.message).unwrap(),
        lez_compat_core::token::Message::Transfer {
            definition: state.token_definition_id,
            recipient: id(42),
            amount: 200
        }
    );
}

#[test]
fn setup_checks_optional_expiry_at_every_stage_and_callback() {
    for end_timestamp in [None, Some(200)] {
        let mut input = creation();
        let mut terms = inputs();
        terms.end_timestamp = end_timestamp;
        terms.now = 1_000;
        input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
        let mut mint = handle(&input).unwrap();
        for bytes in [namespace_state(), collateral()] {
            input = reply(&input, &mint, bytes);
            mint = handle(&input).unwrap();
        }
        let (input, allocated) = continue_sale(
            &input,
            &mint,
            factory_core::FixedPriceCreationStage::Minted,
            1000,
            0,
            0,
        );
        let (input, prepared) = continue_sale(
            &input,
            &allocated,
            factory_core::FixedPriceCreationStage::Allocated,
            0,
            800,
            200,
        );
        let prepared_state =
            factory_core::FixedPriceLaunchState::decode(prepared.post_state.as_ref().unwrap())
                .unwrap();
        let prepare = prepared
            .sync_messages
            .iter()
            .find(|m| m.to == Actor::new(prepared_state.pool_id, prepared_state.lbp_program_id))
            .unwrap();
        let lbp_core::Message::PrepareFixedPrice { inputs } =
            borsh::from_slice(&prepare.message).unwrap()
        else {
            panic!("pool preparation")
        };
        assert_eq!(inputs.end_timestamp, end_timestamp);
        let (mut input, mut complete) = continue_sale(
            &input,
            &prepared,
            factory_core::FixedPriceCreationStage::PoolPrepared,
            0,
            800,
            200,
        );
        for bytes in [
            prepared_pool(&prepared_state, input.receiver),
            holding(
                prepared_state.pool_id,
                prepared_state.token_definition_id,
                0,
            ),
            holding(
                prepared_state.pool_id,
                prepared_state.collateral_definition_id,
                17,
            ),
        ] {
            input = reply(&input, &complete, bytes);
            complete = handle(&input).unwrap();
        }
        for (stage, response) in [mint, allocated, prepared, complete].iter().enumerate() {
            let state =
                factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap())
                    .unwrap();
            assert_eq!(state.end_timestamp, end_timestamp);
            assert!(!state.creator_allocation_released);
            let clocks: Vec<clock_core::Message> = response
                .sync_messages
                .iter()
                .filter(|m| {
                    m.to == Actor::new(
                        clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
                        clock_core::clock_account_id(),
                    )
                })
                .map(|m| borsh::from_slice(&m.message).unwrap())
                .collect();
            let mut expected = vec![];
            if stage == 0 {
                expected.push(clock_core::Message::AssertTimestamp {
                    at_least: 0,
                    at_most: u64::MAX,
                });
            }
            if let Some(end) = end_timestamp {
                expected.push(clock_core::Message::AssertTimestamp {
                    at_least: 0,
                    at_most: end * 1000 - 1,
                });
            }
            assert_eq!(clocks, expected, "stage {stage}, end {end_timestamp:?}");
        }
    }
}

#[test]
fn optional_end_must_be_strictly_after_the_saved_start() {
    for start in [
        SaleStart::Now { timestamp: 100 },
        SaleStart::At { timestamp: 100 },
    ] {
        for (end_timestamp, valid) in [
            (None, true),
            (Some(99), false),
            (Some(100), false),
            (Some(101), true),
        ] {
            let mut input = creation();
            let mut terms = inputs();
            terms.start = start;
            terms.end_timestamp = end_timestamp;
            input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
            let result = handle(&input);
            if valid {
                assert!(result.is_ok());
            } else {
                assert!(matches!(result, Err(Error::InvalidTerms)));
            }
        }
    }
}

#[test]
fn fixed_price_state_read_returns_raw_state_only_from_the_canonical_sale_actor() {
    let (mut input, mint) = minted();
    input.pre_state = mint.post_state.unwrap();
    input.from = Some(Actor::new(id(90), id(91)));
    input.message = borsh::to_vec(&Message::ReadState).unwrap();
    let response = handle(&input).unwrap();
    let reply: lee_core::program::StateReply =
        borsh::from_slice(&response.sync_messages[0].message).unwrap();
    assert_eq!(reply.state, input.pre_state);
    assert_eq!(response.sync_messages[0].to, input.from.unwrap());
    input.receiver.account_id = id(99);
    assert!(matches!(handle(&input), Err(Error::WrongAccount)));
}

#[test]
fn continuation_rejects_forged_creator_role_proofs_and_stale_stage_requests() {
    let (mut input, mint) = minted();
    input.pre_state = mint.post_state.unwrap();
    input.message = borsh::to_vec(&Message::ContinueFixedPriceCreation {
        expected_stage: factory_core::FixedPriceCreationStage::Allocated,
        creator: id(42),
        now: 0,
    })
    .unwrap();
    assert!(matches!(handle(&input), Err(Error::StaleStage)));
    input.message = borsh::to_vec(&Message::ContinueFixedPriceCreation {
        expected_stage: factory_core::FixedPriceCreationStage::Minted,
        creator: id(42),
        now: 0,
    })
    .unwrap();
    let response = handle(&input).unwrap();
    let state =
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    input.pre_state = response.post_state.unwrap();
    input.message = borsh::to_vec(&lez_compat_core::authority::Authorized {
        role: state.creator_role_id,
        authority: id(42),
    })
    .unwrap();
    for source in [
        None,
        Some(Actor::new(
            id(99),
            factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
        )),
        Some(Actor::new(state.creator_role_id, id(99))),
    ] {
        input.from = source;
        assert!(matches!(handle(&input), Err(Error::Unauthorized)));
    }
    input.from = Some(Actor::new(
        state.creator_role_id,
        factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
    ));
    input.message = borsh::to_vec(&lez_compat_core::authority::Authorized {
        role: state.creator_role_id,
        authority: id(99),
    })
    .unwrap();
    assert!(matches!(handle(&input), Err(Error::Unauthorized)));
}

#[test]
fn allocation_rejects_changed_supply_metadata_or_credited_setup_balances() {
    for case in 0..5 {
        let (mut input, mint) = minted();
        input.pre_state = mint.post_state.unwrap();
        input.message = borsh::to_vec(&Message::ContinueFixedPriceCreation {
            expected_stage: factory_core::FixedPriceCreationStage::Minted,
            creator: id(42),
            now: 0,
        })
        .unwrap();
        let response = handle(&input).unwrap();
        let state =
            factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap())
                .unwrap();
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
        let mut response = handle(&input).unwrap();
        let definition = lez_compat_core::token::TokenState::Definition {
            name: state.name.clone(),
            kind: lez_compat_core::token::TokenKind::Fungible,
            supply: if case == 0 { 1001 } else { 1000 },
            metadata: Some(lez_compat_core::token::Metadata {
                standard: lez_compat_core::token::MetadataStandard::Simple,
                uri: if case == 1 {
                    "changed".into()
                } else {
                    state.uri.clone()
                },
                creators: String::new(),
            }),
        };
        for (index, bytes) in [
            borsh::to_vec(&definition).unwrap(),
            holding(
                factory_core::compute_mint_pda(id(40), id(7), [3; 32]),
                state.token_definition_id,
                if case == 2 { 999 } else { 1000 },
            ),
            holding(
                input.receiver.account_id,
                state.token_definition_id,
                if case == 3 { 1 } else { 0 },
            ),
            holding(
                state.creator_escrow_id,
                state.token_definition_id,
                if case == 4 { 1 } else { 0 },
            ),
        ]
        .into_iter()
        .enumerate()
        {
            input = reply(&input, &response, bytes);
            let result = handle(&input);
            if index == 3 {
                assert!(result.is_err(), "case {case}");
                break;
            }
            response = result.unwrap();
        }
    }
}

fn completed() -> (ReceiveInput, factory_core::FixedPriceLaunchState) {
    let (input, response) = minted();
    let mut state =
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = factory_core::FixedPriceCreationStage::Complete;
    (input, state)
}

fn authorized(
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

#[test]
fn fixed_price_proceeds_require_creator_role_before_forwarding_to_the_bound_pool() {
    let (mut input, state) = completed();
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.from = None;
    input.message = borsh::to_vec(&Message::WithdrawFactoryProceeds {
        creator: id(42),
        now: 100,
    })
    .unwrap();
    let response = handle(&input).unwrap();
    assert_eq!(response.sync_messages.len(), 1);
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(id(42), factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID)
    );
    assert_eq!(
        borsh::from_slice::<lez_compat_core::authority::Message>(
            &response.sync_messages[0].message
        )
        .unwrap(),
        lez_compat_core::authority::Message::Check {
            role: state.creator_role_id,
            callback: input.receiver,
        }
    );
    let response = handle(&authorized(&input, &response, id(42))).unwrap();
    assert_eq!(response.sync_messages.len(), 1);
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(state.pool_id, state.lbp_program_id)
    );
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&response.sync_messages[0].message).unwrap(),
        lbp_core::Message::SettleFixedPrice {
            creator: id(42),
            now: 100
        }
    );
    assert_eq!(
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap(),
        state
    );
}

fn claim_allocation(
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

fn active_pool(
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

#[test]
fn after_end_allocation_claim_at_expiry_does_not_settle_proceeds() {
    let (input, state) = completed();
    let pool = active_pool(&state, input.receiver);
    let response = claim_allocation(&input, &state, &pool, 200, 217).unwrap();
    let claimed =
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(claimed.creator_allocation_released);
    assert_eq!(response.sync_messages.len(), 2);
    assert_eq!(
        borsh::from_slice::<clock_core::Message>(&response.sync_messages[0].message).unwrap(),
        clock_core::Message::AssertTimestamp {
            at_least: 200_000,
            at_most: u64::MAX
        }
    );
    let payout = &response.sync_messages[1];
    assert_eq!(
        payout.to,
        Actor::new(
            state.creator_escrow_id,
            factory_core::TOKEN_PROGRAM_ACCOUNT_ID
        )
    );
    assert_eq!(
        borsh::from_slice::<lez_compat_core::token::Message>(&payout.message).unwrap(),
        lez_compat_core::token::Message::Transfer {
            definition: state.token_definition_id,
            recipient: id(42),
            amount: 200,
        }
    );
    assert_eq!(
        payout.pda_seeds,
        [factory_core::compute_escrow_seed(
            state.namespace,
            state.launch_salt
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn released_allocation_retry_requires_authority_but_no_clock_or_escrow_transfer() {
    let (input, state) = completed();
    let pool = active_pool(&state, input.receiver);
    let response = claim_allocation(&input, &state, &pool, 200, 200).unwrap();
    let claimed =
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    let repeated = claim_allocation(&input, &claimed, &pool, 0, 0).unwrap();
    assert!(repeated.sync_messages.is_empty());
    assert_eq!(
        factory_core::FixedPriceLaunchState::decode(repeated.post_state.as_ref().unwrap()).unwrap(),
        claimed
    );
}

#[test]
fn payout_role_callbacks_require_the_canonical_sale_actor_and_saved_creator_role() {
    for command in [
        Message::WithdrawFactoryProceeds {
            creator: id(42),
            now: 200,
        },
        Message::ClaimCreatorAllocation {
            creator: id(42),
            now: 200,
        },
    ] {
        let (mut input, state) = completed();
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.message = borsh::to_vec(&command).unwrap();
        let response = handle(&input).unwrap();
        let proof = authorized(&input, &response, id(42));
        let mut wrong_sale = proof.clone();
        wrong_sale.receiver.account_id = id(99);
        assert_eq!(handle(&wrong_sale).err(), Some(Error::WrongAccount));
        for from in [
            None,
            Some(Actor::new(
                id(99),
                factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
            )),
            Some(Actor::new(state.creator_role_id, id(99))),
        ] {
            let mut forged = proof.clone();
            forged.from = from;
            assert_eq!(handle(&forged).err(), Some(Error::Unauthorized));
        }
        assert_eq!(
            handle(&authorized(&input, &response, id(99))).err(),
            Some(Error::Unauthorized)
        );
    }
}

#[test]
fn sellout_releases_after_end_allocation_before_or_after_proceeds_settlement() {
    let (input, mut state) = completed();
    for end in [None, Some(200)] {
        state.end_timestamp = end;
        let mut pool = active_pool(&state, input.receiver);
        pool.remaining_inventory = 0;
        for settled in [false, true] {
            pool.proceeds_settled = settled;
            let response = claim_allocation(&input, &state, &pool, 100, 217).unwrap();
            assert_eq!(response.sync_messages.len(), 1);
            assert_eq!(
                borsh::from_slice::<lez_compat_core::token::Message>(
                    &response.sync_messages[0].message
                )
                .unwrap(),
                lez_compat_core::token::Message::Transfer {
                    definition: state.token_definition_id,
                    recipient: id(42),
                    amount: 200,
                }
            );
        }
    }
}

#[test]
fn allocation_stays_locked_before_close_and_requires_escrow_coverage() {
    let (input, mut state) = completed();
    let pool = active_pool(&state, input.receiver);
    assert_eq!(
        claim_allocation(&input, &state, &pool, 199, 200).err(),
        Some(Error::NotClosed)
    );
    assert_eq!(
        claim_allocation(&input, &state, &pool, 200, 199).err(),
        Some(Error::InvalidCustody)
    );
    state.end_timestamp = None;
    let pool = active_pool(&state, input.receiver);
    assert_eq!(
        claim_allocation(&input, &state, &pool, u64::MAX, 200).err(),
        Some(Error::NotClosed)
    );
    state.end_timestamp = Some(u64::MAX);
    let mut pool = pool;
    pool.end_timestamp = state.end_timestamp;
    assert_eq!(
        claim_allocation(&input, &state, &pool, u64::MAX, 200).err(),
        Some(Error::InvalidTerms)
    );
}

#[test]
fn expiry_burn_does_not_replace_the_allocation_clock_check_with_sellout() {
    let (input, state) = completed();
    let mut pool = active_pool(&state, input.receiver);
    pool.proceeds_settled = true;
    pool.recorded_collateral = 0;
    pool.accrued_fees = 0;
    assert_eq!(
        claim_allocation(&input, &state, &pool, 199, 200).err(),
        Some(Error::NotClosed)
    );
    let response = claim_allocation(&input, &state, &pool, 200, 200).unwrap();
    assert_eq!(response.sync_messages.len(), 2);
    assert_eq!(
        borsh::from_slice::<clock_core::Message>(&response.sync_messages[0].message).unwrap(),
        clock_core::Message::AssertTimestamp {
            at_least: 200_000,
            at_most: u64::MAX,
        }
    );
}

#[test]
fn zero_and_after_setup_allocations_never_emit_a_second_token_payout() {
    let (input, mut state) = completed();
    state.creator_allocation = 0;
    let pool = active_pool(&state, input.receiver);
    let response = claim_allocation(&input, &state, &pool, 200, 0).unwrap();
    assert!(
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .creator_allocation_released
    );
    assert_eq!(response.sync_messages.len(), 1);
    assert_eq!(
        response.sync_messages[0].to.account_id,
        clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID
    );
    state.creator_allocation = 200;
    state.allocation_release = CreatorAllocationRelease::AfterSetup;
    state.creator_allocation_released = true;
    let pool = active_pool(&state, input.receiver);
    let response = claim_allocation(&input, &state, &pool, 100, 0).unwrap();
    assert!(response.sync_messages.is_empty());
}

#[test]
fn allocation_callbacks_reject_substituted_pool_and_escrow_actors_or_holdings() {
    let (mut input, state) = completed();
    let pool = active_pool(&state, input.receiver);
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::ClaimCreatorAllocation {
        creator: id(42),
        now: 200,
    })
    .unwrap();
    let response = handle(&input).unwrap();
    input = authorized(&input, &response, id(42));
    let mut response = handle(&input).unwrap();
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(state.pool_id, state.lbp_program_id)
    );
    for bytes in [
        borsh::to_vec(&pool).unwrap(),
        holding(state.creator_escrow_id, state.token_definition_id, 200),
    ] {
        input = reply(&input, &response, bytes);
        let source = input.from.unwrap();
        for from in [
            None,
            Some(Actor::new(id(99), source.program_account_id)),
            Some(Actor::new(source.account_id, id(99))),
        ] {
            let mut forged = input.clone();
            forged.from = from;
            assert_eq!(handle(&forged).err(), Some(Error::Unauthorized));
        }
        let mut malformed = input.clone();
        malformed.message = vec![255];
        assert_eq!(handle(&malformed).err(), Some(Error::InvalidMessage));
        if source != Actor::new(state.pool_id, state.lbp_program_id) {
            assert_eq!(
                source,
                Actor::new(
                    lez_compat_core::token::holding_address(
                        factory_core::TOKEN_PROGRAM_ACCOUNT_ID,
                        state.creator_escrow_id,
                        state.token_definition_id
                    ),
                    factory_core::TOKEN_PROGRAM_ACCOUNT_ID
                )
            );
            let wrong_holding = reply(
                &input,
                &response,
                holding(id(99), state.token_definition_id, 200),
            );
            assert_eq!(handle(&wrong_holding).err(), Some(Error::InvalidCustody));
        }
        response = handle(&input).unwrap();
    }
}

#[test]
fn allocation_rejects_pool_terms_that_do_not_match_the_saved_sale() {
    let (input, state) = completed();
    let pool = active_pool(&state, input.receiver);
    for case in 0..14 {
        let mut changed = pool.clone();
        match case {
            0 => changed.namespace = id(99),
            1 => changed.namespace_program_id = id(99),
            2 => changed.factory_owner = id(99),
            3 => changed.factory_program_id = id(99),
            4 => changed.factory_seed = [99; 32],
            5 => changed.token_definition_id = id(99),
            6 => changed.collateral_definition_id = id(99),
            7 => changed.sale_inventory += 1,
            8 => changed.creator_allocation += 1,
            9 => changed.price.collateral_units += 1,
            10 => changed.start_timestamp += 1,
            11 => changed.end_timestamp = None,
            12 => changed.creator_allocation_release = CreatorAllocationRelease::AfterSetup,
            _ => changed.funded = false,
        }
        let expected = if case == 13 {
            Error::CreationPending
        } else {
            Error::InvalidState
        };
        assert_eq!(
            claim_allocation(&input, &state, &changed, 200, 200).err(),
            Some(expected),
            "case {case}"
        );
    }
}

fn reply(
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
fn collateral() -> Vec<u8> {
    borsh::to_vec(&lez_compat_core::token::TokenState::Definition {
        name: "Collateral".into(),
        kind: lez_compat_core::token::TokenKind::Fungible,
        supply: 1000000,
        metadata: None,
    })
    .unwrap()
}
fn minted() -> (ReceiveInput, lee_core::program::Response) {
    let mut input = creation();
    let mut response = handle(&input).unwrap();
    for bytes in [namespace_state(), collateral()] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    (input, response)
}
#[test]
fn creation_mints_exact_supply_with_metadata_and_a_separate_creator_role() {
    let (input, response) = minted();
    let state =
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.total_supply, 1000);
    assert_eq!(state.start_timestamp, 0);
    assert_eq!(
        state.creation_stage,
        factory_core::FixedPriceCreationStage::Minted
    );
    assert_ne!(state.creator_role_id, input.receiver.account_id);
    let mint = response
        .sync_messages
        .iter()
        .find(|m| m.to.account_id == state.token_definition_id)
        .unwrap();
    assert_eq!(
        borsh::from_slice::<lez_compat_core::token::Message>(&mint.message).unwrap(),
        lez_compat_core::token::Message::Create {
            name: "Launch".into(),
            kind: lez_compat_core::token::TokenKind::Fungible,
            supply: 1000,
            recipient: factory_core::compute_mint_pda(id(40), id(7), [3; 32]),
            metadata: Some(lez_compat_core::token::Metadata {
                standard: lez_compat_core::token::MetadataStandard::Simple,
                uri: "https://example.test/launch.json".into(),
                creators: String::new()
            })
        }
    );
    let role = response
        .sync_messages
        .iter()
        .find(|m| m.to.account_id == state.creator_role_id)
        .unwrap();
    assert_eq!(
        borsh::from_slice::<lez_compat_core::authority::Message>(&role.message).unwrap(),
        lez_compat_core::authority::Message::Bootstrap { authority: id(42) }
    );
    let clock = response
        .sync_messages
        .iter()
        .find(|m| m.to.account_id == clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID)
        .unwrap();
    assert_eq!(
        borsh::from_slice::<clock_core::Message>(&clock.message).unwrap(),
        clock_core::Message::AssertTimestamp {
            at_least: 0,
            at_most: u64::MAX
        }
    );
}
