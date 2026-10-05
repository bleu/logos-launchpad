//! Component authority and admission probe, not a sale or asset-transfer implementation.
//! Creator references are fixed fixtures; NFT control and sale lifecycle checks are outside this probe.

use admin_authority::{AdminCandidate, AdminConfig};
use freeze_authority::{FreezeCandidate, FreezeConfig};
use serde::{Deserialize, Serialize};
use spel_framework::prelude::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Instruction {
    Initialize {
        creator: AccountId,
        custody: AccountId,
    },
    CheckAdmin,
    RenounceAdmin,
    Freeze,
    Release,
    CreateSaleReference {
        sale_seed: [u8; 32],
    },
    Admit {
        sale_seed: [u8; 32],
        operation: Operation,
    },
    TransferAdmin {
        candidate: AdminCandidate,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Operation {
    CurveSwap,
    LbpSwap,
    CollectFees,
    Settle,
    Recover,
}

pub fn sale_id(program: &ProgramId, namespace: AccountId, sale_seed: [u8; 32]) -> AccountId {
    compute_pda(
        program,
        &[
            &seed_from_str("sale"),
            namespace.as_ref().try_into().expect("32-byte account ID"),
            &sale_seed,
        ],
    )
}

pub fn namespace_id(program: &ProgramId, seed: [u8; 32]) -> AccountId {
    AccountId::for_public_pda(program, &PdaSeed::new(seed))
}

const ADMIN_OFFSET: usize = 0;
const FREEZE_OFFSET: usize = AdminConfig::SIZE;
const REFERENCES_OFFSET: usize = FREEZE_OFFSET + FreezeConfig::SIZE;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct References {
    pub creator: AccountId,
    pub custody: AccountId,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
struct SaleReference {
    namespace: AccountId,
    references: References,
}

pub fn references(namespace: &AccountWithMetadata) -> Result<References, String> {
    let data = namespace
        .account
        .data
        .get(REFERENCES_OFFSET..)
        .ok_or("missing references")?;
    borsh::from_slice(data).map_err(|e| e.to_string())
}

pub fn dispatch(
    program: ProgramId,
    seed: [u8; 32],
    mut accounts: Vec<AccountWithMetadata>,
    instruction: Instruction,
) -> Result<SpelOutput, String> {
    let namespace = accounts.first().ok_or("missing namespace")?;
    if namespace.account_id != namespace_id(&program, seed) {
        return Err("wrong namespace PDA".into());
    }
    if !matches!(instruction, Instruction::Initialize { .. })
        && namespace.account.program_owner != program
    {
        return Err("wrong namespace owner".into());
    }
    let mut claim = AutoClaim::None;
    let mut claim_index = 0;
    match instruction {
        Instruction::Initialize { creator, custody } => {
            let [namespace, admin, emergency] = accounts.as_mut_slice() else {
                return Err("expected namespace, admin and emergency".into());
            };
            if namespace.account != Account::default() {
                return Err("namespace already exists".into());
            }
            let mut data = vec![0; REFERENCES_OFFSET];
            data.extend(
                borsh::to_vec(&References { creator, custody }).map_err(|e| e.to_string())?,
            );
            namespace.account.data = data.try_into().map_err(|_| "namespace too large")?;
            AdminConfig::bootstrap_at(namespace, ADMIN_OFFSET, AdminCandidate::Signer, admin)
                .map_err(|e| e.to_string())?;
            FreezeConfig::bootstrap_at(
                namespace,
                FREEZE_OFFSET,
                FreezeCandidate::Signer,
                emergency,
            )
            .map_err(|e| e.to_string())?;
            claim = AutoClaim::Claimed(Claim::Pda(PdaSeed::new(seed)));
        }
        Instruction::TransferAdmin { candidate } => {
            let [namespace, current, successor] = accounts.as_mut_slice() else {
                return Err("expected namespace, current admin and successor".into());
            };
            if let AdminCandidate::Pda { program_id, .. } = &candidate {
                if successor.account.program_owner != *program_id {
                    return Err("wrong successor PDA owner".into());
                }
            }
            AdminConfig::perform_transfer_at(
                namespace,
                ADMIN_OFFSET,
                current,
                candidate,
                successor,
            )
            .map_err(|e| e.to_string())?;
        }
        Instruction::CreateSaleReference { sale_seed } => {
            let [namespace, sale, caller] = accounts.as_mut_slice() else {
                return Err("expected namespace, sale and caller".into());
            };
            let refs = references(namespace)?;
            if caller.account_id != refs.creator || !caller.is_authorized {
                return Err("creator authorization required".into());
            }
            if sale.account_id != sale_id(&program, namespace.account_id, sale_seed)
                || sale.account != Account::default()
            {
                return Err("invalid new sale reference".into());
            }
            sale.account.data = borsh::to_vec(&SaleReference {
                namespace: namespace.account_id,
                references: refs,
            })
            .map_err(|e| e.to_string())?
            .try_into()
            .map_err(|_| "sale reference too large")?;
            claim = AutoClaim::pda_from_seeds(&[
                &seed_from_str("sale"),
                namespace.account_id.as_ref(),
                &sale_seed,
            ]);
            claim_index = 1;
        }
        Instruction::Admit {
            sale_seed,
            operation,
        } => {
            let [namespace, sale, caller] = accounts.as_slice() else {
                return Err("expected namespace, sale and caller".into());
            };
            if sale.account_id != sale_id(&program, namespace.account_id, sale_seed)
                || sale.account.program_owner != program
            {
                return Err("wrong sale PDA or owner".into());
            }
            let reference: SaleReference =
                borsh::from_slice(&sale.account.data).map_err(|e| e.to_string())?;
            if reference.namespace != namespace.account_id {
                return Err("wrong sale namespace".into());
            }
            match operation {
                Operation::CurveSwap | Operation::LbpSwap => {
                    if !caller.is_authorized {
                        return Err("participant authorization required".into());
                    }
                    if FreezeConfig::from_account_at(namespace, FREEZE_OFFSET)
                        .map_err(|e| e.to_string())?
                        .is_frozen
                    {
                        return Err("namespace swaps are frozen".into());
                    }
                }
                Operation::CollectFees => {}
                Operation::Settle | Operation::Recover => {
                    if caller.account_id != reference.references.creator || !caller.is_authorized {
                        return Err("creator authorization required".into());
                    }
                }
            }
        }
        Instruction::Freeze | Instruction::Release => {
            let [namespace, caller] = accounts.as_mut_slice() else {
                return Err("expected namespace and caller".into());
            };
            if matches!(instruction, Instruction::Freeze) {
                FreezeConfig::perform_freeze_at(namespace, FREEZE_OFFSET, caller)
                    .map_err(|e| e.to_string())?;
            } else {
                FreezeConfig::perform_release_at(namespace, FREEZE_OFFSET, caller)
                    .map_err(|e| e.to_string())?;
            }
        }
        Instruction::RenounceAdmin => {
            let [namespace, caller] = accounts.as_mut_slice() else {
                return Err("expected namespace and caller".into());
            };
            AdminConfig::perform_renounce_at(namespace, ADMIN_OFFSET, caller)
                .map_err(|e| e.to_string())?;
        }
        Instruction::CheckAdmin => {
            let [namespace, caller] = accounts.as_slice() else {
                return Err("expected namespace and caller".into());
            };
            AdminConfig::from_account_at(namespace, ADMIN_OFFSET)
                .map_err(|e| e.to_string())?
                .assert_admin(caller)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(SpelOutput::execute(
        accounts.into_iter().enumerate().map(|(i, a)| {
            (
                a.account,
                if i == claim_index {
                    claim.clone()
                } else {
                    AutoClaim::None
                },
            )
        }),
        vec![],
    ))
}
