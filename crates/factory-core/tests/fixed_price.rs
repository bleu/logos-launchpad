#[path = "support/lbp_pause_forwarding.rs"]
mod lbp_pause_forwarding;

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
        buy_policy: curve_core::BuyPolicy::Open,
        pause_policy: curve_core::PausePolicy::Disabled,
        ending_policy: factory_core::EndingPolicy::Either,
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
fn scheduled_creation_allows_inclusion_at_the_saved_start_millisecond() {
    let mut input = creation();
    let mut value = inputs();
    value.start = SaleStart::At { timestamp: 150 };
    input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: value }).unwrap();
    let mut response = handle(&input).unwrap();
    for bytes in [namespace_state(), collateral()] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    assert_eq!(response.validity.timestamps.start(), None);
    assert_eq!(response.validity.timestamps.end(), Some(150_001));
    assert!(response.validity.timestamps.is_valid_for(150_000));
    assert!(!response.validity.timestamps.is_valid_for(150_001));
}

#[test]
fn creation_intersects_saved_start_with_optional_setup_end() {
    for (start, end_timestamp, lower, upper) in [
        (
            SaleStart::Now { timestamp: 0 },
            Some(200),
            Some(0),
            Some(200_000),
        ),
        (
            SaleStart::Now { timestamp: 150 },
            Some(200),
            Some(150_000),
            Some(200_000),
        ),
        (SaleStart::Now { timestamp: 150 }, None, Some(150_000), None),
        (SaleStart::At { timestamp: 0 }, Some(200), None, Some(1)),
        (
            SaleStart::At { timestamp: 150 },
            Some(151),
            None,
            Some(150_001),
        ),
        (SaleStart::At { timestamp: 150 }, None, None, Some(150_001)),
    ] {
        let mut input = creation();
        let mut terms = inputs();
        terms.start = start;
        terms.end_timestamp = end_timestamp;
        terms.ending_policy = if end_timestamp.is_some() {
            factory_core::EndingPolicy::Either
        } else {
            factory_core::EndingPolicy::Depletion
        };
        input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
        let mut response = handle(&input).unwrap();
        for bytes in [namespace_state(), collateral()] {
            input = reply(&input, &response, bytes);
            response = handle(&input).unwrap();
        }
        assert_eq!(response.validity.timestamps.start(), lower);
        assert_eq!(response.validity.timestamps.end(), upper);
    }
}

#[test]
fn creation_rejects_timestamp_conversion_overflow_before_minting() {
    for (start, end_timestamp) in [
        (
            SaleStart::Now {
                timestamp: u64::MAX,
            },
            None,
        ),
        (
            SaleStart::At {
                timestamp: u64::MAX,
            },
            None,
        ),
        (SaleStart::Now { timestamp: 0 }, Some(u64::MAX)),
    ] {
        let mut input = creation();
        let mut terms = inputs();
        terms.start = start;
        terms.end_timestamp = end_timestamp;
        terms.ending_policy = if end_timestamp.is_some() {
            factory_core::EndingPolicy::Either
        } else {
            factory_core::EndingPolicy::Depletion
        };
        input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
        assert_eq!(handle(&input).err(), Some(Error::InvalidTerms));
    }
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
    assert_eq!(inputs.ending_policy, factory_core::EndingPolicy::Either);
    assert_eq!(inputs.start_timestamp, 0);
    assert_eq!(inputs.end_timestamp, Some(200));
    assert_eq!(inputs.factory_program_id, input.receiver.program_account_id);
    assert_eq!(response.validity.timestamps.start(), None);
    assert_eq!(response.validity.timestamps.end(), Some(200_000));
}

fn prepared_pool(state: &factory_core::FixedPriceLaunchState, factory: Actor) -> Vec<u8> {
    let inputs = lbp_core::FixedPricePoolInputs {
        buy_policy: state.buy_policy.clone(),
        pause_policy: state.pause_policy,
        ending_policy: state.ending_policy,
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
        terms.ending_policy = if end_timestamp.is_some() {
            factory_core::EndingPolicy::Either
        } else {
            factory_core::EndingPolicy::Depletion
        };
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
            assert_eq!(
                state.ending_policy,
                if end_timestamp.is_some() {
                    factory_core::EndingPolicy::Either
                } else {
                    factory_core::EndingPolicy::Depletion
                }
            );
            assert!(!state.creator_allocation_released);
            assert_eq!(
                response.validity.timestamps.start(),
                if stage == 0 { Some(0) } else { None }
            );
            assert_eq!(
                response.validity.timestamps.end(),
                end_timestamp.map(|_| 200_000),
                "stage {stage}"
            );
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
            terms.ending_policy = if end_timestamp.is_some() {
                factory_core::EndingPolicy::Either
            } else {
                factory_core::EndingPolicy::Depletion
            };
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
    let response = claim_allocation(&input, &state, &pool, 999, 217).unwrap();
    let claimed =
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(claimed.creator_allocation_released);
    assert_eq!(response.sync_messages.len(), 1);
    assert_eq!(response.validity.timestamps.start(), Some(200_000));
    assert_eq!(response.validity.timestamps.end(), None);
    let payout = &response.sync_messages[0];
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
fn released_allocation_retry_keeps_selected_close_bound_without_another_transfer() {
    let (input, state) = completed();
    let pool = active_pool(&state, input.receiver);
    let response = claim_allocation(&input, &state, &pool, 200, 200).unwrap();
    let claimed =
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    let repeated = claim_allocation(&input, &claimed, &pool, 0, 0).unwrap();
    assert!(repeated.sync_messages.is_empty());
    assert_eq!(repeated.validity.timestamps.start(), Some(200_000));
    assert_eq!(repeated.validity.timestamps.end(), None);
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
        state.ending_policy = if end.is_some() {
            factory_core::EndingPolicy::Either
        } else {
            factory_core::EndingPolicy::Depletion
        };
        let mut pool = active_pool(&state, input.receiver);
        pool.remaining_inventory = 0;
        for settled in [false, true] {
            pool.proceeds_settled = settled;
            let response = claim_allocation(&input, &state, &pool, 100, 217).unwrap();
            assert_eq!(response.sync_messages.len(), 1);
            assert_eq!(response.validity.timestamps.start(), None);
            assert_eq!(response.validity.timestamps.end(), None);
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
    let before_end = claim_allocation(&input, &state, &pool, 199, 200).unwrap();
    assert!(!before_end.validity.timestamps.is_valid_for(199_999));
    assert!(before_end.validity.timestamps.is_valid_for(200_000));
    assert_eq!(
        claim_allocation(&input, &state, &pool, 200, 199).err(),
        Some(Error::InvalidCustody)
    );
    state.end_timestamp = None;
    state.ending_policy = factory_core::EndingPolicy::Depletion;
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
        Some(Error::InvalidState)
    );
}

#[test]
fn expiry_burn_keeps_the_allocation_inclusion_bound_until_expiry() {
    let (input, state) = completed();
    let mut pool = active_pool(&state, input.receiver);
    pool.proceeds_settled = true;
    pool.recorded_collateral = 0;
    pool.accrued_fees = 0;
    let before_end = claim_allocation(&input, &state, &pool, 199, 200).unwrap();
    assert!(!before_end.validity.timestamps.is_valid_for(199_999));
    assert!(before_end.validity.timestamps.is_valid_for(200_000));
    let response = claim_allocation(&input, &state, &pool, 200, 200).unwrap();
    assert_eq!(response.sync_messages.len(), 1);
    assert_eq!(response.validity.timestamps.start(), Some(200_000));
    assert_eq!(response.validity.timestamps.end(), None);
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
    assert!(response.sync_messages.is_empty());
    assert_eq!(response.validity.timestamps.start(), Some(200_000));
    assert_eq!(response.validity.timestamps.end(), None);
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
    assert_eq!(response.validity.timestamps.start(), Some(0));
    assert_eq!(response.validity.timestamps.end(), Some(200_000));
}

#[test]
fn fixed_price_creation_saves_the_initial_buy_policy() {
    let mut input = creation();
    let mut terms = inputs();
    terms.buy_policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *terms.namespace.value(),
        sale_policy_id: *lbp_core::compute_pool_pda(
            terms.namespace,
            terms.lbp_program_id,
            factory_core::compute_definition_pda(terms.namespace, id(7), terms.launch_salt),
            terms.collateral_definition_id,
            input.receiver.account_id,
        )
        .value(),
        root: [9; 32],
    });
    input.message = borsh::to_vec(&Message::CreateFixedPriceLbp {
        inputs: terms.clone(),
    })
    .unwrap();
    let response = handle(&input).unwrap();
    assert_eq!(
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        terms.buy_policy
    );
}

#[test]
fn fixed_price_creation_rejects_unbound_or_unsupported_buy_policy() {
    for case in 0..3 {
        let mut input = creation();
        let mut terms = inputs();
        terms.buy_policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
            version: if case == 0 { 2 } else { 1 },
            namespace: if case == 1 {
                [99; 32]
            } else {
                *terms.namespace.value()
            },
            sale_policy_id: if case == 2 {
                [99; 32]
            } else {
                *lbp_core::compute_pool_pda(
                    terms.namespace,
                    terms.lbp_program_id,
                    factory_core::compute_definition_pda(terms.namespace, id(7), terms.launch_salt),
                    terms.collateral_definition_id,
                    input.receiver.account_id,
                )
                .value()
            },
            root: [9; 32],
        });
        input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
        assert_eq!(
            handle(&input).err(),
            Some(Error::InvalidTerms),
            "case {case}"
        );
    }
}

fn allowlisted_prepared() -> (ReceiveInput, lee_core::program::Response) {
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
    let mut response = handle(&input).unwrap();
    for bytes in [namespace_state(), collateral()] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap();
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
        (input, response) = continue_sale(&input, &response, stage, source, inventory, escrow);
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
    (input, response)
}

#[test]
fn fixed_price_setup_rejects_policy_downgrades_and_replacements_on_every_retry() {
    let (input, prepared) = allowlisted_prepared();
    for case in 0..3 {
        let (mut input, mut response) = continue_sale(
            &input,
            &prepared,
            factory_core::FixedPriceCreationStage::PoolPrepared,
            0,
            800,
            200,
        );
        let state =
            factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap())
                .unwrap();
        let mut pool =
            lbp_core::PoolAccount::decode(&prepared_pool(&state, input.receiver)).unwrap();
        match case {
            0 => pool.buy_policy = curve_core::BuyPolicy::Open,
            1 => {
                let curve_core::BuyPolicy::Allowlist(policy) = &mut pool.buy_policy else {
                    unreachable!()
                };
                policy.root = [99; 32];
            }
            _ => {}
        }
        let values = [
            borsh::to_vec(&pool).unwrap(),
            holding(state.pool_id, state.token_definition_id, 0),
            holding(state.pool_id, state.collateral_definition_id, 0),
        ];
        for (slot, bytes) in values.into_iter().enumerate() {
            input = reply(&input, &response, bytes);
            let result = handle(&input);
            if slot == 2 && case < 2 {
                assert_eq!(result.err(), Some(Error::InvalidState), "case {case}");
                break;
            }
            response = result.unwrap();
        }
        if case == 2 {
            let complete =
                factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap())
                    .unwrap();
            assert_eq!(
                complete.creation_stage,
                factory_core::FixedPriceCreationStage::Complete
            );
            assert_eq!(complete.buy_policy, state.buy_policy);
        }
    }
}

#[test]
fn fixed_price_allocation_rejects_a_different_pool_buy_policy() {
    let (input, prepared) = allowlisted_prepared();
    let mut state =
        factory_core::FixedPriceLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = factory_core::FixedPriceCreationStage::Complete;
    let mut pool = active_pool(&state, input.receiver);
    pool.buy_policy = curve_core::BuyPolicy::Open;
    assert_eq!(
        claim_allocation(&input, &state, &pool, 200, 200).err(),
        Some(Error::InvalidState)
    );
}

#[test]
fn fixed_price_pause_policy_survives_every_setup_stage_and_rejects_substitution() {
    for policy in [
        curve_core::PausePolicy::Disabled,
        curve_core::PausePolicy::AuthorityControlled,
    ] {
        let mut input = creation();
        let mut terms = inputs();
        terms.pause_policy = policy;
        input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
        let mut mint = handle(&input).unwrap();
        for bytes in [namespace_state(), collateral()] {
            assert_eq!(
                factory_core::LbpLaunchState::decode(mint.post_state.as_ref().unwrap())
                    .unwrap()
                    .pause_policy,
                policy
            );
            input = reply(&input, &mint, bytes);
            mint = handle(&input).unwrap();
        }
        let (input, allocated) = continue_sale(
            &input,
            &mint,
            factory_core::LbpCreationStage::Minted,
            1000,
            0,
            0,
        );
        assert_eq!(
            factory_core::LbpLaunchState::decode(allocated.post_state.as_ref().unwrap())
                .unwrap()
                .pause_policy,
            policy
        );
        let (input, prepared) = continue_sale(
            &input,
            &allocated,
            factory_core::LbpCreationStage::Allocated,
            0,
            800,
            200,
        );
        let state =
            factory_core::LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(state.pause_policy, policy);
        let call = prepared
            .sync_messages
            .iter()
            .find(|call| call.to.account_id == state.pool_id)
            .unwrap();
        let lbp_core::Message::PrepareFixedPrice { inputs } =
            borsh::from_slice(&call.message).unwrap()
        else {
            panic!("prepare")
        };
        assert_eq!(inputs.pause_policy, policy);
        for control in [
            curve_core::TradeControl::Unrestricted,
            curve_core::TradeControl::Running,
            curve_core::TradeControl::Paused,
        ] {
            let (mut input, mut response) = continue_sale(
                &input,
                &prepared,
                factory_core::LbpCreationStage::PoolPrepared,
                0,
                800,
                200,
            );
            let mut pool =
                lbp_core::PoolAccount::decode(&prepared_pool(&state, input.receiver)).unwrap();
            pool.trade_control = control;
            let values = [
                borsh::to_vec(&pool).unwrap(),
                holding(state.pool_id, state.token_definition_id, 0),
                holding(state.pool_id, state.collateral_definition_id, 0),
            ];
            for (slot, bytes) in values.into_iter().enumerate() {
                input = reply(&input, &response, bytes);
                let result = handle(&input);
                if slot == 2 && control.policy() != policy {
                    assert_eq!(result.err(), Some(Error::InvalidState));
                    break;
                }
                response = result.unwrap();
                if slot == 2 {
                    let complete =
                        factory_core::LbpLaunchState::decode(response.post_state.as_ref().unwrap())
                            .unwrap();
                    assert_eq!(
                        complete.creation_stage,
                        factory_core::LbpCreationStage::Complete
                    );
                    assert_eq!(complete.pause_policy, policy);
                }
            }
        }
    }
}

#[test]
fn paused_fixed_price_allocation_uses_immutable_policy_and_keeps_close_bounds() {
    let (input, mut state) = completed();
    state.pause_policy = curve_core::PausePolicy::AuthorityControlled;
    let mut pool = active_pool(&state, input.receiver);
    pool.trade_control = curve_core::TradeControl::Paused;
    let response = claim_allocation(&input, &state, &pool, 0, 200).unwrap();
    assert_eq!(response.validity.timestamps.start(), Some(200_000));
    pool.trade_control = curve_core::TradeControl::Unrestricted;
    assert_eq!(
        claim_allocation(&input, &state, &pool, 200, 200).err(),
        Some(Error::InvalidState)
    );
}

#[test]
fn shared_pause_requires_the_sale_creator_on_every_call_before_pool_forwarding() {
    let (input, mut state) = completed();
    state.pause_policy = curve_core::PausePolicy::AuthorityControlled;
    assert!(matches!(state.mode, factory_core::LbpSetupMode::FixedPrice));
    lbp_pause_forwarding::assert_creator_authority_and_pool_forwarding(input, state);
}

#[test]
fn legacy_weighted_pause_does_not_route_to_fixed_price_sales() {
    let (mut input, state) = completed();
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SetWeightedPaused {
        creator: id(42),
        paused: true,
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(Error::InvalidState));
}

#[test]
fn fixed_price_creation_checks_every_ending_and_deadline_combination() {
    use factory_core::EndingPolicy;
    for ending in [
        EndingPolicy::Manual,
        EndingPolicy::Scheduled,
        EndingPolicy::Depletion,
        EndingPolicy::Either,
    ] {
        for end in [None, Some(200)] {
            let mut terms = inputs();
            terms.ending_policy = ending;
            terms.end_timestamp = end;
            let mut input = creation();
            input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
            let result = handle(&input);
            if (ending == EndingPolicy::Depletion && end.is_none())
                || (matches!(ending, EndingPolicy::Scheduled | EndingPolicy::Either)
                    && end.is_some())
            {
                let state = factory_core::LbpLaunchState::decode(
                    result.unwrap().post_state.as_ref().unwrap(),
                )
                .unwrap();
                assert_eq!(state.ending_policy, ending);
            } else {
                assert_eq!(result.err(), Some(Error::InvalidTerms));
            }
        }
    }
}

#[test]
fn fixed_price_rejects_malformed_saved_terms_and_the_old_record_version() {
    let (input, response) = minted();
    let state =
        factory_core::LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    for case in 0..4 {
        let mut changed = state.clone();
        match case {
            0 => changed.ending_policy = factory_core::EndingPolicy::Manual,
            1 => changed.ending_policy = factory_core::EndingPolicy::Depletion,
            2 => changed.end_timestamp = Some(u64::MAX),
            _ => changed.start_timestamp = u64::MAX,
        }
        let mut request = input.clone();
        request.pre_state = borsh::to_vec(&changed).unwrap().into();
        request.message = borsh::to_vec(&Message::ContinueFixedPriceCreation {
            expected_stage: factory_core::LbpCreationStage::Minted,
            creator: id(42),
            now: 0,
        })
        .unwrap();
        assert_eq!(
            factory_core::LbpLaunchState::decode(&request.pre_state).err(),
            Some(Error::InvalidState)
        );
        assert_eq!(handle(&request).err(), Some(Error::InvalidState));
    }
    let mut bytes = borsh::to_vec(&state).unwrap();
    let version_offset =
        borsh::to_vec(&(state.buy_policy, state.pause_policy, state.ending_policy))
            .unwrap()
            .len()
            + 8;
    assert_eq!(
        &bytes[version_offset..version_offset + 2],
        &3_u16.to_le_bytes()
    );
    bytes[version_offset..version_offset + 2].copy_from_slice(&2_u16.to_le_bytes());
    assert_eq!(
        factory_core::LbpLaunchState::decode(&bytes).err(),
        Some(Error::InvalidState)
    );
}

#[test]
fn fixed_price_pending_callbacks_reject_malformed_saved_policies() {
    let input = creation();
    let response = handle(&input).unwrap();
    let mut state =
        factory_core::LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    state.ending_policy = factory_core::EndingPolicy::Manual;
    let mut next = reply(&input, &response, namespace_state());
    next.pre_state = borsh::to_vec(&state).unwrap().into();
    assert_eq!(handle(&next).err(), Some(Error::InvalidState));
}

#[test]
fn fixed_price_setup_and_allocation_reject_changed_pool_policies() {
    let (input, prepared) = allowlisted_prepared();
    let state =
        factory_core::LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    for case in 0..2 {
        let (mut next, mut response) = continue_sale(
            &input,
            &prepared,
            factory_core::LbpCreationStage::PoolPrepared,
            0,
            800,
            200,
        );
        let mut pool =
            lbp_core::PoolAccount::decode(&prepared_pool(&state, input.receiver)).unwrap();
        if case == 0 {
            pool.ending_policy = factory_core::EndingPolicy::Manual;
        } else {
            pool.ending_policy = factory_core::EndingPolicy::Scheduled;
        }
        let values = [
            borsh::to_vec(&pool).unwrap(),
            holding(state.pool_id, state.token_definition_id, 0),
            holding(state.pool_id, state.collateral_definition_id, 0),
        ];
        for (slot, bytes) in values.into_iter().enumerate() {
            next = reply(&next, &response, bytes);
            let result = handle(&next);
            if slot == 2 {
                assert_eq!(result.err(), Some(Error::InvalidCustody));
            } else {
                response = result.unwrap();
            }
        }
        let mut complete = state.clone();
        complete.creation_stage = factory_core::LbpCreationStage::Complete;
        pool.funded = true;
        assert_eq!(
            claim_allocation(&input, &complete, &pool, 200, 200).err(),
            Some(Error::InvalidCustody)
        );
    }
}

#[test]
fn fixed_allocation_and_retries_keep_the_selected_ending_bound() {
    let (input, original) = completed();
    for ending in [
        factory_core::EndingPolicy::Scheduled,
        factory_core::EndingPolicy::Depletion,
        factory_core::EndingPolicy::Either,
    ] {
        let mut state = original.clone();
        state.ending_policy = ending;
        state.end_timestamp = if ending == factory_core::EndingPolicy::Depletion {
            None
        } else {
            Some(200)
        };
        let mut pool = active_pool(&state, input.receiver);
        pool.remaining_inventory = 0;
        for released in [false, true] {
            state.creator_allocation_released = released;
            let response =
                claim_allocation(&input, &state, &pool, 100, if released { 0 } else { 200 })
                    .unwrap();
            assert_eq!(
                response.validity.timestamps.start(),
                if ending == factory_core::EndingPolicy::Scheduled {
                    Some(200_000)
                } else {
                    None
                }
            );
            assert_eq!(response.sync_messages.len(), usize::from(!released));
        }
    }
}

#[test]
fn released_after_setup_allocation_retry_stays_available_while_fixed_sale_is_open() {
    let (input, mut state) = completed();
    state.allocation_release = CreatorAllocationRelease::AfterSetup;
    state.creator_allocation_released = true;
    state.ending_policy = factory_core::EndingPolicy::Depletion;
    state.end_timestamp = None;
    let pool = active_pool(&state, input.receiver);
    assert_eq!(
        pool.close_time_bound().err(),
        Some(lbp_core::Error::NotClosed)
    );
    let mut request = input.clone();
    request.from = None;
    request.pre_state = borsh::to_vec(&state).unwrap().into();
    request.message = borsh::to_vec(&Message::ClaimCreatorAllocation {
        creator: id(42),
        now: 100,
    })
    .unwrap();
    let response = handle(&request).unwrap();
    let mut callback = authorized(&request, &response, id(42));
    callback.from = None;
    assert_eq!(handle(&callback).err(), Some(Error::Unauthorized));
    let repeated = handle(&authorized(&request, &response, id(42))).unwrap();
    assert!(repeated.sync_messages.is_empty());
    assert_eq!(repeated.validity.timestamps.start(), None);
    assert_eq!(
        factory_core::LbpLaunchState::decode(repeated.post_state.as_ref().unwrap()).unwrap(),
        state
    );
}
