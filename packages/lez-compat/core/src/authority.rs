use crate::Error;
use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, ReceiveInput, Response, SyncMessage},
};

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Message {
    Bootstrap {
        authority: AccountId,
    },
    Consent {
        role: AccountId,
        epoch: u64,
    },
    Install {
        epoch: u64,
    },
    Check {
        role: AccountId,
        callback: Actor,
    },
    Assert {
        authority: AccountId,
        callback: Actor,
    },
    Transfer {
        role: AccountId,
        successor: AccountId,
    },
    Propose {
        authority: AccountId,
        successor: AccountId,
    },
    Renounce {
        role: AccountId,
    },
    Revoke {
        authority: AccountId,
    },
    TransferPda {
        role: AccountId,
        program: AccountId,
        seed: PdaSeed,
    },
    ProposePda {
        authority: AccountId,
        program: AccountId,
        seed: PdaSeed,
    },
    AcceptPda {
        role: AccountId,
        epoch: u64,
        program: AccountId,
        seed: PdaSeed,
    },
    BootstrapTransferOnly {
        authority: AccountId,
    },
    Resolve,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PdaRequest {
    pub role: Actor,
    pub epoch: u64,
    pub seed: PdaSeed,
}

/// The successor's owner program calls this helper to approve a role transfer.
pub fn approve_pda(input: &ReceiveInput, authority_program: AccountId) -> Result<Response, Error> {
    let request: PdaRequest =
        borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    let program = input.receiver.program_account_id;
    if request.role.program_account_id != authority_program
        || input.from != Some(request.role)
        || input.receiver.account_id != AccountId::for_public_pda(&program, &request.seed)
    {
        return Err(Error::Unauthorized);
    }
    Ok(Response::keep_state().send(
        SyncMessage::new(
            Actor::new(input.receiver.account_id, request.role.program_account_id),
            &Message::AcceptPda {
                role: request.role.account_id,
                epoch: request.epoch,
                program,
                seed: request.seed,
            },
        )
        .with_pda_seeds([request.seed]),
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Authorized {
    pub role: AccountId,
    pub authority: AccountId,
}

pub fn verify(input: &ReceiveInput, role: Actor, authority: AccountId) -> Result<(), Error> {
    if input.from != Some(role) {
        return Err(Error::Unauthorized);
    }
    let reply: Authorized = borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    if reply
        != (Authorized {
            role: role.account_id,
            authority,
        })
    {
        return Err(Error::Unauthorized);
    }
    Ok(())
}

/// Current holder data only; this reply does not approve an action.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CurrentAuthority {
    pub role: AccountId,
    pub authority: AccountId,
}

pub fn verify_resolved(input: &ReceiveInput, expected_role: Actor) -> Result<AccountId, Error> {
    if input.from != Some(expected_role) {
        return Err(Error::Unauthorized);
    }
    let reply: CurrentAuthority =
        borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    if reply.role != expected_role.account_id || reply.authority == AccountId::default() {
        return Err(Error::Unauthorized);
    }
    Ok(reply.authority)
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
struct Role {
    holder: Option<AccountId>,
    pending: Option<AccountId>,
    epoch: u64,
    can_renounce: bool,
}

impl Role {
    fn response(&self) -> Response {
        Response::set_state(borsh::to_vec(self).unwrap())
    }
}

pub fn handle(input: &ReceiveInput) -> Result<Response, Error> {
    let message: Message = borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    let program = input.receiver.program_account_id;
    match message {
        Message::Resolve => {
            let caller = input.from.ok_or(Error::Unauthorized)?;
            let role: Role =
                borsh::from_slice(&input.pre_state).map_err(|_| Error::InvalidState)?;
            let authority = role.holder.ok_or(Error::Renounced)?;
            if authority == AccountId::default() {
                return Err(Error::Unauthorized);
            }
            Ok(Response::keep_state().send_sync(
                caller,
                &CurrentAuthority {
                    role: input.receiver.account_id,
                    authority,
                },
            ))
        }
        Message::TransferPda {
            role,
            program: owner,
            seed,
        } => {
            if !input.is_authorized {
                return Err(Error::Unauthorized);
            }
            Ok(Response::keep_state().send_sync(
                Actor::new(role, program),
                &Message::ProposePda {
                    authority: input.receiver.account_id,
                    program: owner,
                    seed,
                },
            ))
        }
        Message::ProposePda {
            authority,
            program: owner,
            seed,
        } => {
            let mut role: Role =
                borsh::from_slice(&input.pre_state).map_err(|_| Error::InvalidState)?;
            if role.holder.is_none() {
                return Err(Error::Renounced);
            }
            if role.holder != Some(authority) || input.from != Some(Actor::new(authority, program))
            {
                return Err(Error::Unauthorized);
            }
            let successor = AccountId::for_public_pda(&owner, &seed);
            role.pending = Some(successor);
            Ok(role.response().send_sync(
                Actor::new(successor, owner),
                &PdaRequest {
                    role: input.receiver,
                    epoch: role.epoch,
                    seed,
                },
            ))
        }
        Message::AcceptPda {
            role,
            epoch,
            program: owner,
            seed,
        } => {
            let successor = AccountId::for_public_pda(&owner, &seed);
            if !input.is_authorized
                || input.receiver.account_id != successor
                || input.from != Some(Actor::new(successor, owner))
            {
                return Err(Error::Unauthorized);
            }
            Ok(Response::keep_state()
                .send_sync(Actor::new(role, program), &Message::Install { epoch }))
        }
        Message::Renounce { role } => {
            if !input.is_authorized {
                return Err(Error::Unauthorized);
            }
            Ok(Response::keep_state().send_sync(
                Actor::new(role, program),
                &Message::Revoke {
                    authority: input.receiver.account_id,
                },
            ))
        }
        Message::Revoke { authority } => {
            let mut role: Role =
                borsh::from_slice(&input.pre_state).map_err(|_| Error::InvalidState)?;
            if role.holder.is_none() {
                return Err(Error::Renounced);
            }
            if role.holder != Some(authority) || input.from != Some(Actor::new(authority, program))
            {
                return Err(Error::Unauthorized);
            }
            if !role.can_renounce {
                return Err(Error::Unauthorized);
            }
            role.holder = None;
            role.pending = None;
            role.epoch = role.epoch.checked_add(1).ok_or(Error::Overflow)?;
            Ok(role.response())
        }
        Message::Transfer { role, successor } => {
            if !input.is_authorized {
                return Err(Error::Unauthorized);
            }
            Ok(Response::keep_state().send_sync(
                Actor::new(role, program),
                &Message::Propose {
                    authority: input.receiver.account_id,
                    successor,
                },
            ))
        }
        Message::Propose {
            authority,
            successor,
        } => {
            let mut role: Role =
                borsh::from_slice(&input.pre_state).map_err(|_| Error::InvalidState)?;
            if role.holder.is_none() {
                return Err(Error::Renounced);
            }
            if role.holder != Some(authority)
                || input.from != Some(Actor::new(authority, program))
                || successor == AccountId::default()
            {
                return Err(Error::Unauthorized);
            }
            role.pending = Some(successor);
            Ok(role.response().send_sync(
                Actor::new(successor, program),
                &Message::Consent {
                    role: input.receiver.account_id,
                    epoch: role.epoch,
                },
            ))
        }
        Message::Bootstrap { authority } | Message::BootstrapTransferOnly { authority } => {
            if !input.is_authorized {
                return Err(Error::Unauthorized);
            }
            if !input.pre_state.is_empty() {
                return Err(Error::AlreadyInitialized);
            }
            if authority == AccountId::default() {
                return Err(Error::Unauthorized);
            }
            Ok(Role {
                holder: None,
                pending: Some(authority),
                epoch: 0,
                can_renounce: matches!(message, Message::Bootstrap { .. }),
            }
            .response()
            .send_sync(
                Actor::new(authority, program),
                &Message::Consent {
                    role: input.receiver.account_id,
                    epoch: 0,
                },
            ))
        }
        Message::Consent { role, epoch } => {
            if !input.is_authorized || input.from != Some(Actor::new(role, program)) {
                return Err(Error::Unauthorized);
            }
            Ok(Response::keep_state()
                .send_sync(Actor::new(role, program), &Message::Install { epoch }))
        }
        Message::Install { epoch } => {
            let mut role: Role =
                borsh::from_slice(&input.pre_state).map_err(|_| Error::InvalidState)?;
            let candidate = role.pending.ok_or(Error::Unauthorized)?;
            if epoch != role.epoch || input.from != Some(Actor::new(candidate, program)) {
                return Err(Error::Unauthorized);
            }
            role.holder = Some(candidate);
            role.pending = None;
            role.epoch = role.epoch.checked_add(1).ok_or(Error::Overflow)?;
            Ok(role.response())
        }
        Message::Check { role, callback } => {
            if !input.is_authorized {
                return Err(Error::Unauthorized);
            }
            Ok(Response::keep_state().send_sync(
                Actor::new(role, program),
                &Message::Assert {
                    authority: input.receiver.account_id,
                    callback,
                },
            ))
        }
        Message::Assert {
            authority,
            callback,
        } => {
            let role: Role =
                borsh::from_slice(&input.pre_state).map_err(|_| Error::InvalidState)?;
            if role.holder.is_none() {
                return Err(Error::Renounced);
            }
            if role.holder != Some(authority) || input.from != Some(Actor::new(authority, program))
            {
                return Err(Error::Unauthorized);
            }
            Ok(Response::keep_state().send_sync(
                callback,
                &Authorized {
                    role: input.receiver.account_id,
                    authority,
                },
            ))
        }
    }
}
