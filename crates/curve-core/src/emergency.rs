// Adapted from SPEL authority and freeze helpers; see docs/adr/0028-token-custody-and-freeze.md.
// Copyright (c) 2026 mmlado. Licensed under MIT.
use crate::{Config, Error, Message, compute_config_pda};
use lee_core::{
    account::Actor,
    program::{ReceiveInput, Response},
};

pub fn handle(input: &ReceiveInput, message: Message) -> Result<Response, Error> {
    let namespace = match message {
        Message::TransferEmergencyAuthority { namespace, .. }
        | Message::SetFrozen { namespace, .. } => namespace,
        _ => return Err(Error::InvalidMessage),
    };
    let program = input.receiver.program_account_id;
    let config = Actor::new(compute_config_pda(namespace, program), program);
    if input.receiver != config {
        if !input.is_authorized {
            return Err(Error::Unauthorized);
        }
        return Ok(Response::keep_state().send_sync(config, &message));
    }
    let mut state = Config::decode(&input.pre_state)?;
    if state.namespace != namespace {
        return Err(Error::WrongAccount);
    }
    if input.from != Some(Actor::new(state.emergency.authority(), program)) {
        return Err(Error::Unauthorized);
    }
    match message {
        Message::SetFrozen { frozen, .. } => state.emergency.is_frozen = frozen,
        Message::TransferEmergencyAuthority { new_authority, .. } => {
            if new_authority == lee_core::account::AccountId::default() {
                return Err(Error::Unauthorized);
            }
            state.emergency.holder = new_authority;
        }
        _ => return Err(Error::InvalidMessage),
    }
    Ok(Response::set_state(borsh::to_vec(&state).unwrap()))
}
