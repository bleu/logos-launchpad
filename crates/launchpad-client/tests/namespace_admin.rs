//! Signed public transactions through the real curve guest and V03State.
use curve_core::{AdminCandidate, Config, compute_config_pda};
use launchpad_client::{
    PublicInvocation, build_freeze_namespace_invocation, build_initialize_namespace_invocation,
    build_renounce_admin_invocation, build_transfer_admin_invocation,
    build_unfreeze_namespace_invocation, build_update_fee_invocation,
    build_update_treasury_invocation,
};
use lee::{
    PrivateKey, PublicKey, PublicTransaction, V03State,
    program::Program,
    public_transaction::{Message, WitnessSet},
};
use lee_core::account::{Account, AccountId, Data};
use std::{collections::HashMap, path::PathBuf};

fn key(n: u8) -> PrivateKey {
    PrivateKey::try_new([n; 32]).unwrap()
}
fn id(n: u8) -> AccountId {
    AccountId::from(&PublicKey::new_from_private_key(&key(n)))
}
fn guest(name: &str) -> Program {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../methods/target/riscv-guest/launchpad_methods/launchpad_programs/riscv32im-risc0-zkvm-elf/release");
    launchpad_client::load_program(&root.join(format!("{name}.bin"))).unwrap()
}
struct Chain {
    curve: Program,
    accounts: HashMap<AccountId, Account>,
    programs: Vec<Program>,
}
impl Chain {
    fn new(namespace: AccountId) -> Self {
        let mut chain = Self {
            curve: guest("curve"),
            accounts: HashMap::from([(
                id(1),
                Account {
                    program_owner: programs::token().id(),
                    data: Data::from(&token_core::TokenHolding::NftMaster {
                        definition_id: namespace,
                        print_balance: 1,
                    }),
                    ..Account::default()
                },
            )]),
            programs: vec![],
        };
        for n in 2..=9 {
            chain.accounts.insert(
                id(n),
                Account {
                    program_owner: [77; 8],
                    ..Account::default()
                },
            );
        }
        chain
    }
    fn run<I: serde::Serialize>(
        &mut self,
        call: PublicInvocation<I>,
    ) -> Result<(), lee::error::LeeError> {
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
                    .find(|n| id(*n) == *signer)
                    .expect("known signing key"))
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
                    "Rejected transaction changed complete account state for {id}"
                );
            }
            return Err(error);
        }
        for id in ids {
            self.accounts.insert(id, state.get_account_by_id(id));
        }
        Ok(())
    }
    fn config(&self, namespace: AccountId) -> Config {
        let account = &self.accounts[&compute_config_pda(namespace, self.curve.id())];
        assert_eq!(
            account.program_owner,
            self.curve.id(),
            "Runtime must apply the namespace PDA claim"
        );
        Config::try_from(&account.data).unwrap()
    }
}

#[test]
fn signed_namespace_lifecycle_applies_claims_and_rolls_back_rejections() {
    let namespace = AccountId::new([11; 32]);
    let mut chain = Chain::new(namespace);
    let program = chain.curve.id();
    let initialize = build_initialize_namespace_invocation(
        namespace,
        program,
        id(1),
        id(2),
        30,
        id(4),
        id(2),
        AdminCandidate::Signer,
    );
    for missing in [id(1), id(2)] {
        let mut unsigned = initialize.clone();
        unsigned.signer_accounts.retain(|signer| *signer != missing);
        assert!(chain.run(unsigned).is_err());
        assert!(
            !chain
                .accounts
                .contains_key(&compute_config_pda(namespace, program))
        );
    }
    chain.run(initialize.clone()).unwrap();
    assert_eq!(chain.config(namespace).current_admin(), Some(id(2)));
    assert!(chain.run(initialize.clone()).is_err());
    assert!(
        chain
            .run(build_update_fee_invocation(namespace, program, id(1), 80))
            .is_err()
    );
    chain
        .run(build_update_fee_invocation(namespace, program, id(2), 80))
        .unwrap();
    chain
        .run(build_update_treasury_invocation(
            namespace,
            program,
            id(2),
            id(5),
        ))
        .unwrap();
    let transfer =
        build_transfer_admin_invocation(namespace, program, id(2), id(3), AdminCandidate::Signer);
    for missing in [id(2), id(3)] {
        let mut unsigned = transfer.clone();
        unsigned.signer_accounts.retain(|signer| *signer != missing);
        assert!(chain.run(unsigned).is_err());
    }
    chain.run(transfer).unwrap();
    assert_eq!(chain.config(namespace).current_admin(), Some(id(3)));
    assert!(
        chain
            .run(build_update_fee_invocation(namespace, program, id(2), 90))
            .is_err()
    );
    chain
        .run(build_renounce_admin_invocation(namespace, program, id(3)))
        .unwrap();
    let config = chain.config(namespace);
    assert_eq!(config.current_admin(), None);
    assert_eq!(config.protocol_fee_bps, 80);
    assert_eq!(config.treasury, id(5));
    assert!(
        chain
            .run(build_update_fee_invocation(namespace, program, id(3), 90))
            .is_err()
    );
    assert!(
        chain
            .run(build_transfer_admin_invocation(
                namespace,
                program,
                id(3),
                id(2),
                AdminCandidate::Signer
            ))
            .is_err()
    );
    assert!(chain.run(initialize).is_err());
    assert!(
        chain
            .run(build_update_treasury_invocation(
                namespace,
                program,
                id(3),
                id(6)
            ))
            .is_err()
    );
    assert!(
        chain
            .run(build_renounce_admin_invocation(namespace, program, id(3)))
            .is_err()
    );
    assert!(
        chain
            .run(build_transfer_admin_invocation(
                namespace,
                program,
                id(3),
                id(3),
                AdminCandidate::Signer
            ))
            .is_err()
    );
}

#[test]
fn signed_alias_initialization_and_self_transfer_use_unique_accounts() {
    let namespace = AccountId::new([13; 32]);
    let mut chain = Chain::new(namespace);
    let program = chain.curve.id();
    let initialize = build_initialize_namespace_invocation(
        namespace,
        program,
        id(1),
        id(1),
        30,
        id(4),
        id(1),
        AdminCandidate::Signer,
    );
    assert_eq!(initialize.account_ids.len(), 2);
    assert_eq!(initialize.signer_accounts, vec![id(1)]);
    let mut unsigned = initialize.clone();
    unsigned.signer_accounts.clear();
    assert!(chain.run(unsigned).is_err());
    chain.run(initialize).unwrap();
    let transfer =
        build_transfer_admin_invocation(namespace, program, id(1), id(1), AdminCandidate::Signer);
    assert_eq!(transfer.account_ids.len(), 2);
    assert_eq!(transfer.signer_accounts, vec![id(1)]);
    chain.run(transfer).unwrap();
    assert_eq!(chain.config(namespace).current_admin(), Some(id(1)));
    chain
        .run(build_update_fee_invocation(namespace, program, id(1), 40))
        .unwrap();
    assert_eq!(chain.config(namespace).protocol_fee_bps, 40);
}

#[test]
fn signed_initialization_rejects_invalid_namespace_proofs_without_state_changes() {
    use token_core::TokenHolding::{Fungible, NftMaster, NftPrintedCopy};
    let namespace = AccountId::new([14; 32]);
    let valid = Data::from(&NftMaster {
        definition_id: namespace,
        print_balance: 1,
    });
    let cases = [
        (
            "spent",
            programs::token().id(),
            Data::from(&NftMaster {
                definition_id: namespace,
                print_balance: 0,
            }),
        ),
        (
            "multiple units",
            programs::token().id(),
            Data::from(&NftMaster {
                definition_id: namespace,
                print_balance: 2,
            }),
        ),
        (
            "printed copy",
            programs::token().id(),
            Data::from(&NftPrintedCopy {
                definition_id: namespace,
                owned: true,
            }),
        ),
        (
            "fungible",
            programs::token().id(),
            Data::from(&Fungible {
                definition_id: namespace,
                balance: 1,
            }),
        ),
        (
            "foreign definition",
            programs::token().id(),
            Data::from(&NftMaster {
                definition_id: id(9),
                print_balance: 1,
            }),
        ),
        ("forged owner", [99; 8], valid),
        (
            "malformed",
            programs::token().id(),
            vec![255].try_into().unwrap(),
        ),
    ];
    for (label, owner, data) in cases {
        let mut chain = Chain::new(namespace);
        let proof = chain.accounts.get_mut(&id(1)).unwrap();
        proof.program_owner = owner;
        proof.data = data;
        let initialize = build_initialize_namespace_invocation(
            namespace,
            chain.curve.id(),
            id(1),
            id(2),
            30,
            id(4),
            id(2),
            AdminCandidate::Signer,
        );
        assert!(chain.run(initialize).is_err(), "{label}");
    }
    // Missing proof and acceptance signatures are covered by the signed lifecycle test.
}

#[test]
fn signed_admin_operations_reject_invalid_config_state_and_roll_back() {
    let namespace = AccountId::new([15; 32]);
    let mut chain = Chain::new(namespace);
    let program = chain.curve.id();
    chain
        .run(build_initialize_namespace_invocation(
            namespace,
            program,
            id(1),
            id(2),
            30,
            id(4),
            id(2),
            AdminCandidate::Signer,
        ))
        .unwrap();
    let config_id = compute_config_pda(namespace, program);
    let original = chain.accounts[&config_id].clone();
    let mut legacy = id(2).to_bytes().to_vec();
    legacy.extend_from_slice(&30u16.to_le_bytes());
    legacy.extend_from_slice(&id(4).to_bytes());
    let mut magic = original.data.to_vec();
    magic[0] ^= 1;
    let mut version = original.data.to_vec();
    version[8] = 1;
    let mut trailing = original.data.to_vec();
    trailing.push(0);
    for bytes in [vec![], vec![255], legacy, magic, version, trailing] {
        chain.accounts.get_mut(&config_id).unwrap().data = bytes.try_into().unwrap();
        for call in [
            build_update_fee_invocation(namespace, program, id(2), 90),
            build_update_treasury_invocation(namespace, program, id(2), id(5)),
            build_transfer_admin_invocation(
                namespace,
                program,
                id(2),
                id(3),
                AdminCandidate::Signer,
            ),
            build_renounce_admin_invocation(namespace, program, id(2)),
        ] {
            assert!(chain.run(call).is_err());
        }
    }
    chain.accounts.insert(config_id, original.clone());
    chain.accounts.get_mut(&config_id).unwrap().program_owner = [99; 8];
    assert!(
        chain
            .run(build_update_fee_invocation(namespace, program, id(2), 90))
            .is_err()
    );
    chain.accounts.insert(config_id, original);
    let mut wrong_namespace = build_update_fee_invocation(id(9), program, id(2), 90);
    wrong_namespace.account_ids[0] = config_id;
    assert!(chain.run(wrong_namespace).is_err());
}

#[test]
fn signed_fee_boundaries_and_default_identities_are_checked_before_changes() {
    let namespace = AccountId::new([16; 32]);
    for fee in [0, 9_999, 10_000, 10_001, 65_536, u32::MAX] {
        let mut chain = Chain::new(namespace);
        let program = chain.curve.id();
        let mut initialize = build_initialize_namespace_invocation(
            namespace,
            program,
            id(1),
            id(2),
            0,
            id(4),
            id(2),
            AdminCandidate::Signer,
        );
        initialize.instruction = curve_core::Instruction::InitializeNamespace {
            namespace,
            admin: id(2),
            protocol_fee_bps: fee,
            treasury: id(4),
            emergency_authority: id(2),
            emergency_candidate: AdminCandidate::Signer,
        };
        let result = chain.run(initialize);
        assert_eq!(result.is_ok(), fee < 10_000, "initial fee {fee}");
        if result.is_err() {
            chain
                .run(build_initialize_namespace_invocation(
                    namespace,
                    program,
                    id(1),
                    id(2),
                    30,
                    id(4),
                    id(2),
                    AdminCandidate::Signer,
                ))
                .unwrap();
        }
        let mut update = build_update_fee_invocation(namespace, program, id(2), 0);
        update.instruction = curve_core::Instruction::UpdateFee {
            namespace,
            protocol_fee_bps: fee,
        };
        assert_eq!(chain.run(update).is_ok(), fee < 10_000, "updated fee {fee}");
    }
    let mut chain = Chain::new(namespace);
    let program = chain.curve.id();
    let mut no_admin = build_initialize_namespace_invocation(
        namespace,
        program,
        id(1),
        id(2),
        30,
        id(4),
        id(2),
        AdminCandidate::Signer,
    );
    no_admin.instruction = curve_core::Instruction::InitializeNamespace {
        namespace,
        admin: AccountId::default(),
        protocol_fee_bps: 30,
        treasury: id(4),
        emergency_authority: id(2),
        emergency_candidate: AdminCandidate::Signer,
    };
    assert!(chain.run(no_admin).is_err());
    assert!(
        chain
            .run(build_initialize_namespace_invocation(
                namespace,
                program,
                id(1),
                id(2),
                30,
                AccountId::default(),
                id(2),
                AdminCandidate::Signer
            ))
            .is_err()
    );
    chain
        .run(build_initialize_namespace_invocation(
            namespace,
            program,
            id(1),
            id(2),
            30,
            id(4),
            id(2),
            AdminCandidate::Signer,
        ))
        .unwrap();
    assert!(
        chain
            .run(build_update_treasury_invocation(
                namespace,
                program,
                id(2),
                AccountId::default()
            ))
            .is_err()
    );
    let mut no_successor =
        build_transfer_admin_invocation(namespace, program, id(2), id(3), AdminCandidate::Signer);
    no_successor.instruction = curve_core::Instruction::TransferAdmin {
        namespace,
        new_admin: AccountId::default(),
        candidate: AdminCandidate::Signer,
    };
    assert!(chain.run(no_successor).is_err());
}

#[test]
fn signed_initialization_rejects_missing_evidence_and_nonempty_config_accounts() {
    let namespace = AccountId::new([17; 32]);
    let mut chain = Chain::new(namespace);
    let program = chain.curve.id();
    let initialize = build_initialize_namespace_invocation(
        namespace,
        program,
        id(1),
        id(2),
        30,
        id(4),
        id(2),
        AdminCandidate::Signer,
    );
    let mut missing_acceptance = initialize.clone();
    missing_acceptance.account_ids.pop();
    missing_acceptance.signer_accounts.pop();
    assert!(chain.run(missing_acceptance).is_err());
    let mut missing_proof = initialize.clone();
    missing_proof.account_ids.remove(1);
    missing_proof.signer_accounts.remove(0);
    assert!(chain.run(missing_proof).is_err());
    let mut wrong_address = initialize.clone();
    wrong_address.account_ids[0] = id(9);
    assert!(chain.run(wrong_address).is_err());
    let config_id = initialize.account_ids[0];
    for account in [
        Account {
            balance: 1,
            ..Account::default()
        },
        Account {
            program_owner: program,
            ..Account::default()
        },
        Account {
            program_owner: [99; 8],
            ..Account::default()
        },
        Account {
            data: vec![1].try_into().unwrap(),
            ..Account::default()
        },
    ] {
        chain.accounts.insert(config_id, account);
        assert!(chain.run(initialize.clone()).is_err());
    }
    let mut empty_namespace = Chain::new(AccountId::default());
    assert!(
        empty_namespace
            .run(build_initialize_namespace_invocation(
                AccountId::default(),
                program,
                id(1),
                id(2),
                30,
                id(4),
                id(2),
                AdminCandidate::Signer
            ))
            .is_err()
    );
}

#[test]
fn signed_namespaces_are_isolated_and_every_admin_operation_requires_current_signature() {
    let a = AccountId::new([18; 32]);
    let b = AccountId::new([19; 32]);
    let mut chain = Chain::new(a);
    let program = chain.curve.id();
    chain.accounts.get_mut(&id(7)).unwrap().program_owner = programs::token().id();
    chain.accounts.get_mut(&id(7)).unwrap().data =
        Data::from(&token_core::TokenHolding::NftMaster {
            definition_id: b,
            print_balance: 1,
        });
    chain
        .run(build_initialize_namespace_invocation(
            a,
            program,
            id(1),
            id(2),
            30,
            id(4),
            id(2),
            AdminCandidate::Signer,
        ))
        .unwrap();
    chain
        .run(build_initialize_namespace_invocation(
            b,
            program,
            id(7),
            id(3),
            50,
            id(5),
            id(3),
            AdminCandidate::Signer,
        ))
        .unwrap();
    for caller in [id(1), id(2), id(3)] {
        for mut call in [
            build_update_fee_invocation(a, program, caller, 90),
            build_update_treasury_invocation(a, program, caller, id(6)),
            build_transfer_admin_invocation(a, program, caller, id(8), AdminCandidate::Signer),
            build_renounce_admin_invocation(a, program, caller),
        ] {
            if caller == id(2) {
                call.signer_accounts.retain(|signer| *signer != caller);
            }
            assert!(chain.run(call).is_err());
        }
    }
    let b_before = chain.accounts[&compute_config_pda(b, program)].clone();
    chain
        .run(build_update_fee_invocation(a, program, id(2), 90))
        .unwrap();
    chain
        .run(build_update_treasury_invocation(a, program, id(2), id(6)))
        .unwrap();
    chain
        .run(build_transfer_admin_invocation(
            a,
            program,
            id(2),
            id(8),
            AdminCandidate::Signer,
        ))
        .unwrap();
    chain
        .run(build_renounce_admin_invocation(a, program, id(8)))
        .unwrap();
    assert_eq!(chain.accounts[&compute_config_pda(b, program)], b_before);
    chain
        .run(build_update_fee_invocation(b, program, id(3), 100))
        .unwrap();
    assert_eq!(chain.config(a).protocol_fee_bps, 90);
    assert_eq!(chain.config(a).treasury, id(6));
}

#[test]
fn actual_owner_program_claims_and_administers_its_pda() {
    let namespace = AccountId::new([12; 32]);
    let mut chain = Chain::new(namespace);
    let program = chain.curve.id();
    let owner = guest("namespace_admin_owner");
    let seed = [42; 32];
    let pda = AccountId::for_public_pda(&owner.id(), &lee_core::program::PdaSeed::new(seed));
    chain.programs.push(owner.clone());
    chain
        .run(build_initialize_namespace_invocation(
            namespace,
            program,
            id(1),
            id(2),
            30,
            id(4),
            id(2),
            AdminCandidate::Signer,
        ))
        .unwrap();
    let transfer = build_transfer_admin_invocation(
        namespace,
        program,
        id(2),
        pda,
        AdminCandidate::Pda {
            program_id: owner.id(),
            seed,
        },
    );
    assert!(chain.run(transfer.clone()).is_err());
    chain.accounts.insert(
        pda,
        Account {
            program_owner: [99; 8],
            ..Account::default()
        },
    );
    assert!(chain.run(transfer.clone()).is_err());
    chain.accounts.remove(&pda);
    chain
        .run(PublicInvocation {
            program_id: owner.id(),
            account_ids: vec![pda, id(6)],
            signer_accounts: vec![id(6)],
            instruction: (program, seed, None::<curve_core::Instruction>),
        })
        .unwrap();
    assert_eq!(chain.accounts[&pda].program_owner, owner.id());
    chain.run(transfer).unwrap();
    let mut direct = build_update_fee_invocation(namespace, program, pda, 90);
    direct.signer_accounts.clear();
    assert!(chain.run(direct.clone()).is_err());
    let call = PublicInvocation {
        program_id: owner.id(),
        account_ids: vec![pda, id(6), compute_config_pda(namespace, program)],
        signer_accounts: vec![id(6)],
        instruction: (program, seed, Some(direct.instruction)),
    };
    let mut missing_controller = call.clone();
    missing_controller.signer_accounts.clear();
    assert!(chain.run(missing_controller).is_err());
    chain.run(call).unwrap();
    assert_eq!(chain.config(namespace).protocol_fee_bps, 90);
    chain
        .run(PublicInvocation {
            program_id: owner.id(),
            account_ids: vec![pda, id(6), compute_config_pda(namespace, program)],
            signer_accounts: vec![id(6)],
            instruction: (
                program,
                seed,
                Some(curve_core::Instruction::RenounceAdmin { namespace }),
            ),
        })
        .unwrap();
    assert_eq!(chain.config(namespace).current_admin(), None);
}

#[test]
fn signed_initialization_requires_separate_emergency_acceptance_atomically() {
    let namespace = AccountId::new([20; 32]);
    let mut chain = Chain::new(namespace);
    let program = chain.curve.id();
    let initialize = build_initialize_namespace_invocation(
        namespace,
        program,
        id(1),
        id(2),
        30,
        id(4),
        id(3),
        AdminCandidate::Signer,
    );
    let mut unsigned = initialize.clone();
    unsigned.signer_accounts.retain(|signer| *signer != id(3));
    assert!(chain.run(unsigned.clone()).is_err());
    unsigned.account_ids.retain(|account| *account != id(3));
    assert!(chain.run(unsigned).is_err());
    let mut invalid = build_initialize_namespace_invocation(
        namespace,
        program,
        id(1),
        id(2),
        30,
        id(4),
        AccountId::default(),
        AdminCandidate::Signer,
    );
    invalid
        .signer_accounts
        .retain(|signer| *signer != AccountId::default());
    assert!(chain.run(invalid).is_err());
    assert!(
        !chain
            .accounts
            .contains_key(&compute_config_pda(namespace, program))
    );
    chain.run(initialize).unwrap();
    let config = chain.config(namespace);
    assert_eq!(config.current_admin(), Some(id(2)));
    assert_eq!(config.emergency_authority(), Some(id(3)));
    assert!(!config.is_frozen());
}

#[test]
fn signed_emergency_calls_are_idempotent_but_always_require_emergency_signature() {
    let namespace = AccountId::new([21; 32]);
    let mut chain = Chain::new(namespace);
    let program = chain.curve.id();
    chain
        .run(build_initialize_namespace_invocation(
            namespace,
            program,
            id(1),
            id(2),
            30,
            id(4),
            id(3),
            AdminCandidate::Signer,
        ))
        .unwrap();
    for frozen in [false, true, true, false, false] {
        for caller in [id(1), id(2), id(3)] {
            for mut call in [
                build_freeze_namespace_invocation(namespace, program, caller),
                build_unfreeze_namespace_invocation(namespace, program, caller),
            ] {
                if caller == id(3) {
                    call.signer_accounts.clear();
                }
                assert!(chain.run(call).is_err());
            }
        }
        let before = chain.accounts[&compute_config_pda(namespace, program)].clone();
        let was_frozen = chain.config(namespace).is_frozen();
        let call = if frozen {
            build_freeze_namespace_invocation(namespace, program, id(3))
        } else {
            build_unfreeze_namespace_invocation(namespace, program, id(3))
        };
        chain.run(call).unwrap();
        assert_eq!(chain.config(namespace).is_frozen(), frozen);
        if was_frozen == frozen {
            assert_eq!(
                chain.accounts[&compute_config_pda(namespace, program)],
                before
            );
        }
    }
}

#[test]
fn signed_shared_roles_keep_emergency_control_after_admin_transfer_and_renounce() {
    let namespace = AccountId::new([22; 32]);
    let mut chain = Chain::new(namespace);
    let program = chain.curve.id();
    chain
        .run(build_initialize_namespace_invocation(
            namespace,
            program,
            id(1),
            id(2),
            30,
            id(4),
            id(2),
            AdminCandidate::Signer,
        ))
        .unwrap();
    chain
        .run(build_freeze_namespace_invocation(namespace, program, id(2)))
        .unwrap();
    chain
        .run(build_transfer_admin_invocation(
            namespace,
            program,
            id(2),
            id(3),
            AdminCandidate::Signer,
        ))
        .unwrap();
    assert_eq!(chain.config(namespace).current_admin(), Some(id(3)));
    assert_eq!(chain.config(namespace).emergency_authority(), Some(id(2)));
    assert!(chain.config(namespace).is_frozen());
    assert!(
        chain
            .run(build_unfreeze_namespace_invocation(
                namespace,
                program,
                id(3)
            ))
            .is_err()
    );
    chain
        .run(build_renounce_admin_invocation(namespace, program, id(3)))
        .unwrap();
    assert_eq!(chain.config(namespace).current_admin(), None);
    assert_eq!(chain.config(namespace).emergency_authority(), Some(id(2)));
    assert!(chain.config(namespace).is_frozen());
    chain
        .run(build_unfreeze_namespace_invocation(
            namespace,
            program,
            id(2),
        ))
        .unwrap();
    assert!(!chain.config(namespace).is_frozen());
    chain
        .run(build_freeze_namespace_invocation(namespace, program, id(2)))
        .unwrap();
    assert!(chain.config(namespace).is_frozen());
}

#[test]
fn signed_emergency_calls_require_canonical_owned_well_formed_namespace_state() {
    let namespace = AccountId::new([23; 32]);
    let mut chain = Chain::new(namespace);
    let program = chain.curve.id();
    chain
        .run(build_initialize_namespace_invocation(
            namespace,
            program,
            id(1),
            id(2),
            30,
            id(4),
            id(3),
            AdminCandidate::Signer,
        ))
        .unwrap();
    let config_id = compute_config_pda(namespace, program);
    let original = chain.accounts[&config_id].clone();
    let calls = [
        build_freeze_namespace_invocation(namespace, program, id(3)),
        build_unfreeze_namespace_invocation(namespace, program, id(3)),
    ];
    let mut old_version = original.data.to_vec();
    old_version[8] = 1;
    let mut trailing = original.data.to_vec();
    trailing.push(0);
    for bytes in [vec![], vec![255], old_version, trailing] {
        chain.accounts.get_mut(&config_id).unwrap().data = bytes.try_into().unwrap();
        for call in &calls {
            assert!(chain.run(call.clone()).is_err());
        }
    }
    chain.accounts.insert(config_id, original.clone());
    chain.accounts.get_mut(&config_id).unwrap().program_owner = [99; 8];
    for call in &calls {
        assert!(chain.run(call.clone()).is_err());
    }
    chain.accounts.insert(config_id, original.clone());
    chain.accounts.insert(id(9), original);
    for mut call in calls {
        call.account_ids[0] = id(9);
        assert!(chain.run(call).is_err());
    }
    for mut call in [
        build_freeze_namespace_invocation(id(8), program, id(3)),
        build_unfreeze_namespace_invocation(id(8), program, id(3)),
    ] {
        call.account_ids[0] = config_id;
        assert!(chain.run(call).is_err());
    }
    chain
        .run(build_freeze_namespace_invocation(namespace, program, id(3)))
        .unwrap();
    assert!(chain.config(namespace).is_frozen());
}

#[test]
fn actual_owner_program_controls_claimed_emergency_pda_with_seed_authorization() {
    let namespace = AccountId::new([24; 32]);
    let mut chain = Chain::new(namespace);
    let program = chain.curve.id();
    let owner = guest("namespace_admin_owner");
    let seed = [42; 32];
    let pda = AccountId::for_public_pda(&owner.id(), &lee_core::program::PdaSeed::new(seed));
    chain.programs.push(owner.clone());
    let initialize = build_initialize_namespace_invocation(
        namespace,
        program,
        id(1),
        id(2),
        30,
        id(4),
        pda,
        AdminCandidate::Pda {
            program_id: owner.id(),
            seed,
        },
    );
    assert!(chain.run(initialize.clone()).is_err(), "unclaimed PDA");
    chain.accounts.insert(
        pda,
        Account {
            program_owner: [99; 8],
            ..Account::default()
        },
    );
    assert!(chain.run(initialize.clone()).is_err(), "wrong PDA owner");
    chain.accounts.remove(&pda);
    chain
        .run(PublicInvocation {
            program_id: owner.id(),
            account_ids: vec![pda, id(6)],
            signer_accounts: vec![id(6)],
            instruction: (program, seed, None::<curve_core::Instruction>),
        })
        .unwrap();
    assert_eq!(chain.accounts[&pda].program_owner, owner.id());
    let wrong_derivation = build_initialize_namespace_invocation(
        namespace,
        program,
        id(1),
        id(2),
        30,
        id(4),
        pda,
        AdminCandidate::Pda {
            program_id: owner.id(),
            seed: [43; 32],
        },
    );
    assert!(chain.run(wrong_derivation).is_err());
    chain.run(initialize).unwrap();
    assert_eq!(chain.config(namespace).emergency_authority(), Some(pda));
    for (mut direct, frozen) in [
        (
            build_freeze_namespace_invocation(namespace, program, pda),
            true,
        ),
        (
            build_unfreeze_namespace_invocation(namespace, program, pda),
            false,
        ),
    ] {
        direct.signer_accounts.clear();
        assert!(chain.run(direct.clone()).is_err());
        let call = PublicInvocation {
            program_id: owner.id(),
            account_ids: vec![pda, id(6), compute_config_pda(namespace, program)],
            signer_accounts: vec![id(6)],
            instruction: (program, seed, Some(direct.instruction)),
        };
        let mut unsigned = call.clone();
        unsigned.signer_accounts.clear();
        assert!(chain.run(unsigned).is_err());
        let mut wrong_controller = call.clone();
        wrong_controller.account_ids[1] = id(7);
        wrong_controller.signer_accounts = vec![id(7)];
        assert!(chain.run(wrong_controller).is_err());
        chain.run(call).unwrap();
        assert_eq!(chain.config(namespace).is_frozen(), frozen);
    }
}

#[test]
fn actual_owner_program_initializes_both_roles_with_one_authorized_pda() {
    let namespace = AccountId::new([25; 32]);
    let mut chain = Chain::new(namespace);
    let program = chain.curve.id();
    let owner = guest("namespace_admin_owner");
    let seed = [42; 32];
    let pda = AccountId::for_public_pda(&owner.id(), &lee_core::program::PdaSeed::new(seed));
    chain.programs.push(owner.clone());
    chain
        .run(PublicInvocation {
            program_id: owner.id(),
            account_ids: vec![pda, id(6)],
            signer_accounts: vec![id(6)],
            instruction: (program, seed, None::<curve_core::Instruction>),
        })
        .unwrap();
    let mut initialize = build_initialize_namespace_invocation(
        namespace,
        program,
        id(1),
        pda,
        30,
        id(4),
        pda,
        AdminCandidate::Pda {
            program_id: owner.id(),
            seed,
        },
    );
    assert_eq!(initialize.account_ids.len(), 3);
    initialize.signer_accounts.retain(|signer| *signer != pda);
    assert!(chain.run(initialize.clone()).is_err());
    let call = PublicInvocation {
        program_id: owner.id(),
        account_ids: vec![pda, id(6), compute_config_pda(namespace, program), id(1)],
        signer_accounts: vec![id(6), id(1)],
        instruction: (program, seed, Some(initialize.instruction)),
    };
    for missing in [id(6), id(1)] {
        let mut unsigned = call.clone();
        unsigned.signer_accounts.retain(|signer| *signer != missing);
        assert!(chain.run(unsigned).is_err());
    }
    chain.run(call).unwrap();
    assert_eq!(chain.config(namespace).current_admin(), Some(pda));
    assert_eq!(chain.config(namespace).emergency_authority(), Some(pda));
    assert!(!chain.config(namespace).is_frozen());
    for (call, frozen) in [
        (
            build_freeze_namespace_invocation(namespace, program, pda),
            true,
        ),
        (
            build_unfreeze_namespace_invocation(namespace, program, pda),
            false,
        ),
    ] {
        chain
            .run(PublicInvocation {
                program_id: owner.id(),
                account_ids: vec![pda, id(6), compute_config_pda(namespace, program)],
                signer_accounts: vec![id(6)],
                instruction: (program, seed, Some(call.instruction)),
            })
            .unwrap();
        assert_eq!(chain.config(namespace).is_frozen(), frozen);
    }
}
