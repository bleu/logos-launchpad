use super::*;

#[test]
fn allocation_checks_creator_role_and_splits_the_exact_mint() {
    let (_, mint, setup) = minted();
    let mut input = setup.continuation(&mint, factory_core::FixedPriceCreationStage::Minted);
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

#[test]
fn pool_preparation_uses_saved_terms_and_separate_setup_stage() {
    let (_, mint, setup) = minted();
    let (_, allocated) = continue_sale(
        &setup,
        &mint,
        factory_core::FixedPriceCreationStage::Minted,
        1000,
        0,
        0,
    );
    let (input, response) = continue_sale(
        &setup,
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

#[test]
fn final_setup_funds_inventory_and_keeps_after_end_allocation_locked() {
    let (_, mint, setup) = minted();
    let (_, allocated) = continue_sale(
        &setup,
        &mint,
        factory_core::FixedPriceCreationStage::Minted,
        1000,
        0,
        0,
    );
    let (_, prepared) = continue_sale(
        &setup,
        &allocated,
        factory_core::FixedPriceCreationStage::Allocated,
        0,
        800,
        200,
    );
    let (mut input, mut response) = continue_sale(
        &setup,
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
    assert!(response.events.is_empty());
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
    let (_, response, setup) = mint_from(&input);
    let (_, allocated) = continue_sale(
        &setup,
        &response,
        factory_core::FixedPriceCreationStage::Minted,
        1000,
        0,
        0,
    );
    let (_, prepared) = continue_sale(
        &setup,
        &allocated,
        factory_core::FixedPriceCreationStage::Allocated,
        0,
        800,
        200,
    );
    let (mut input, mut response) = continue_sale(
        &setup,
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
    assert!(
        matches!(&launch_events(&response)[..], [SaleEventV1 { kind: SaleEventKindV1::AllocationReleased { amount: 200, release: AllocationReleaseV1::AfterSetup, recipient, .. }, .. }] if recipient == id(42).value())
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
        let (_, mint, setup) = mint_from(&input);
        let (_, allocated) = continue_sale(
            &setup,
            &mint,
            factory_core::FixedPriceCreationStage::Minted,
            1000,
            0,
            0,
        );
        let (_, prepared) = continue_sale(
            &setup,
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
            &setup,
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
fn fixed_price_state_read_returns_raw_state_only_from_the_canonical_sale_actor() {
    let (mut input, mint, _) = minted();
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
    use factory_core::FixedPriceCreationStage::{Allocated, Minted};
    let (_, mint, setup) = minted();
    setup.assert_rejects_external_continuation(&mint);
    let stale = setup.continuation(&mint, Allocated);
    assert!(matches!(handle(&stale), Err(Error::StaleStage)));
    let mut input = setup.continuation(&mint, Minted);
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
        let (_, mint, setup) = minted();
        let mut input = setup.continuation(&mint, factory_core::FixedPriceCreationStage::Minted);
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
