use super::*;

#[test]
fn weighted_pause_policy_survives_setup_and_rejects_substitution() {
    for policy in [
        curve_core::PausePolicy::Disabled,
        curve_core::PausePolicy::AuthorityControlled,
    ] {
        let mut terms = inputs();
        terms.pause_policy = policy;
        let (_, prepared, bytes, setup) = prepared_with_terms(terms);
        let state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(state.pause_policy, policy);
        let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
        assert_eq!(pool.trade_control.policy(), policy);
        for control in [
            curve_core::TradeControl::Unrestricted,
            curve_core::TradeControl::Running,
            curve_core::TradeControl::Paused,
        ] {
            let mut pool = pool.clone();
            pool.trade_control = control;
            let result = advance(
                &setup,
                &prepared,
                vec![
                    borsh::to_vec(&pool).unwrap(),
                    holding(state.pool_id, state.token_definition_id, 0),
                    holding(state.pool_id, state.collateral_definition_id, 0),
                    holding(id(42), state.collateral_definition_id, 100),
                ],
            );
            if control.policy() != policy {
                assert_eq!(result.err(), Some(Error::InvalidState));
            } else {
                let complete =
                    LbpLaunchState::decode(result.unwrap().post_state.as_ref().unwrap()).unwrap();
                assert_eq!(complete.pause_policy, policy);
                assert_eq!(complete.creation_stage, LbpCreationStage::Complete);
            }
        }
    }
}

#[test]
fn shared_pause_requires_the_sale_creator_on_every_call_before_pool_forwarding() {
    let (input, response, _) = minted(CreatorAllocationRelease::AfterEnd);
    let mut state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = LbpCreationStage::Complete;
    state.pause_policy = curve_core::PausePolicy::AuthorityControlled;
    assert!(matches!(
        state.mode,
        factory_core::LbpSetupMode::Weighted { .. }
    ));
    lbp_pause_forwarding::assert_creator_authority_and_pool_forwarding(input, state);
}

#[test]
fn legacy_weighted_pause_keeps_its_weighted_pool_alias() {
    let (mut input, response, _) = minted(CreatorAllocationRelease::AfterEnd);
    let mut state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = LbpCreationStage::Complete;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SetWeightedPaused {
        creator: id(42),
        paused: true,
    })
    .unwrap();
    let response = handle(&input).unwrap();
    assert_eq!(response.sync_messages.len(), 1);
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
    let response = handle(&input).unwrap();
    assert_eq!(response.sync_messages.len(), 1);
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(state.pool_id, state.lbp_program_id)
    );
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&response.sync_messages[0].message).unwrap(),
        lbp_core::Message::SetWeightedPaused { paused: true }
    );
}
