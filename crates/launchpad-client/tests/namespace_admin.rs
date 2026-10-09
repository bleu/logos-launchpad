//! Signed public transactions through the curve guest, including runtime PDA claims.
use curve_core::{Config, Instruction, compute_config_pda};
use launchpad_client::PublicInvocation;
use lee::{
    PrivateKey, PublicKey, PublicTransaction, V03State,
    error::LeeError,
    program::Program,
    public_transaction::{Message, WitnessSet},
};
use lee_core::account::{Account, AccountId};
use std::{collections::HashMap, path::PathBuf};

fn key(n: u8) -> PrivateKey {
    PrivateKey::try_new([n; 32]).unwrap()
}

fn id(n: u8) -> AccountId {
    AccountId::from(&PublicKey::new_from_private_key(&key(n)))
}

fn guest(name: &str) -> Program {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../methods/target/riscv-guest/launchpad_methods/launchpad_programs/riscv32im-risc0-zkvm-elf/release");
    launchpad_client::load_program(&root.join(format!("{name}.bin")))
        .expect("build methods before running guest tests")
}

struct Chain {
    curve: Program,
    accounts: HashMap<AccountId, Account>,
    programs: Vec<Program>,
}

impl Chain {
    fn new() -> Self {
        Self {
            curve: guest("curve"),
            accounts: (1..=9)
                .chain(std::iter::once(101))
                .map(|n| {
                    (
                        id(n),
                        Account {
                            program_owner: [77; 8],
                            ..Account::default()
                        },
                    )
                })
                .collect(),
            programs: vec![],
        }
    }

    fn run<I: serde::Serialize>(&mut self, call: PublicInvocation<I>) -> Result<(), LeeError> {
        let mut state = V03State::new()
            .with_public_accounts(self.accounts.clone())
            .with_programs(
                std::iter::once(self.curve.clone()).chain(self.programs.iter().cloned()),
            );
        let keys: Vec<_> = call
            .signer_accounts
            .iter()
            .map(|signer| {
                key((1..=9)
                    .chain(std::iter::once(101))
                    .find(|n| id(*n) == *signer)
                    .expect("fixture signing key"))
            })
            .collect();
        let nonces = call
            .signer_accounts
            .iter()
            .map(|id| state.get_account_by_id(*id).nonce)
            .collect();
        let ids = call.account_ids.clone();
        let message =
            Message::try_new(call.program_id, call.account_ids, nonces, call.instruction).unwrap();
        let witnesses = WitnessSet::for_message(&message, &keys.iter().collect::<Vec<_>>());
        if let Err(error) = state.transition_from_public_transaction(
            &PublicTransaction::new(message, witnesses),
            1,
            1,
        ) {
            for id in self.accounts.keys().chain(ids.iter()) {
                assert_eq!(
                    state.get_account_by_id(*id),
                    self.accounts.get(id).cloned().unwrap_or_default(),
                    "rejected transaction changed runtime account {id}"
                );
            }
            return Err(error);
        }
        for id in ids {
            self.accounts.insert(id, state.get_account_by_id(id));
        }
        Ok(())
    }

    fn call(
        &self,
        namespace: AccountId,
        admin: AccountId,
        instruction: Instruction,
    ) -> PublicInvocation<Instruction> {
        PublicInvocation {
            program_id: self.curve.id(),
            account_ids: vec![compute_config_pda(namespace, self.curve.id()), admin],
            signer_accounts: vec![admin],
            instruction,
        }
    }

    fn transfer(
        &self,
        namespace: AccountId,
        current: AccountId,
        successor: AccountId,
        candidate: curve_core::AdminCandidate,
    ) -> PublicInvocation<Instruction> {
        PublicInvocation {
            program_id: self.curve.id(),
            account_ids: vec![
                compute_config_pda(namespace, self.curve.id()),
                current,
                successor,
            ],
            signer_accounts: vec![current, successor],
            instruction: Instruction::TransferAdmin {
                namespace,
                new_admin: successor,
                candidate,
            },
        }
    }

    fn initialize(&mut self, namespace: AccountId, fee: u32, treasury: AccountId) {
        self.run(self.call(
            namespace,
            namespace,
            Instruction::InitializeNamespace {
                namespace,
                protocol_fee_bps: fee,
                treasury,
            },
        ))
        .unwrap();
    }

    fn config(&self, namespace: AccountId) -> Config {
        let account = &self.accounts[&compute_config_pda(namespace, self.curve.id())];
        assert_eq!(account.program_owner, self.curve.id(), "runtime PDA claim");
        Config::try_from(&account.data).unwrap()
    }
}

#[test]
fn signed_namespace_creation_assigns_creator_and_applies_config_claim() {
    let mut chain = Chain::new();
    let namespace = id(1);
    chain
        .run(chain.call(
            namespace,
            namespace,
            Instruction::InitializeNamespace {
                namespace,
                protocol_fee_bps: 30,
                treasury: id(4),
            },
        ))
        .unwrap();
    let config = chain.config(namespace);
    assert_eq!(config.protocol_fee_bps, 30);
    assert_eq!(config.treasury, id(4));
    assert_eq!(
        config.emergency.authority(),
        curve_core::INITIAL_EMERGENCY_AUTHORITY
    );
    assert!(!config.emergency.is_frozen);
    chain
        .run(chain.call(
            namespace,
            namespace,
            Instruction::UpdateFee {
                namespace,
                protocol_fee_bps: 30,
            },
        ))
        .unwrap();
}

#[test]
fn signed_setting_changes_preserve_other_namespace_fields() {
    let mut chain = Chain::new();
    let namespace = id(1);
    chain.initialize(namespace, 30, id(4));
    let initial = chain.config(namespace);
    chain
        .run(chain.call(
            namespace,
            namespace,
            Instruction::UpdateFee {
                namespace,
                protocol_fee_bps: 9_999,
            },
        ))
        .unwrap();
    let config = chain.config(namespace);
    assert_eq!(config.protocol_fee_bps, 9_999);
    assert_eq!(config.treasury, id(4));
    assert_eq!(config.emergency, initial.emergency);
    chain
        .run(chain.call(
            namespace,
            namespace,
            Instruction::UpdateTreasury {
                namespace,
                treasury: id(5),
            },
        ))
        .unwrap();
    let config = chain.config(namespace);
    assert_eq!(config.protocol_fee_bps, 9_999);
    assert_eq!(config.treasury, id(5));
    assert_eq!(config.emergency, initial.emergency);
    chain
        .run(chain.call(
            namespace,
            namespace,
            Instruction::UpdateFee {
                namespace,
                protocol_fee_bps: 0,
            },
        ))
        .unwrap();
    let config = chain.config(namespace);
    assert_eq!(config.protocol_fee_bps, 0);
    assert_eq!(config.treasury, id(5));
    assert_eq!(config.emergency, initial.emergency);
}

#[test]
fn signed_namespace_creation_rejects_invalid_inputs_without_claims() {
    let mut chain = Chain::new();
    let saved = chain.accounts.clone();
    for (namespace, signer, fee, treasury) in [
        (id(1), id(1), 10_000, id(4)),
        (id(1), id(1), u32::MAX, id(4)),
        (id(1), id(1), 30, AccountId::default()),
        (AccountId::default(), id(1), 30, id(4)),
        (id(1), id(2), 30, id(4)),
    ] {
        assert!(
            chain
                .run(chain.call(
                    namespace,
                    signer,
                    Instruction::InitializeNamespace {
                        namespace,
                        protocol_fee_bps: fee,
                        treasury,
                    },
                ))
                .is_err()
        );
        assert_eq!(chain.accounts, saved);
    }
    let initialize = chain.call(
        id(1),
        id(1),
        Instruction::InitializeNamespace {
            namespace: id(1),
            protocol_fee_bps: 30,
            treasury: id(4),
        },
    );
    let mut unsigned = initialize.clone();
    unsigned.signer_accounts.clear();
    assert!(chain.run(unsigned).is_err());
    assert_eq!(chain.accounts, saved);
    chain.run(initialize.clone()).unwrap();
    let initialized = chain.accounts.clone();
    assert!(chain.run(initialize).is_err());
    assert_eq!(chain.accounts, initialized);
}

#[test]
fn signed_namespace_changes_reject_invalid_and_foreign_authority_without_writes() {
    let mut chain = Chain::new();
    let namespace = id(1);
    let other = id(2);
    chain.initialize(namespace, 30, id(4));
    chain.initialize(other, 80, id(5));
    let saved = chain.accounts.clone();
    for instruction in [
        Instruction::UpdateFee {
            namespace,
            protocol_fee_bps: 10_000,
        },
        Instruction::UpdateFee {
            namespace,
            protocol_fee_bps: u32::MAX,
        },
        Instruction::UpdateTreasury {
            namespace,
            treasury: AccountId::default(),
        },
    ] {
        assert!(
            chain
                .run(chain.call(namespace, namespace, instruction))
                .is_err()
        );
        assert_eq!(chain.accounts, saved);
    }
    for instruction in [
        Instruction::UpdateFee {
            namespace,
            protocol_fee_bps: 75,
        },
        Instruction::UpdateTreasury {
            namespace,
            treasury: id(6),
        },
        Instruction::RenounceAdmin { namespace },
    ] {
        let mut unsigned = chain.call(namespace, namespace, instruction.clone());
        unsigned.signer_accounts.clear();
        assert!(chain.run(unsigned).is_err());
        assert_eq!(chain.accounts, saved);
        assert!(
            chain
                .run(chain.call(namespace, other, instruction.clone()))
                .is_err()
        );
        assert_eq!(chain.accounts, saved);
        let mut wrong_config = chain.call(namespace, namespace, instruction);
        wrong_config.account_ids[0] = compute_config_pda(other, chain.curve.id());
        assert!(chain.run(wrong_config).is_err());
        assert_eq!(chain.accounts, saved);
    }
    chain
        .run(chain.call(
            namespace,
            namespace,
            Instruction::UpdateFee {
                namespace,
                protocol_fee_bps: 75,
            },
        ))
        .unwrap();
    chain
        .run(chain.call(
            namespace,
            namespace,
            Instruction::UpdateTreasury {
                namespace,
                treasury: id(6),
            },
        ))
        .unwrap();
    let config = chain.config(namespace);
    assert_eq!(config.protocol_fee_bps, 75);
    assert_eq!(config.treasury, id(6));
    let other_config = compute_config_pda(other, chain.curve.id());
    assert_eq!(chain.accounts[&other_config], saved[&other_config]);
    assert_eq!(chain.accounts[&other], saved[&other]);
}

#[test]
fn signed_admin_transfer_requires_acceptance_and_rejects_old_signer() {
    let mut chain = Chain::new();
    let namespace = id(1);
    let successor = id(2);
    chain.initialize(namespace, 30, id(4));
    let initial = chain.config(namespace);
    let transfer = chain.transfer(
        namespace,
        namespace,
        successor,
        curve_core::AdminCandidate::Signer,
    );
    let saved = chain.accounts.clone();
    for missing in [namespace, successor] {
        let mut unsigned = transfer.clone();
        unsigned.signer_accounts.retain(|signer| *signer != missing);
        assert!(chain.run(unsigned).is_err());
        assert_eq!(chain.accounts, saved);
    }
    let mut wrong_successor = transfer.clone();
    wrong_successor.account_ids[2] = id(3);
    wrong_successor.signer_accounts[1] = id(3);
    assert!(chain.run(wrong_successor).is_err());
    assert_eq!(chain.accounts, saved);
    assert!(
        chain
            .run(chain.transfer(
                namespace,
                id(3),
                successor,
                curve_core::AdminCandidate::Signer
            ))
            .is_err()
    );
    assert_eq!(chain.accounts, saved);
    chain.run(transfer).unwrap();
    let config = chain.config(namespace);
    assert_eq!(config.protocol_fee_bps, 30);
    assert_eq!(config.treasury, id(4));
    assert_eq!(config.emergency, initial.emergency);
    let transferred = chain.accounts.clone();
    for instruction in [
        Instruction::UpdateFee {
            namespace,
            protocol_fee_bps: 80,
        },
        Instruction::UpdateTreasury {
            namespace,
            treasury: id(5),
        },
        Instruction::RenounceAdmin { namespace },
    ] {
        assert!(
            chain
                .run(chain.call(namespace, namespace, instruction))
                .is_err()
        );
        assert_eq!(chain.accounts, transferred);
    }
    assert!(
        chain
            .run(chain.transfer(
                namespace,
                namespace,
                id(3),
                curve_core::AdminCandidate::Signer
            ))
            .is_err()
    );
    assert_eq!(chain.accounts, transferred);
    chain
        .run(chain.call(
            namespace,
            successor,
            Instruction::UpdateFee {
                namespace,
                protocol_fee_bps: 80,
            },
        ))
        .unwrap();
    assert_eq!(chain.config(namespace).protocol_fee_bps, 80);
}

#[test]
fn signed_renunciation_is_terminal_and_preserves_settings_and_emergency_control() {
    let mut chain = Chain::new();
    let namespace = id(1);
    chain.initialize(namespace, 75, id(5));
    chain
        .run(chain.call(
            namespace,
            id(101),
            Instruction::SetFrozen {
                namespace,
                frozen: true,
            },
        ))
        .unwrap();
    chain
        .run(chain.call(
            namespace,
            id(101),
            Instruction::TransferEmergencyAuthority {
                namespace,
                new_authority: id(3),
            },
        ))
        .unwrap();
    let before = chain.config(namespace);
    chain
        .run(chain.call(
            namespace,
            namespace,
            Instruction::RenounceAdmin { namespace },
        ))
        .unwrap();
    let config = chain.config(namespace);
    assert_eq!(config.protocol_fee_bps, 75);
    assert_eq!(config.treasury, id(5));
    assert_eq!(config.emergency, before.emergency);
    assert_eq!(config.emergency.authority(), id(3));
    assert!(config.emergency.is_frozen);
    let renounced = chain.accounts.clone();
    for instruction in [
        Instruction::UpdateFee {
            namespace,
            protocol_fee_bps: 80,
        },
        Instruction::UpdateTreasury {
            namespace,
            treasury: id(6),
        },
        Instruction::RenounceAdmin { namespace },
        Instruction::InitializeNamespace {
            namespace,
            protocol_fee_bps: 30,
            treasury: id(4),
        },
    ] {
        assert!(
            chain
                .run(chain.call(namespace, namespace, instruction))
                .is_err()
        );
        assert_eq!(chain.accounts, renounced);
    }
    assert!(
        chain
            .run(chain.transfer(
                namespace,
                namespace,
                id(2),
                curve_core::AdminCandidate::Signer
            ))
            .is_err()
    );
    assert_eq!(chain.accounts, renounced);
    chain
        .run(chain.call(
            namespace,
            id(3),
            Instruction::SetFrozen {
                namespace,
                frozen: false,
            },
        ))
        .unwrap();
    let config = chain.config(namespace);
    assert_eq!(config.protocol_fee_bps, 75);
    assert_eq!(config.treasury, id(5));
    assert_eq!(config.emergency.authority(), id(3));
    assert!(!config.emergency.is_frozen);
}

#[test]
fn program_owned_administrator_requires_its_owner_program_call() {
    let mut chain = Chain::new();
    let namespace = id(1);
    chain.initialize(namespace, 30, id(4));
    let owner = guest("namespace_admin_owner");
    let seed = [91; 32];
    let pda = AccountId::for_public_pda(&owner.id(), &lee_core::program::PdaSeed::new(seed));
    chain.programs.push(owner.clone());
    let transfer = launchpad_client::build_transfer_admin_invocation(
        namespace,
        chain.curve.id(),
        namespace,
        pda,
        curve_core::AdminCandidate::Pda {
            program_id: owner.id(),
            seed,
        },
    );
    let saved = chain.accounts.clone();
    assert!(
        chain.run(transfer.clone()).is_err(),
        "unclaimed successor must reject"
    );
    assert_eq!(chain.accounts, saved);
    chain
        .run(PublicInvocation {
            program_id: owner.id(),
            account_ids: vec![pda, id(2)],
            signer_accounts: vec![id(2)],
            instruction: (chain.curve.id(), seed, None::<Instruction>),
        })
        .unwrap();
    assert_eq!(chain.accounts[&pda].program_owner, owner.id());
    let mut invalid = transfer.clone();
    invalid.instruction = Instruction::TransferAdmin {
        namespace,
        new_admin: pda,
        candidate: curve_core::AdminCandidate::Pda {
            program_id: owner.id(),
            seed: [92; 32],
        },
    };
    let claimed = chain.accounts.clone();
    assert!(chain.run(invalid).is_err());
    assert_eq!(chain.accounts, claimed);
    chain.run(transfer).unwrap();
    let before = chain.config(namespace);
    let action = Instruction::UpdateFee {
        namespace,
        protocol_fee_bps: 90,
    };
    let mut direct = chain.call(namespace, pda, action.clone());
    direct.signer_accounts.clear();
    let transferred = chain.accounts.clone();
    assert!(chain.run(direct).is_err());
    assert_eq!(chain.accounts, transferred);
    let call = PublicInvocation {
        program_id: owner.id(),
        account_ids: vec![pda, id(2), compute_config_pda(namespace, chain.curve.id())],
        signer_accounts: vec![id(2)],
        instruction: (chain.curve.id(), seed, Some(action)),
    };
    let mut wrong_controller = call.clone();
    wrong_controller.account_ids[1] = id(3);
    wrong_controller.signer_accounts[0] = id(3);
    assert!(chain.run(wrong_controller).is_err());
    assert_eq!(chain.accounts, transferred);
    chain.run(call).unwrap();
    let after = chain.config(namespace);
    assert_eq!(after.protocol_fee_bps, 90);
    assert_eq!(after.treasury, before.treasury);
    assert_eq!(after.emergency, before.emergency);
}
