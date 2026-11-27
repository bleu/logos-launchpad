use super::*;

#[test]
fn curve_pause_policy_survives_setup_and_checks_policy_instead_of_pause_value() {
    for policy in [PausePolicy::Disabled, PausePolicy::AuthorityControlled] {
        let mut input = creation();
        let mut message: Message = borsh::from_slice(&input.message).unwrap();
        if let Message::CreateFactoryPool { pause_policy, .. } = &mut message {
            *pause_policy = policy;
        }
        input.message = borsh::to_vec(&message).unwrap();
        let initial = handle(&input).unwrap();
        let setup = Creation::new(&input, &initial);
        let input = setup.namespace_reply(&initial, config());
        let checked = handle(&input).unwrap();
        let mint = handle(&reply(&input, &checked, definition())).unwrap();
        for response in [&initial, &checked, &mint] {
            assert_eq!(
                FactoryState::decode(response.post_state.as_ref().unwrap())
                    .unwrap()
                    .pause_policy,
                policy
            );
        }
        let (_, allocated) = continued(&setup, &mint, 900, 0, 0);
        assert_eq!(
            FactoryState::decode(allocated.post_state.as_ref().unwrap())
                .unwrap()
                .pause_policy,
            policy
        );
        let (_, prepared) = continued(&setup, &allocated, 0, 800, 100);
        assert_eq!(
            FactoryState::decode(prepared.post_state.as_ref().unwrap())
                .unwrap()
                .pause_policy,
            policy
        );
        let create = prepared
            .sync_messages
            .iter()
            .find_map(|call| borsh::from_slice::<curve_core::Message>(&call.message).ok())
            .unwrap();
        assert!(
            matches!(create, curve_core::Message::CreatePool { pause_policy, .. } if pause_policy == policy)
        );
        for control in [
            curve_core::TradeControl::Unrestricted,
            curve_core::TradeControl::Running,
            curve_core::TradeControl::Paused,
        ] {
            let (mut input, mut response) = continued(&setup, &prepared, 0, 800, 100);
            let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
            let mut saved = pool_state(&state, input.receiver, false);
            saved.trade_control = control;
            input = reply(&input, &response, borsh::to_vec(&saved).unwrap());
            response = handle(&input).unwrap();
            input = reply(
                &input,
                &response,
                holding(state.pool_id, state.token_definition_id, 0),
            );
            let result = handle(&input);
            if control.policy() != policy {
                assert_eq!(result.err(), Some(Error::InvalidState));
            } else {
                let active =
                    FactoryState::decode(result.unwrap().post_state.as_ref().unwrap()).unwrap();
                assert_eq!(active.creation_stage, CreationStage::Active);
                assert_eq!(active.pause_policy, policy);
            }
        }
    }
}

#[test]
fn curve_pause_checks_current_creator_on_every_call_and_forwards_only_to_bound_pool() {
    let (mut input, active) = active();
    let mut state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    state.pause_policy = PausePolicy::AuthorityControlled;
    for paused in [true, false] {
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.from = None;
        input.message = borsh::to_vec(&Message::SetPaused {
            creator: id(99),
            paused,
        })
        .unwrap();
        let response = handle(&input).unwrap();
        assert_eq!(response.sync_messages.len(), 1);
        let check = &response.sync_messages[0];
        assert_eq!(check.to, Actor::new(id(99), AUTHORITY_PROGRAM_ACCOUNT_ID));
        assert_eq!(
            borsh::from_slice::<authority::Message>(&check.message).unwrap(),
            authority::Message::Check {
                role: state.creator_role_id,
                callback: input.receiver
            }
        );
        assert!(check.pda_seeds.is_empty());
        assert_eq!(
            handle(&proof(&input, &response, id(42))).err(),
            Some(Error::Unauthorized)
        );
        let response = handle(&proof(&input, &response, id(99))).unwrap();
        assert!(response.events.is_empty());
        assert_eq!(response.sync_messages.len(), 1);
        let call = &response.sync_messages[0];
        assert_eq!(call.to, Actor::new(state.pool_id, state.curve_program_id));
        assert_eq!(
            borsh::from_slice::<curve_core::Message>(&call.message).unwrap(),
            curve_core::Message::SetPaused { paused }
        );
        assert_eq!(
            call.pda_seeds,
            [compute_factory_seed(state.namespace, state.launch_salt)].into()
        );
        assert_eq!(
            FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap(),
            state
        );
    }
    state.creation_stage = CreationStage::PoolPrepared;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    assert_eq!(handle(&input).err(), Some(Error::CreationPending));
    input.message = borsh::to_vec(&Message::SetWeightedPaused {
        creator: id(42),
        paused: true,
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(Error::InvalidState));
}
