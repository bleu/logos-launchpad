use super::*;

#[test]
fn scheduled_weighted_allocation_keeps_deadline_and_settlement_delegates_to_pool() {
    let (input, prepared, bytes, _) = prepared(CreatorAllocationRelease::AfterEnd);
    let mut state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = LbpCreationStage::Complete;
    let mut pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    pool.funded = true;
    pool.remaining_inventory = 0;
    pool.recorded_collateral = 100;
    if let lbp_core::PoolMode::Weighted {
        pricing_collateral, ..
    } = &mut pool.mode
    {
        *pricing_collateral = 100;
    }
    for released in [false, true] {
        state.creator_allocation_released = released;
        let mut request = input.clone();
        request.from = None;
        request.pre_state = borsh::to_vec(&state).unwrap().into();
        request.message =
            borsh::to_vec(&Message::ClaimWeightedAllocation { creator: id(42) }).unwrap();
        let response = handle(&request).unwrap();
        request = weighted_authorized(&request, &response);
        let mut response = handle(&request).unwrap();
        {
            for bytes in [
                borsh::to_vec(&pool).unwrap(),
                holding(state.creator_escrow_id, state.token_definition_id, 200),
            ] {
                request = reply(&request, &response, bytes);
                response = handle(&request).unwrap();
            }
        }
        assert_eq!(response.validity.timestamps.start(), Some(300_000));
        assert!(!response.validity.timestamps.is_valid_for(299_999));
        assert!(response.validity.timestamps.is_valid_for(300_000));
        assert_eq!(response.sync_messages.len(), usize::from(!released));
        if released {
            assert!(response.events.is_empty());
        } else {
            assert!(matches!(
                &launch_events(&response)[..],
                [SaleEventV1 {
                    kind: SaleEventKindV1::AllocationReleased {
                        amount: 200,
                        release: AllocationReleaseV1::AfterEnd,
                        ..
                    },
                    ..
                }]
            ));
        }
    }
    let mut request = input;
    request.from = None;
    request.pre_state = borsh::to_vec(&state).unwrap().into();
    request.message =
        borsh::to_vec(&Message::WithdrawWeightedProceeds { creator: id(42) }).unwrap();
    let response = handle(&request).unwrap();
    request = weighted_authorized(&request, &response);
    let response = handle(&request).unwrap();
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&response.sync_messages[0].message).unwrap(),
        lbp_core::Message::ReadState
    );
    request = reply(&request, &response, borsh::to_vec(&pool).unwrap());
    let response = handle(&request).unwrap();
    assert_eq!(response.validity.timestamps.start(), Some(300_000));
    assert!(!response.validity.timestamps.is_valid_for(299_999));
    assert!(response.validity.timestamps.is_valid_for(300_000));
    assert_eq!(response.sync_messages.len(), 3);
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&response.sync_messages.last().unwrap().message)
            .unwrap(),
        lbp_core::Message::Settle {
            route: factory_core::SettlementRoute::Manual,
            creator: id(42),
            adapter: None
        }
    );
}

#[test]
fn weighted_allocation_and_retries_use_selected_close_without_releasing_schedule_dust() {
    for ending in [
        factory_core::EndingPolicy::Scheduled,
        factory_core::EndingPolicy::Either,
    ] {
        let mut terms = inputs();
        terms.ending_policy = ending;
        let (input, prepared, bytes, _) = prepared_with_terms(terms);
        let mut state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
        state.creation_stage = LbpCreationStage::Complete;
        let original = lbp_core::PoolAccount::decode(&bytes).unwrap();
        for remaining in [0, 1] {
            let mut pool = original.clone();
            pool.funded = true;
            pool.remaining_inventory = remaining;
            pool.recorded_collateral = 100;
            if let lbp_core::PoolMode::Weighted {
                pricing_collateral, ..
            } = &mut pool.mode
            {
                *pricing_collateral = 100;
            }
            for released in [false, true] {
                state.creator_allocation_released = released;
                let mut request = input.clone();
                request.from = None;
                request.pre_state = borsh::to_vec(&state).unwrap().into();
                request.message =
                    borsh::to_vec(&Message::ClaimWeightedAllocation { creator: id(42) }).unwrap();
                let response = handle(&request).unwrap();
                request = weighted_authorized(&request, &response);
                let mut response = handle(&request).unwrap();
                for bytes in [
                    borsh::to_vec(&pool).unwrap(),
                    holding(
                        state.creator_escrow_id,
                        state.token_definition_id,
                        if released { 0 } else { 200 },
                    ),
                ] {
                    request = reply(&request, &response, bytes);
                    response = handle(&request).unwrap();
                }
                let expected = if ending == factory_core::EndingPolicy::Either && remaining == 0 {
                    None
                } else {
                    Some(300_000)
                };
                assert_eq!(response.validity.timestamps.start(), expected);
                assert_eq!(response.sync_messages.len(), usize::from(!released));
                assert!(
                    LbpLaunchState::decode(response.post_state.as_ref().unwrap())
                        .unwrap()
                        .creator_allocation_released
                );
            }
        }
    }
}

#[test]
fn released_after_setup_allocation_retry_stays_available_while_weighted_sale_is_open() {
    let (input, prepared, bytes, _) = prepared(CreatorAllocationRelease::AfterSetup);
    let mut state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = LbpCreationStage::Complete;
    state.creator_allocation_released = true;
    let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    assert!(pool.remaining_inventory > 0);
    assert_eq!(pool.close_time_bound().unwrap(), Some(300));
    let mut request = input.clone();
    request.from = None;
    request.pre_state = borsh::to_vec(&state).unwrap().into();
    request.message = borsh::to_vec(&Message::ClaimWeightedAllocation { creator: id(42) }).unwrap();
    let response = handle(&request).unwrap();
    let mut callback = weighted_authorized(&request, &response);
    callback.from = None;
    assert_eq!(handle(&callback).err(), Some(Error::Unauthorized));
    let repeated = handle(&weighted_authorized(&request, &response)).unwrap();
    assert!(repeated.sync_messages.is_empty());
    assert_eq!(repeated.validity.timestamps.start(), None);
    assert_eq!(
        LbpLaunchState::decode(repeated.post_state.as_ref().unwrap()).unwrap(),
        state
    );
}
