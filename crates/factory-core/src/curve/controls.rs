//! Creator-authorized forwarding of curve pause controls.
use super::*;

pub(crate) fn begin_pause(input: &ReceiveInput, command: Message) -> Result<Response, Error> {
    let mut state = FactoryState::decode(&input.pre_state)?;
    state.validate(input.receiver)?;
    if state.creation_stage != CreationStage::Active {
        return Err(Error::CreationPending);
    }
    let Message::SetPaused { creator, .. } = command else {
        return Err(Error::InvalidMessage);
    };
    state.pending = Some(Pending {
        command,
        proof: true,
        reads: vec![],
        values: vec![],
    });
    Ok(state.response().send_sync(
        Actor::new(creator, AUTHORITY_PROGRAM_ACCOUNT_ID),
        &authority::Message::Check {
            role: state.creator_role_id,
            callback: input.receiver,
        },
    ))
}

pub(super) fn finish_pause(state: &FactoryState, paused: bool) -> Response {
    state.response().send(
        SyncMessage::new(
            Actor::new(state.pool_id, state.curve_program_id),
            &curve_core::Message::SetPaused { paused },
        )
        .with_pda_seeds([compute_factory_seed(state.namespace, state.launch_salt)]),
    )
}
