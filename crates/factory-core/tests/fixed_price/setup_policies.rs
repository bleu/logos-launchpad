use super::*;

#[test]
fn fixed_price_setup_rejects_policy_downgrades_and_replacements() {
    let (_, prepared, setup) = allowlisted_prepared();
    for case in 0..3 {
        let (mut input, mut response) = continue_sale(
            &setup,
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
fn fixed_price_rejects_malformed_saved_terms_and_the_old_record_version() {
    let (_, response, setup) = minted();
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
        let mut request = setup.continuation(&response, factory_core::LbpCreationStage::Minted);
        request.pre_state = borsh::to_vec(&changed).unwrap().into();
        assert_eq!(
            factory_core::LbpLaunchState::decode(&request.pre_state).err(),
            Some(Error::InvalidState)
        );
        assert_eq!(handle(&request).err(), Some(Error::InvalidState));
    }
    let mut bytes = borsh::to_vec(&state).unwrap();
    let version_offset = borsh::to_vec(&(
        state.per_block_token_limit,
        state.buy_policy,
        state.pause_policy,
        state.ending_policy,
        state.trade_mode,
        state.sell_fee_bps,
        state.graduation_hook,
        state.graduation_policy,
    ))
    .unwrap()
    .len()
        + 8;
    assert_eq!(
        &bytes[version_offset..version_offset + 2],
        &8_u16.to_le_bytes()
    );
    for old_version in [2_u16, 5, 6] {
        bytes[version_offset..version_offset + 2].copy_from_slice(&old_version.to_le_bytes());
        assert_eq!(
            factory_core::LbpLaunchState::decode(&bytes).err(),
            Some(Error::InvalidState)
        );
    }
}

#[test]
fn fixed_price_pending_callbacks_reject_malformed_saved_policies() {
    let input = creation();
    let response = handle(&input).unwrap();
    let mut state =
        factory_core::LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    state.ending_policy = factory_core::EndingPolicy::Manual;
    let setup = Creation::new(&input, &response);
    let mut next = setup.namespace_reply(&response, namespace_state());
    next.pre_state = borsh::to_vec(&state).unwrap().into();
    assert_eq!(handle(&next).err(), Some(Error::InvalidState));
}

#[test]
fn fixed_price_setup_and_allocation_reject_changed_pool_policies() {
    let (input, prepared, setup) = allowlisted_prepared();
    let state =
        factory_core::LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    for case in 0..2 {
        let (mut next, mut response) = continue_sale(
            &setup,
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
fn fixed_price_hook_terms_survive_pool_preparation_and_reject_adapter_substitution() {
    let mut value = inputs();
    value.graduation_hook = Some(factory_core::GraduationHook {
        program_id: id(9),
        configuration: b"fixed".to_vec(),
    });
    let mut input = creation();
    input.message = borsh::to_vec(&Message::CreateFixedPriceLbp {
        inputs: value.clone(),
    })
    .unwrap();
    let (_, response, setup) = mint_from(&input);
    assert_eq!(
        factory_core::LbpLaunchState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .graduation_hook,
        value.graduation_hook
    );
    let (_, allocated) = continue_sale(
        &setup,
        &response,
        factory_core::LbpCreationStage::Minted,
        1000,
        0,
        0,
    );
    let (_, prepared) = continue_sale(
        &setup,
        &allocated,
        factory_core::LbpCreationStage::Allocated,
        0,
        800,
        200,
    );
    let (mut input, mut response) = continue_sale(
        &setup,
        &prepared,
        factory_core::LbpCreationStage::PoolPrepared,
        0,
        800,
        200,
    );
    let state =
        factory_core::LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    let mut pool = lbp_core::PoolAccount::decode(&prepared_pool(&state, input.receiver)).unwrap();
    assert_eq!(
        pool.graduation_adapter,
        Some(Actor::new(
            factory_core::compute_graduation_pda(id(9), input.receiver.account_id),
            id(9)
        ))
    );
    pool.graduation_adapter = None;
    input = reply(&input, &response, borsh::to_vec(&pool).unwrap());
    response = handle(&input).unwrap();
    input = reply(
        &input,
        &response,
        holding(state.pool_id, state.token_definition_id, 0),
    );
    response = handle(&input).unwrap();
    input = reply(
        &input,
        &response,
        holding(state.pool_id, state.collateral_definition_id, 0),
    );
    assert_eq!(handle(&input).err(), Some(Error::InvalidCustody));
}

#[test]
fn fixed_price_trade_mode_survives_every_setup_stage_and_rejects_substitution() {
    for mode in [
        factory_core::TradeMode::BuyOnly,
        factory_core::TradeMode::BuyAndSell,
    ] {
        let mut input = creation();
        let mut terms = inputs();
        terms.trade_mode = mode;
        input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
        let initial = handle(&input).unwrap();
        let setup = Creation::new(&input, &initial);
        let input = setup.namespace_reply(&initial, namespace_state());
        let checked = handle(&input).unwrap();
        let mint = handle(&reply(&input, &checked, collateral())).unwrap();
        for response in [&initial, &checked, &mint] {
            assert_eq!(
                factory_core::LbpLaunchState::decode(response.post_state.as_ref().unwrap())
                    .unwrap()
                    .trade_mode,
                mode
            );
        }
        let (_, allocated) = continue_sale(
            &setup,
            &mint,
            factory_core::LbpCreationStage::Minted,
            1000,
            0,
            0,
        );
        assert_eq!(
            factory_core::LbpLaunchState::decode(allocated.post_state.as_ref().unwrap())
                .unwrap()
                .trade_mode,
            mode
        );
        let (_, prepared) = continue_sale(
            &setup,
            &allocated,
            factory_core::LbpCreationStage::Allocated,
            0,
            800,
            200,
        );
        let state =
            factory_core::LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(state.trade_mode, mode);
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
        assert_eq!(inputs.trade_mode, mode);
        for pool_mode in [
            factory_core::TradeMode::BuyOnly,
            factory_core::TradeMode::BuyAndSell,
        ] {
            let (mut input, mut response) = continue_sale(
                &setup,
                &prepared,
                factory_core::LbpCreationStage::PoolPrepared,
                0,
                800,
                200,
            );
            let mut pool =
                lbp_core::PoolAccount::decode(&prepared_pool(&state, input.receiver)).unwrap();
            pool.trade_mode = pool_mode;
            let values = [
                borsh::to_vec(&pool).unwrap(),
                holding(state.pool_id, state.token_definition_id, 0),
                holding(state.pool_id, state.collateral_definition_id, 0),
            ];
            for (slot, bytes) in values.into_iter().enumerate() {
                input = reply(&input, &response, bytes);
                let result = handle(&input);
                if slot == 2 && pool_mode != mode {
                    assert_eq!(result.err(), Some(Error::InvalidCustody));
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
                    assert_eq!(complete.trade_mode, mode);
                }
            }
        }
    }
}

#[test]
fn fixed_price_sell_fee_survives_every_setup_stage_and_rejects_substitution() {
    for rate in [0, 125, 9999] {
        let mut input = creation();
        let mut terms = inputs();
        terms.sell_fee_bps = rate;
        input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
        let initial = handle(&input).unwrap();
        let setup = Creation::new(&input, &initial);
        let input = setup.namespace_reply(&initial, namespace_state());
        let checked = handle(&input).unwrap();
        let mint = handle(&reply(&input, &checked, collateral())).unwrap();
        for response in [&initial, &checked, &mint] {
            assert_eq!(
                factory_core::LbpLaunchState::decode(response.post_state.as_ref().unwrap())
                    .unwrap()
                    .sell_fee_bps,
                rate
            );
        }
        let (_, allocated) = continue_sale(
            &setup,
            &mint,
            factory_core::LbpCreationStage::Minted,
            1000,
            0,
            0,
        );
        assert_eq!(
            factory_core::LbpLaunchState::decode(allocated.post_state.as_ref().unwrap())
                .unwrap()
                .sell_fee_bps,
            rate
        );
        let (_, prepared) = continue_sale(
            &setup,
            &allocated,
            factory_core::LbpCreationStage::Allocated,
            0,
            800,
            200,
        );
        let state =
            factory_core::LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(state.sell_fee_bps, rate);
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
        assert_eq!(inputs.sell_fee_bps, rate);
        for pool_rate in [0, 125, 9999] {
            let (mut input, mut response) = continue_sale(
                &setup,
                &prepared,
                factory_core::LbpCreationStage::PoolPrepared,
                0,
                800,
                200,
            );
            let mut pool =
                lbp_core::PoolAccount::decode(&prepared_pool(&state, input.receiver)).unwrap();
            pool.sell_fee_bps = pool_rate;
            let values = [
                borsh::to_vec(&pool).unwrap(),
                holding(state.pool_id, state.token_definition_id, 0),
                holding(state.pool_id, state.collateral_definition_id, 0),
            ];
            for (slot, bytes) in values.into_iter().enumerate() {
                input = reply(&input, &response, bytes);
                let result = handle(&input);
                if slot == 2 && pool_rate != rate {
                    assert_eq!(result.err(), Some(Error::InvalidCustody));
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
                    assert_eq!(complete.sell_fee_bps, rate);
                }
            }
        }
    }
}
