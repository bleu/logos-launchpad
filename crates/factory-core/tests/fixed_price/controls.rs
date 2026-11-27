use super::*;

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
        let initial = handle(&input).unwrap();
        let setup = Creation::new(&input, &initial);
        let input = setup.namespace_reply(&initial, namespace_state());
        let checked = handle(&input).unwrap();
        let mint = handle(&reply(&input, &checked, collateral())).unwrap();
        for response in [&initial, &checked, &mint] {
            assert_eq!(
                factory_core::LbpLaunchState::decode(response.post_state.as_ref().unwrap())
                    .unwrap()
                    .pause_policy,
                policy
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
                .pause_policy,
            policy
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
                &setup,
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
