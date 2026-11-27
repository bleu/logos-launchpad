use super::*;

#[test]
fn after_end_allocation_claim_at_expiry_does_not_settle_proceeds() {
    let (input, state) = completed();
    let pool = active_pool(&state, input.receiver);
    let response = claim_allocation(&input, &state, &pool, 999, 217).unwrap();
    assert!(
        matches!(&launch_events(&response)[..], [SaleEventV1 { kind: SaleEventKindV1::AllocationReleased { amount: 200, release: AllocationReleaseV1::AfterEnd, recipient, .. }, .. }] if recipient == id(42).value())
    );
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
    assert!(repeated.events.is_empty());
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
            pool.settlement_progress = if settled {
                lbp_core::SettlementProgress::ManualComplete
            } else {
                lbp_core::SettlementProgress::Unselected
            };
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
fn inventory_payout_keeps_the_allocation_inclusion_bound_until_expiry() {
    let (input, state) = completed();
    let mut pool = active_pool(&state, input.receiver);
    pool.settlement_progress = lbp_core::SettlementProgress::ManualComplete;
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
    state.total_supply = state.sale_inventory + state.graduation_reserve;
    let pool = active_pool(&state, input.receiver);
    let response = claim_allocation(&input, &state, &pool, 200, 0).unwrap();
    assert!(
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .creator_allocation_released
    );
    assert!(response.sync_messages.is_empty());
    assert!(response.events.is_empty());
    assert_eq!(response.validity.timestamps.start(), Some(200_000));
    assert_eq!(response.validity.timestamps.end(), None);
    state.creator_allocation = 200;
    state.total_supply = state.sale_inventory + state.graduation_reserve + state.creator_allocation;
    state.allocation_release = CreatorAllocationRelease::AfterSetup;
    state.creator_allocation_released = true;
    let pool = active_pool(&state, input.receiver);
    let response = claim_allocation(&input, &state, &pool, 100, 0).unwrap();
    assert!(response.sync_messages.is_empty());
    assert!(response.events.is_empty());
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
    for case in 0..16 {
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
            13 => changed.funded = false,
            14 => changed.trade_mode = factory_core::TradeMode::BuyAndSell,
            _ => changed.sell_fee_bps = 1,
        }
        let expected = if case >= 14 {
            Error::InvalidCustody
        } else if case == 13 {
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

#[test]
fn fixed_price_allocation_rejects_a_different_pool_buy_policy() {
    let (input, prepared, _) = allowlisted_prepared();
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
