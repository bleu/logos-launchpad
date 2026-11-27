use super::*;

#[test]
fn curve_creation_rejects_policy_for_another_pool() {
    let mut input = creation();
    let mut message: Message = borsh::from_slice(&input.message).unwrap();
    if let Message::CreateFactoryPool { buy_policy, .. } = &mut message {
        *buy_policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
            version: 1,
            namespace: *id(40).value(),
            sale_policy_id: [99; 32],
            root: [8; 32],
        });
    }
    input.message = borsh::to_vec(&message).unwrap();
    assert!(matches!(handle(&input), Err(Error::InvalidTerms)));
}

#[test]
fn curve_allocation_funds_only_sale_inventory_and_locks_creator_tokens() {
    let (_, mint, setup) = minted();
    let (input, response) = continued(&setup, &mint, 900, 0, 0);
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.creation_stage, CreationStage::Allocated);
    let transfers: Vec<_> = response
        .sync_messages
        .iter()
        .filter_map(|m| borsh::from_slice::<token::Message>(&m.message).ok())
        .filter(|m| matches!(m, token::Message::Transfer { .. }))
        .collect();
    assert_eq!(
        transfers,
        vec![
            token::Message::Transfer {
                definition: state.token_definition_id,
                recipient: input.receiver.account_id,
                amount: 800
            },
            token::Message::Transfer {
                definition: state.token_definition_id,
                recipient: state.creator_escrow_id,
                amount: 100
            }
        ]
    );
}

#[test]
fn curve_preparation_uses_neutral_restriction_and_defers_funding() {
    let (_, mint, setup) = minted();
    let (_, allocated) = continued(&setup, &mint, 900, 0, 0);
    let (input, response) = continued(&setup, &allocated, 0, 800, 100);
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.creation_stage, CreationStage::PoolPrepared);
    let call = response
        .sync_messages
        .iter()
        .find(|m| m.to == Actor::new(input.receiver.account_id, id(6)))
        .unwrap();
    assert_eq!(
        call.pda_seeds,
        [compute_factory_seed(state.namespace, state.launch_salt)].into()
    );
    let curve_core::Message::CreatePool {
        allowed_input_token,
        defer_funding,
        token0_amount,
        token1_amount,
        owner_program_id,
        close_on_depletion,
        ending_policy,
        ..
    } = borsh::from_slice(&call.message).unwrap()
    else {
        panic!("prepare")
    };
    assert_eq!(allowed_input_token, Some(1));
    assert!(defer_funding);
    assert_eq!((token0_amount, token1_amount), (800, 0));
    assert_eq!(owner_program_id, Some(id(7)));
    assert_eq!(close_on_depletion, Some(0));
    assert_eq!(ending_policy, EndingPolicy::Either);
}

fn allowlist_policy() -> curve_core::BuyPolicy {
    let factory = compute_factory_pda(id(40), id(7), [3; 32]);
    let pool = curve_core::compute_pool_pda(
        id(40),
        id(6),
        compute_definition_pda(id(40), id(7), [3; 32]),
        id(41),
        factory,
    );
    curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *id(40).value(),
        sale_policy_id: *pool.value(),
        root: [8; 32],
    })
}

#[test]
fn curve_activation_rejects_a_replaced_or_downgraded_pool_policy() {
    let policy = allowlist_policy();
    let (_, mint, setup) = minted_with_policy(policy.clone());
    let (_, allocated) = continued(&setup, &mint, 900, 0, 0);
    let (_, prepared) = continued(&setup, &allocated, 0, 800, 100);
    let (input, response) = continued(&setup, &prepared, 0, 800, 100);
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    let curve_core::BuyPolicy::Allowlist(mut replacement) = policy else {
        unreachable!()
    };
    replacement.root = [9; 32];
    for policy in [
        curve_core::BuyPolicy::Open,
        curve_core::BuyPolicy::Allowlist(replacement),
    ] {
        let mut saved = pool_state(&state, input.receiver, false);
        saved.buy_policy = policy;
        let next = reply(&input, &response, borsh::to_vec(&saved).unwrap());
        let next_response = handle(&next).unwrap();
        let next = reply(
            &next,
            &next_response,
            holding(state.pool_id, state.token_definition_id, 0),
        );
        assert!(matches!(handle(&next), Err(Error::InvalidState)));
    }
}

#[test]
fn curve_allowlist_policy_is_unchanged_through_all_creation_stages() {
    let policy = allowlist_policy();
    let (_, mint, setup) = minted_with_policy(policy.clone());
    assert_eq!(
        FactoryState::decode(mint.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        policy
    );
    let (_, allocated) = continued(&setup, &mint, 900, 0, 0);
    assert_eq!(
        FactoryState::decode(allocated.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        policy
    );
    let (_, prepared) = continued(&setup, &allocated, 0, 800, 100);
    assert_eq!(
        FactoryState::decode(prepared.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        policy
    );
    let create = prepared
        .sync_messages
        .iter()
        .find_map(|call| borsh::from_slice::<curve_core::Message>(&call.message).ok())
        .unwrap();
    assert!(
        matches!(create, curve_core::Message::CreatePool { buy_policy, .. } if buy_policy == policy)
    );
    let (mut input, mut response) = continued(&setup, &prepared, 0, 800, 100);
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    for bytes in [
        borsh::to_vec(&pool_state(&state, input.receiver, false)).unwrap(),
        holding(state.pool_id, state.token_definition_id, 0),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap();
    }
    let active = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(active.creation_stage, CreationStage::Active);
    assert_eq!(active.buy_policy, policy);
    assert_eq!(active.ending_policy, EndingPolicy::Either);
}

#[test]
fn curve_activation_grants_funding_only_to_the_factory_token_call() {
    let (input, response) = active();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.creation_stage, CreationStage::Active);
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(input.receiver.account_id, TOKEN_PROGRAM_ACCOUNT_ID)
    );
    assert_eq!(
        borsh::from_slice::<token::Message>(&response.sync_messages[0].message).unwrap(),
        token::Message::Transfer {
            definition: state.token_definition_id,
            recipient: state.pool_id,
            amount: 800
        }
    );
    assert_eq!(
        response.sync_messages[1].to,
        Actor::new(state.pool_id, id(6))
    );
    assert_eq!(
        borsh::from_slice::<curve_core::Message>(&response.sync_messages[1].message).unwrap(),
        curve_core::Message::ActivatePool { now: 100 }
    );
}

#[test]
fn public_supply_and_address_rules_preserve_the_original_factory_contract() {
    assert_eq!(total_supply(800, 0, 100), Ok(900));
    assert_eq!(total_supply(0, 0, 100), Err(CreateError::SaleReserveZero));
    assert_eq!(
        total_supply(u128::MAX, 0, 1),
        Err(CreateError::SupplyOverflow)
    );
    assert_eq!(
        validate_curve_parameters(800, 800, 100),
        Err(CreateError::VirtualTokenReserveNotAboveSaleReserve)
    );
    assert_eq!(
        validate_curve_parameters(800, 1000, 0),
        Err(CreateError::VirtualReserveZero)
    );
    assert_eq!(
        validate_curve_parameters(800, pool::VIRTUAL_RESERVE_BOUND, 100),
        Err(CreateError::VirtualReserveAboveBound)
    );
    assert!(TradeMode::try_from(2).is_err());
    let mut invalid_mode = creation();
    let mut message: Message = borsh::from_slice(&invalid_mode.message).unwrap();
    let Message::CreateFactoryPool { trade_mode, .. } = &mut message else {
        unreachable!()
    };
    *trade_mode = 2;
    invalid_mode.message = borsh::to_vec(&message).unwrap();
    assert_eq!(handle(&invalid_mode).err(), Some(Error::InvalidTerms));
    let addresses = [
        compute_factory_pda(id(40), id(7), [3; 32]),
        compute_definition_pda(id(40), id(7), [3; 32]),
        compute_mint_pda(id(40), id(7), [3; 32]),
        compute_metadata_pda(id(40), id(7), [3; 32]),
        compute_escrow_pda(id(40), id(7), [3; 32]),
    ];
    for (index, address) in addresses.iter().enumerate() {
        assert!(!addresses[..index].contains(address));
    }
    assert_ne!(
        compute_factory_pda(id(40), id(7), [3; 32]),
        compute_factory_pda(id(41), id(7), [3; 32])
    );
    assert_ne!(
        compute_factory_pda(id(40), id(7), [3; 32]),
        compute_factory_pda(id(40), id(7), [4; 32])
    );
    assert_ne!(
        compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
        compute_curve_creator_role_pda(id(40), id(7), [4; 32])
    );
}

#[test]
fn curve_creation_queues_the_namespace_read_before_its_continuations() {
    let input = creation();
    let response = handle(&input).unwrap();
    Creation::new(&input, &response);
}

#[test]
fn curve_creation_wire_rejects_the_removed_creator_nft_argument() {
    let mut input = creation();
    let legacy_argument = [43; 32];
    input.message.splice(
        input.message.len() - 8..input.message.len() - 8,
        legacy_argument,
    );
    assert!(matches!(handle(&input), Err(Error::InvalidMessage)));
}

#[test]
fn curve_creation_checks_the_ending_matrix_before_reads() {
    for ending in [
        EndingPolicy::Manual,
        EndingPolicy::Scheduled,
        EndingPolicy::Depletion,
        EndingPolicy::Either,
    ] {
        for end in [None, Some(400)] {
            let mut input = creation();
            let mut command: Message = borsh::from_slice(&input.message).unwrap();
            if let Message::CreateFactoryPool {
                ending_policy,
                end_timestamp,
                ..
            } = &mut command
            {
                *ending_policy = ending;
                *end_timestamp = end;
            }
            input.message = borsh::to_vec(&command).unwrap();
            let result = handle(&input);
            let valid = match ending {
                EndingPolicy::Manual => false,
                EndingPolicy::Scheduled | EndingPolicy::Either => end.is_some(),
                EndingPolicy::Depletion => end.is_none(),
            };
            if valid {
                let state =
                    FactoryState::decode(result.unwrap().post_state.as_ref().unwrap()).unwrap();
                assert_eq!((state.ending_policy, state.end_timestamp), (ending, end));
            } else {
                assert_eq!(result.err(), Some(Error::InvalidTerms));
            }
        }
    }
}

#[test]
fn curve_timestamp_overflow_rejects_before_setup_reads_or_minting() {
    let mut input = creation();
    let mut command: Message = borsh::from_slice(&input.message).unwrap();
    if let Message::CreateFactoryPool { end_timestamp, .. } = &mut command {
        *end_timestamp = Some(u64::MAX);
    }
    input.message = borsh::to_vec(&command).unwrap();
    assert_eq!(handle(&input).err(), Some(Error::InvalidTerms));
}

#[test]
fn curve_rejects_malformed_saved_policies_before_resuming_setup() {
    let input = creation();
    let response = handle(&input).unwrap();
    let setup = Creation::new(&input, &response);
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    for case in 0..3 {
        let mut changed = state.clone();
        match case {
            0 => changed.ending_policy = EndingPolicy::Manual,
            1 => changed.ending_policy = EndingPolicy::Depletion,
            _ => changed.end_timestamp = Some(u64::MAX),
        }
        let bytes = borsh::to_vec(&changed).unwrap();
        assert_eq!(
            FactoryState::decode(&bytes).err(),
            Some(Error::InvalidState)
        );
        let mut input = setup.namespace_reply(&response, config());
        input.pre_state = bytes.into();
        assert_eq!(handle(&input).err(), Some(Error::InvalidState));
    }
}

#[test]
fn curve_activation_rejects_a_valid_direct_pool_ending_policy() {
    let (_, mint, setup) = minted();
    let (_, allocated) = continued(&setup, &mint, 900, 0, 0);
    let (_, prepared) = continued(&setup, &allocated, 0, 800, 100);
    let (mut input, mut response) = continued(&setup, &prepared, 0, 800, 100);
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    let mut saved = pool_state(&state, input.receiver, false);
    saved.pool.ending_policy = EndingPolicy::Manual;
    saved.pool.close_timestamp = None;
    saved.pool.close_on_depletion = None;
    input = reply(&input, &response, borsh::to_vec(&saved).unwrap());
    response = handle(&input).unwrap();
    input = reply(
        &input,
        &response,
        holding(state.pool_id, state.token_definition_id, 0),
    );
    assert_eq!(handle(&input).err(), Some(Error::InvalidCustody));
}

#[test]
fn curve_setup_keeps_each_selected_ending_and_its_depletion_side() {
    for ending in [
        EndingPolicy::Scheduled,
        EndingPolicy::Depletion,
        EndingPolicy::Either,
    ] {
        let mut input = creation();
        let mut command: Message = borsh::from_slice(&input.message).unwrap();
        let end = if ending == EndingPolicy::Depletion {
            None
        } else {
            Some(400)
        };
        if let Message::CreateFactoryPool {
            ending_policy,
            end_timestamp,
            ..
        } = &mut command
        {
            *ending_policy = ending;
            *end_timestamp = end;
        }
        input.message = borsh::to_vec(&command).unwrap();
        let (_, response, setup) = mint_from(&input);
        assert_eq!(
            response.validity.timestamps.end(),
            end.map(|end| end * 1000)
        );
        let (_, allocated) = continued(&setup, &response, 900, 0, 0);
        let (_, prepared) = continued(&setup, &allocated, 0, 800, 100);
        let message: curve_core::Message =
            borsh::from_slice(&prepared.sync_messages[0].message).unwrap();
        let expected_side = if ending == EndingPolicy::Scheduled {
            None
        } else {
            Some(0)
        };
        assert!(matches!(message, curve_core::Message::CreatePool {
            ending_policy, close_timestamp, close_on_depletion, token0_amount: 800, token1_amount: 0, ..
        } if ending_policy == ending && close_timestamp == end && close_on_depletion == expected_side));
        let (mut input, mut response) = continued(&setup, &prepared, 0, 800, 100);
        let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
        for bytes in [
            borsh::to_vec(&pool_state(&state, input.receiver, false)).unwrap(),
            holding(state.pool_id, state.token_definition_id, 0),
        ] {
            input = reply(&input, &response, bytes);
            response = handle(&input).unwrap();
        }
        let active = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(active.creation_stage, CreationStage::Active);
        assert_eq!(active.ending_policy, ending);
        assert_eq!(active.end_timestamp, end);
        assert_eq!(active.creator_allocation, 100);
    }
}

#[test]
fn curve_created_event_names_the_launch_and_immutable_terms_once() {
    let input = creation();
    let response = handle(&input).unwrap();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        launch_events(&response),
        vec![SaleEventV1::new(
            *state.namespace.value(),
            PricingEngineV1::Curve,
            EventSubjectV1::Launch {
                factory: *input.receiver.account_id.value(),
                pool: *state.pool_id.value()
            },
            SaleEventKindV1::Created {
                creator: *id(42).value(),
                creator_role: *state.creator_role_id.value(),
                token_definition: *state.token_definition_id.value(),
                collateral_definition: *state.collateral_definition_id.value(),
                graduation_reserve: state.graduation_reserve,
                sale_inventory: 800,
                creator_allocation: 100,
                graduation_policy: GraduationPolicy {
                    launch_bps: 0,
                    collateral_bps: 0
                },
                collateral_seed: 0,
                start_timestamp: None,
                end_timestamp: Some(400),
                policies: EventPoliciesV1 {
                    ending: EndingPolicy::Either,
                    pause: PausePolicy::Disabled,
                    sell_fee_bps: 0
                }
            }
        )]
    );
    let (_, minted, _) = minted();
    assert!(minted.events.is_empty());
    let mut replay = input;
    replay.pre_state = minted.post_state.unwrap();
    assert_eq!(handle(&replay).err(), Some(Error::AlreadyInitialized));
}

#[test]
fn curve_forwards_the_retained_fee_and_purchase_limit() {
    for rate in [0, 125, 9999] {
        let mut input = creation();
        let mut command: Message = borsh::from_slice(&input.message).unwrap();
        if let Message::CreateFactoryPool {
            sell_fee_bps,
            per_block_token_limit,
            ..
        } = &mut command
        {
            *sell_fee_bps = rate;
            *per_block_token_limit = Some(500);
        }
        input.message = borsh::to_vec(&command).unwrap();
        let initial = handle(&input).unwrap();
        let setup = Creation::new(&input, &initial);
        let input = setup.namespace_reply(&initial, config());
        let checked = handle(&input).unwrap();
        let mint = handle(&reply(&input, &checked, definition())).unwrap();
        for response in [&checked, &mint] {
            assert_eq!(
                FactoryState::decode(response.post_state.as_ref().unwrap())
                    .unwrap()
                    .sell_fee_bps,
                rate
            );
        }
        let (_, allocated) = continued(&setup, &mint, 900, 0, 0);
        let (_, prepared) = continued(&setup, &allocated, 0, 800, 100);
        let state = FactoryState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(state.sell_fee_bps, rate);
        assert_eq!(state.per_block_token_limit, Some(500));
        let prepare = prepared
            .sync_messages
            .iter()
            .find(|call| call.to == Actor::new(input.receiver.account_id, state.curve_program_id))
            .unwrap();
        assert!(
            matches!(borsh::from_slice::<curve_core::Message>(&prepare.message).unwrap(), curve_core::Message::CreatePool { sell_fee_bps, per_block_token_limit: Some(500), .. } if sell_fee_bps == rate)
        );
        for case in 0..5 {
            let (mut request, response) = continued(&setup, &prepared, 0, 800, 100);
            let mut saved = pool_state(&state, request.receiver, false);
            match case {
                0 => {}
                1 => saved.pool.sell_fee_bps = if rate == 0 { 1 } else { 0 },
                2 => {
                    saved.pool.k += 1;
                    saved.pool.virtual_reserve1 += 1;
                }
                3 => saved.pool.initial_k -= 1,
                _ => saved.block_allocation.limit = Some(501),
            }
            request = reply(&request, &response, borsh::to_vec(&saved).unwrap());
            let response = handle(&request).unwrap();
            request = reply(
                &request,
                &response,
                holding(state.pool_id, state.token_definition_id, 0),
            );
            let result = handle(&request);
            if case == 0 {
                assert_eq!(
                    FactoryState::decode(result.unwrap().post_state.as_ref().unwrap())
                        .unwrap()
                        .creation_stage,
                    CreationStage::Active
                );
            } else {
                assert!(result.is_err(), "rate {rate}, case {case}");
            }
        }
    }
}

#[test]
fn curve_creation_rejects_invalid_shares_and_shares_without_an_adapter() {
    for (hook, launch_bps, collateral_bps) in [
        (false, 1, 0),
        (false, 0, 1),
        (true, 10001, 0),
        (true, 0, 10001),
    ] {
        let mut input = creation();
        let mut command: Message = borsh::from_slice(&input.message).unwrap();
        if let Message::CreateFactoryPool {
            graduation_hook_program_id,
            graduation_policy,
            ..
        } = &mut command
        {
            *graduation_hook_program_id = hook.then_some(id(9));
            *graduation_policy = GraduationPolicy {
                launch_bps,
                collateral_bps,
            };
        }
        input.message = borsh::to_vec(&command).unwrap();
        assert_eq!(handle(&input).err(), Some(Error::InvalidTerms));
    }
}

#[test]
fn graduation_reserve_is_in_initial_supply_but_not_curve_pricing() {
    for reserve in [0, 120, u128::MAX] {
        let mut input = creation();
        let mut command: Message = borsh::from_slice(&input.message).unwrap();
        if let Message::CreateFactoryPool {
            graduation_reserve, ..
        } = &mut command
        {
            *graduation_reserve = reserve;
        }
        input.message = borsh::to_vec(&command).unwrap();
        let response = handle(&input);
        if reserve == u128::MAX {
            assert_eq!(response.err(), Some(Error::InvalidTerms));
        } else {
            let state =
                FactoryState::decode(response.unwrap().post_state.as_ref().unwrap()).unwrap();
            assert_eq!(state.graduation_reserve, reserve);
            assert_eq!(state.total_supply, 900 + reserve);
            assert_eq!(
                (
                    state.sale_reserve,
                    state.virtual_token_reserve,
                    state.virtual_collateral_reserve
                ),
                (800, 1000, 100)
            );
        }
    }
    assert_eq!(total_supply(1, u128::MAX - 2, 1), Ok(u128::MAX));
    assert_eq!(
        total_supply(1, u128::MAX - 2, 2),
        Err(CreateError::SupplyOverflow)
    );
}

#[test]
fn short_curve_sale_still_requires_inclusion_before_expiry() {
    for (now, accepted) in [
        (99, true),
        (100, true),
        (399, true),
        (400, false),
        (u64::MAX, false),
    ] {
        let mut input = creation();
        let mut message: Message = borsh::from_slice(&input.message).unwrap();
        if let Message::CreateFactoryPool { now: hint, .. } = &mut message {
            *hint = now;
        }
        input.message = borsh::to_vec(&message).unwrap();
        let result = handle(&input);
        if accepted {
            let response = result.unwrap();
            assert_eq!(response.validity.timestamps.end(), Some(400_000));
        } else {
            assert_eq!(result.err(), Some(Error::InvalidTerms));
        }
    }
    assert!(validate_curve_expiry(None, u64::MAX).is_ok());
}

#[test]
fn internal_curve_continuation_rejects_a_hint_at_or_after_expiry() {
    let (_, mint, setup) = minted();
    let mut input = setup.continuation(&mint, LbpCreationStage::Minted);
    for now in [399, 400, u64::MAX] {
        input.message = borsh::to_vec(&Message::ContinueCreation {
            creator: id(42),
            now,
        })
        .unwrap();
        let result = handle(&input);
        if now < 400 {
            assert_eq!(result.unwrap().validity.timestamps.end(), Some(400_000));
        } else {
            assert_eq!(result.err(), Some(Error::InvalidTerms));
        }
    }
}

#[test]
fn curve_setup_keeps_an_exclusive_expiry_and_no_lower_bound() {
    let input = creation();
    let initial = handle(&input).unwrap();
    let setup = Creation::new(&input, &initial);
    let input = setup.namespace_reply(&initial, config());
    let checked = handle(&input).unwrap();
    let mint = handle(&reply(&input, &checked, definition())).unwrap();
    let (_, allocated) = continued(&setup, &mint, 900, 0, 0);
    let (_, prepared) = continued(&setup, &allocated, 0, 800, 100);
    let (mut input, funding) = continued(&setup, &prepared, 0, 800, 100);
    let state = FactoryState::decode(funding.post_state.as_ref().unwrap()).unwrap();
    input = reply(
        &input,
        &funding,
        borsh::to_vec(&pool_state(&state, input.receiver, false)).unwrap(),
    );
    let checked_pool = handle(&input).unwrap();
    let active = handle(&reply(
        &input,
        &checked_pool,
        holding(state.pool_id, state.token_definition_id, 0),
    ))
    .unwrap();
    for response in [
        initial,
        checked,
        mint,
        allocated,
        prepared,
        funding,
        checked_pool,
        active,
    ] {
        let window = response.validity.timestamps;
        assert_eq!(window.start(), None);
        assert_eq!(window.end(), Some(400_000));
        assert!(window.is_valid_for(399_999));
        assert!(!window.is_valid_for(400_000));
    }
}

#[test]
fn curve_creation_rejects_invalid_retained_sell_fees_before_side_effects() {
    for rate in [10_000, u16::MAX] {
        let mut input = creation();
        let mut command: Message = borsh::from_slice(&input.message).unwrap();
        if let Message::CreateFactoryPool { sell_fee_bps, .. } = &mut command {
            *sell_fee_bps = rate;
        }
        input.message = borsh::to_vec(&command).unwrap();
        assert_eq!(handle(&input).err(), Some(Error::InvalidTerms));
        let mut state = FactoryState::decode(active().1.post_state.as_ref().unwrap()).unwrap();
        state.sell_fee_bps = rate;
        assert!(state.validate(input.receiver).is_err());
    }
}
