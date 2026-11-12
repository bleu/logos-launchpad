use factory_core::{Error, LbpLaunchState, Message, handle};
use lee_core::{
    account::{AccountId, Actor},
    program::ReceiveInput,
};

fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}

pub fn assert_creator_authority_and_pool_forwarding(
    mut input: ReceiveInput,
    mut state: LbpLaunchState,
) {
    for paused in [true, false] {
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.from = None;
        input.is_authorized = true;
        input.message = borsh::to_vec(&Message::SetPaused {
            creator: id(99),
            paused,
        })
        .unwrap();
        let response = handle(&input).unwrap();
        assert_eq!(response.sync_messages.len(), 1);
        let check = &response.sync_messages[0];
        assert_eq!(
            check.to,
            Actor::new(id(99), factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID)
        );
        assert_eq!(
            borsh::from_slice::<lez_compat_core::authority::Message>(&check.message).unwrap(),
            lez_compat_core::authority::Message::Check {
                role: state.creator_role_id,
                callback: input.receiver
            }
        );
        assert!(check.pda_seeds.is_empty());
        let valid = ReceiveInput {
            receiver: input.receiver,
            from: Some(Actor::new(
                state.creator_role_id,
                factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
            )),
            is_authorized: false,
            pre_state: response.post_state.clone().unwrap(),
            message: borsh::to_vec(&lez_compat_core::authority::Authorized {
                role: state.creator_role_id,
                authority: id(99),
            })
            .unwrap(),
        };
        for other_role in [
            factory_core::compute_creator_role_pda(
                state.namespace,
                input.receiver.program_account_id,
                [4; 32],
            ),
            factory_core::compute_creator_role_pda(
                id(41),
                input.receiver.program_account_id,
                state.launch_salt,
            ),
            factory_core::compute_curve_creator_role_pda(
                state.namespace,
                input.receiver.program_account_id,
                state.launch_salt,
            ),
            curve_core::compute_config_pda(state.namespace, state.namespace_program_id),
        ] {
            let mut forged = valid.clone();
            forged.from = Some(Actor::new(
                other_role,
                factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
            ));
            forged.message = borsh::to_vec(&lez_compat_core::authority::Authorized {
                role: other_role,
                authority: id(99),
            })
            .unwrap();
            assert_eq!(handle(&forged).err(), Some(Error::Unauthorized));
        }
        for authority in [id(42), state.namespace] {
            let mut forged = valid.clone();
            forged.message = borsh::to_vec(&lez_compat_core::authority::Authorized {
                role: state.creator_role_id,
                authority,
            })
            .unwrap();
            assert_eq!(handle(&forged).err(), Some(Error::Unauthorized));
        }
        let mut forged = valid.clone();
        forged.from = Some(Actor::new(state.creator_role_id, id(90)));
        assert_eq!(handle(&forged).err(), Some(Error::Unauthorized));
        forged = valid.clone();
        forged.receiver.account_id = id(90);
        assert_eq!(handle(&forged).err(), Some(Error::WrongAccount));
        let forwarded = handle(&valid).unwrap();
        assert!(forwarded.events.is_empty());
        assert_eq!(forwarded.sync_messages.len(), 1);
        let call = &forwarded.sync_messages[0];
        assert_eq!(call.to, Actor::new(state.pool_id, state.lbp_program_id));
        assert_eq!(
            borsh::from_slice::<lbp_core::Message>(&call.message).unwrap(),
            lbp_core::Message::SetPaused { paused }
        );
        assert_eq!(
            call.pda_seeds,
            [factory_core::compute_factory_seed(
                state.namespace,
                state.launch_salt
            )]
            .into()
        );
        assert_eq!(
            factory_core::LbpLaunchState::decode(forwarded.post_state.as_ref().unwrap()).unwrap(),
            state
        );
    }
    state.creation_stage = factory_core::LbpCreationStage::PoolPrepared;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    assert_eq!(handle(&input).err(), Some(Error::CreationPending));
}
