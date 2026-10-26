//! Namespace changes require an authoritative role reply.
use crate::{
    AUTHORITY_PROGRAM_ACCOUNT_ID, Config, Error, MAX_FEE_BPS, Message, compute_admin_role,
    compute_config_pda,
};
use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response},
};

#[derive(BorshSerialize, BorshDeserialize)]
struct Pending {
    tag: [u8; 8],
    config: Config,
    authority: AccountId,
    message: Message,
}

pub fn begin(input: &ReceiveInput, message: Message) -> Result<Response, Error> {
    let namespace = match message {
        Message::RenounceAdmin { namespace }
        | Message::TransferAdmin { namespace, .. }
        | Message::UpdateFee { namespace, .. }
        | Message::UpdateTreasury { namespace, .. } => namespace,
        _ => return Err(Error::InvalidMessage),
    };
    let program = input.receiver.program_account_id;
    let target = Actor::new(compute_config_pda(namespace, program), program);
    if input.receiver != target {
        if !input.is_authorized {
            return Err(Error::Unauthorized);
        }
        return Ok(Response::keep_state().send_sync(target, &message));
    }
    let config = Config::decode(&input.pre_state)?;
    if config.namespace != namespace {
        return Err(Error::WrongAccount);
    }
    let sender = input.from.ok_or(Error::Unauthorized)?;
    if sender.program_account_id != program {
        return Err(Error::Unauthorized);
    }
    let authority = sender.account_id;
    let pending = Pending {
        tag: *b"LZNSPEND",
        config,
        authority,
        message,
    };
    Ok(
        Response::set_state(borsh::to_vec(&pending).unwrap()).send_sync(
            Actor::new(authority, AUTHORITY_PROGRAM_ACCOUNT_ID),
            &lez_compat_core::authority::Message::Check {
                role: compute_admin_role(namespace, program),
                callback: input.receiver,
            },
        ),
    )
}

pub fn resume(input: &ReceiveInput) -> Option<Result<Response, Error>> {
    if !input.pre_state.starts_with(b"LZNSPEND") {
        return None;
    }
    Some((|| {
        let mut pending: Pending =
            borsh::from_slice(&input.pre_state).map_err(|_| Error::InvalidState)?;
        let role = Actor::new(
            compute_admin_role(pending.config.namespace, input.receiver.program_account_id),
            AUTHORITY_PROGRAM_ACCOUNT_ID,
        );
        lez_compat_core::authority::verify(input, role, pending.authority)
            .map_err(|_| Error::Unauthorized)?;
        let mut response = Response::keep_state();
        match pending.message {
            Message::UpdateFee {
                protocol_fee_bps, ..
            } => {
                if protocol_fee_bps >= u32::from(MAX_FEE_BPS) {
                    return Err(Error::InvalidFee);
                }
                pending.config.protocol_fee_bps = protocol_fee_bps as u16;
            }
            Message::UpdateTreasury { treasury, .. } => {
                if treasury == AccountId::default() {
                    return Err(Error::InvalidTreasury);
                }
                pending.config.treasury = treasury;
            }
            Message::RenounceAdmin { .. } => {
                response = response.send_sync(
                    Actor::new(pending.authority, AUTHORITY_PROGRAM_ACCOUNT_ID),
                    &lez_compat_core::authority::Message::Renounce {
                        role: role.account_id,
                    },
                );
            }
            Message::TransferAdmin {
                new_admin,
                candidate,
                ..
            } => {
                if new_admin == AccountId::default() {
                    return Err(Error::Unauthorized);
                }
                let message = match candidate {
                    crate::AdminCandidate::Signer => {
                        lez_compat_core::authority::Message::Transfer {
                            role: role.account_id,
                            successor: new_admin,
                        }
                    }
                    crate::AdminCandidate::Pda { program_id, seed } => {
                        if program_id == AccountId::default()
                            || AccountId::for_public_pda(
                                &program_id,
                                &lee_core::program::PdaSeed::new(seed),
                            ) != new_admin
                        {
                            return Err(Error::WrongAccount);
                        }
                        lez_compat_core::authority::Message::TransferPda {
                            role: role.account_id,
                            program: program_id,
                            seed: lee_core::program::PdaSeed::new(seed),
                        }
                    }
                };
                response = response.send_sync(
                    Actor::new(pending.authority, AUTHORITY_PROGRAM_ACCOUNT_ID),
                    &message,
                );
            }
            _ => return Err(Error::InvalidMessage),
        }
        response.post_state = Some(borsh::to_vec(&pending.config).unwrap().into());
        Ok(response)
    })())
}
