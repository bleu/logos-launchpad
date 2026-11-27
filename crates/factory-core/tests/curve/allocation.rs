use super::*;

#[test]
fn allocation_uses_saved_closure_instead_of_a_future_client_time() {
    for (lifecycle, expected) in [
        (pool::PoolLifecycle::Open, Some(400_000)),
        (pool::PoolLifecycle::Closed, None),
    ] {
        let (mut input, active) = active();
        let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
        let mut saved = pool_state(&state, input.receiver, true);
        saved.pool.lifecycle = lifecycle;
        input.pre_state = active.post_state.unwrap();
        input.message = borsh::to_vec(&Message::ClaimCreatorAllocation {
            creator: id(42),
            now: 999,
        })
        .unwrap();
        let response = handle(&input).unwrap();
        input = proof(&input, &response, id(42));
        let mut response = handle(&input).unwrap();
        for bytes in [
            borsh::to_vec(&saved).unwrap(),
            holding(state.creator_escrow_id, state.token_definition_id, 100),
        ] {
            input = reply(&input, &response, bytes);
            response = handle(&input).unwrap();
        }
        assert_eq!(response.validity.timestamps.start(), expected);
        assert_eq!(response.validity.timestamps.end(), None);
    }
}

#[test]
fn creator_allocation_follows_the_role_to_its_current_holder_after_closure() {
    let (mut input, active) = active();
    let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    input.pre_state = active.post_state.unwrap();
    input.message = borsh::to_vec(&Message::ClaimCreatorAllocation {
        creator: id(99),
        now: 400,
    })
    .unwrap();
    let response = handle(&input).unwrap();
    assert_eq!(
        response.sync_messages.last().unwrap().to,
        Actor::new(id(99), AUTHORITY_PROGRAM_ACCOUNT_ID)
    );
    input = proof(&input, &response, id(99));
    let mut response = handle(&input).unwrap();
    for bytes in [
        borsh::to_vec(&pool_state(&state, input.receiver, true)).unwrap(),
        holding(state.creator_escrow_id, state.token_definition_id, 117),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    assert!(
        FactoryState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .creator_allocation_claimed
    );
    let payout = response
        .sync_messages
        .iter()
        .find(|m| m.to == Actor::new(state.creator_escrow_id, TOKEN_PROGRAM_ACCOUNT_ID))
        .unwrap();
    assert_eq!(
        borsh::from_slice::<token::Message>(&payout.message).unwrap(),
        token::Message::Transfer {
            definition: state.token_definition_id,
            recipient: id(99),
            amount: 100
        }
    );
    input.pre_state = response.post_state.unwrap();
    input.message = borsh::to_vec(&Message::ClaimCreatorAllocation {
        creator: id(99),
        now: 400,
    })
    .unwrap();
    assert!(matches!(handle(&input), Err(Error::AlreadyClaimed)));
}

#[test]
fn curve_allocation_rejects_changed_inner_pool_policy() {
    let (input, active) = active();
    let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    for case in 0..2 {
        let mut saved = pool_state(&state, input.receiver, true);
        if case == 0 {
            saved.pool.ending_policy = EndingPolicy::Manual;
            saved.pool.close_timestamp = None;
            saved.pool.close_on_depletion = None;
        } else {
            saved.pool.ending_policy = EndingPolicy::Scheduled;
            saved.pool.close_on_depletion = None;
        }
        let mut request = input.clone();
        request.pre_state = active.post_state.clone().unwrap();
        request.from = None;
        request.message = borsh::to_vec(&Message::ClaimCreatorAllocation {
            creator: id(42),
            now: 999,
        })
        .unwrap();
        let response = handle(&request).unwrap();
        request = proof(&request, &response, id(42));
        let mut response = handle(&request).unwrap();
        request = reply(&request, &response, borsh::to_vec(&saved).unwrap());
        response = handle(&request).unwrap();
        request = reply(
            &request,
            &response,
            holding(state.creator_escrow_id, state.token_definition_id, 100),
        );
        assert_eq!(handle(&request).err(), Some(Error::InvalidCustody));
    }
}

#[test]
fn curve_allocation_read_emits_only_actual_allocation_payment() {
    for (amount, self_recipient) in [(0, false), (100, false), (100, true)] {
        let (mut input, active) = active();
        let mut state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
        state.creator_allocation = amount;
        state.total_supply =
            total_supply(state.sale_reserve, state.graduation_reserve, amount).unwrap();
        let pool = pool_state(&state, input.receiver, true);
        let creator = if self_recipient {
            state.creator_escrow_id
        } else {
            id(42)
        };
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.from = None;
        input.message =
            borsh::to_vec(&Message::ClaimCreatorAllocation { creator, now: 0 }).unwrap();
        let response = handle(&input).unwrap();
        input = proof(&input, &response, creator);
        let mut response = handle(&input).unwrap();
        let mut values = vec![borsh::to_vec(&pool).unwrap()];
        if amount != 0 {
            values.push(holding(
                state.creator_escrow_id,
                state.token_definition_id,
                amount,
            ));
        }
        for bytes in values {
            input = reply(&input, &response, bytes);
            response = handle(&input).unwrap();
        }
        let events = launch_events(&response);
        if amount == 0 || self_recipient {
            assert!(events.is_empty());
        } else {
            assert!(matches!(
                &events[..],
                [SaleEventV1 {
                    kind: SaleEventKindV1::AllocationReleased {
                        amount: 100,
                        release: AllocationReleaseV1::AfterEnd,
                        ..
                    },
                    ..
                }]
            ));
        }
        assert_eq!(response.validity.timestamps.start(), Some(400_000));
    }
}
