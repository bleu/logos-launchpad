use authority_probe::{Instruction, namespace_id};
use risc0_zkvm::{ExecutorEnv, compute_image_id, default_prover};
use spel_framework::prelude::*;

const SEED: [u8; 32] = [10; 32];

struct Guest {
    elf: Vec<u8>,
    program: ProgramId,
}

impl Guest {
    fn load() -> Self {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("methods/target/riscv-guest/authority-probe-methods/authority-probe-guest/riscv32im-risc0-zkvm-elf/release/authority.bin");
        let elf = std::fs::read(path).expect("build the locked authority methods first");
        let program = compute_image_id(&elf).unwrap().into();
        Self { elf, program }
    }

    fn run(
        &self,
        seed: [u8; 32],
        accounts: Vec<AccountWithMetadata>,
        instruction: Instruction,
    ) -> Result<ProgramOutput, String> {
        let words = risc0_zkvm::serde::to_vec(&(seed, instruction)).unwrap();
        let env = ExecutorEnv::builder()
            .write(&self.program)
            .unwrap()
            .write(&Option::<ProgramId>::None)
            .unwrap()
            .write(&accounts)
            .unwrap()
            .write(&words)
            .unwrap()
            .build()
            .unwrap();
        let receipt = default_prover()
            .prove(env, &self.elf)
            .map_err(|e| e.to_string())?
            .receipt;
        receipt.verify(self.program).map_err(|e| e.to_string())?;
        let output: ProgramOutput = receipt.journal.decode().map_err(|e| e.to_string())?;
        assert_eq!(output.self_program_id, self.program);
        assert_eq!(output.pre_states, accounts);
        assert_eq!(output.instruction_data, words);
        lee_core::program::validate_execution(&accounts, &output.post_states, self.program)
            .map_err(|e| e.to_string())?;
        Ok(output)
    }
}

// These metadata flags are controlled inputs, not verified transaction signatures.
fn actor(id: u8, authorized: bool) -> AccountWithMetadata {
    AccountWithMetadata {
        account_id: AccountId::new([id; 32]),
        account: Account::default(),
        is_authorized: authorized,
    }
}

fn initialize(guest: &Guest) -> AccountWithMetadata {
    let mut namespace = AccountWithMetadata {
        account_id: namespace_id(&guest.program, SEED),
        account: Account::default(),
        is_authorized: false,
    };
    let output = guest
        .run(
            SEED,
            vec![namespace.clone(), actor(1, true), actor(2, true)],
            Instruction::Initialize {
                creator: actor(3, true).account_id,
                custody: actor(4, false).account_id,
            },
        )
        .expect("initialize through guest");
    assert_eq!(
        output.post_states[0].required_claim(),
        Some(Claim::Pda(PdaSeed::new(SEED)))
    );
    namespace.account = output.post_states[0].account().clone();
    assert_eq!(namespace.account.program_owner, ProgramId::default());
    // Prepare a claimed-account fixture, without claiming V03State application coverage.
    namespace.account.program_owner = guest.program;
    namespace
}

#[test]
fn guest_enforces_admin_transfer_renunciation_and_namespace_owner() {
    let guest = Guest::load();
    let mut namespace = initialize(&guest);
    let transfer = Instruction::TransferAdmin {
        candidate: admin_authority::AdminCandidate::Signer,
    };
    assert!(
        guest
            .run(
                SEED,
                vec![namespace.clone(), actor(1, true), actor(5, false)],
                transfer.clone()
            )
            .is_err()
    );
    let output = guest
        .run(
            SEED,
            vec![namespace.clone(), actor(1, true), actor(5, true)],
            transfer,
        )
        .unwrap();
    namespace.account = output.post_states[0].account().clone();
    assert!(
        guest
            .run(
                SEED,
                vec![namespace.clone(), actor(1, true)],
                Instruction::CheckAdmin
            )
            .is_err()
    );
    assert!(
        guest
            .run(
                SEED,
                vec![namespace.clone(), actor(5, false)],
                Instruction::CheckAdmin
            )
            .is_err()
    );
    assert!(
        guest
            .run(
                [11; 32],
                vec![namespace.clone(), actor(5, true)],
                Instruction::CheckAdmin
            )
            .is_err()
    );
    let mut wrong_owner = namespace.clone();
    wrong_owner.account.program_owner = [99; 8];
    assert!(
        guest
            .run(
                SEED,
                vec![wrong_owner, actor(5, true)],
                Instruction::CheckAdmin
            )
            .is_err()
    );
    let output = guest
        .run(
            SEED,
            vec![namespace.clone(), actor(5, true)],
            Instruction::RenounceAdmin,
        )
        .unwrap();
    namespace.account = output.post_states[0].account().clone();
    assert!(
        guest
            .run(
                SEED,
                vec![namespace.clone(), actor(5, true)],
                Instruction::CheckAdmin
            )
            .is_err()
    );
    let output = guest
        .run(SEED, vec![namespace, actor(2, true)], Instruction::Freeze)
        .expect("emergency authority survives renunciation");
    assert_eq!(output.post_states.len(), 2);
}

#[test]
fn guest_freeze_blocks_both_swap_kinds_but_preserves_non_swap_admission() {
    use authority_probe::Operation;
    let guest = Guest::load();
    let mut namespace = initialize(&guest);
    assert!(
        guest
            .run(
                SEED,
                vec![namespace.clone(), actor(3, true)],
                Instruction::Freeze
            )
            .is_err()
    );
    assert!(
        guest
            .run(
                SEED,
                vec![namespace.clone(), actor(2, false)],
                Instruction::Freeze
            )
            .is_err()
    );
    let output = guest
        .run(
            SEED,
            vec![namespace.clone(), actor(2, true)],
            Instruction::Freeze,
        )
        .unwrap();
    namespace.account = output.post_states[0].account().clone();
    let sale_seed = [20; 32];
    let mut sale = AccountWithMetadata {
        account_id: authority_probe::sale_id(&guest.program, namespace.account_id, sale_seed),
        account: Account::default(),
        is_authorized: false,
    };
    let output = guest
        .run(
            SEED,
            vec![namespace.clone(), sale.clone(), actor(3, true)],
            Instruction::CreateSaleReference { sale_seed },
        )
        .unwrap();
    let Some(Claim::Pda(seed)) = output.post_states[1].required_claim() else {
        panic!("sale creation must return a PDA claim");
    };
    assert_eq!(
        AccountId::for_public_pda(&guest.program, &seed),
        sale.account_id
    );
    sale.account = output.post_states[1].account().clone();
    assert_eq!(sale.account.program_owner, ProgramId::default());
    sale.account.program_owner = guest.program;
    for operation in [Operation::CurveSwap, Operation::LbpSwap] {
        assert!(
            guest
                .run(
                    SEED,
                    vec![namespace.clone(), sale.clone(), actor(9, true)],
                    Instruction::Admit {
                        sale_seed,
                        operation
                    }
                )
                .is_err()
        );
    }
    for (operation, caller) in [
        (Operation::CollectFees, actor(9, false)),
        (Operation::Settle, actor(3, true)),
        (Operation::Recover, actor(3, true)),
    ] {
        let output = guest
            .run(
                SEED,
                vec![namespace.clone(), sale.clone(), caller],
                Instruction::Admit {
                    sale_seed,
                    operation,
                },
            )
            .unwrap();
        assert_eq!(output.post_states[1].account(), &sale.account);
    }
    assert!(
        guest
            .run(
                SEED,
                vec![namespace.clone(), actor(1, true)],
                Instruction::Release
            )
            .is_err()
    );
    let output = guest
        .run(
            SEED,
            vec![namespace.clone(), actor(2, true)],
            Instruction::Release,
        )
        .unwrap();
    namespace.account = output.post_states[0].account().clone();
    for operation in [Operation::CurveSwap, Operation::LbpSwap] {
        let output = guest
            .run(
                SEED,
                vec![namespace.clone(), sale.clone(), actor(9, true)],
                Instruction::Admit {
                    sale_seed,
                    operation,
                },
            )
            .unwrap();
        assert_eq!(output.post_states[1].account(), &sale.account);
    }
}

#[test]
fn positional_initialization_with_one_account_in_both_roles_fails_runtime_validation() {
    let guest = Guest::load();
    let namespace = AccountWithMetadata {
        account_id: namespace_id(&guest.program, SEED),
        account: Account::default(),
        is_authorized: false,
    };
    let error = guest
        .run(
            SEED,
            vec![namespace, actor(1, true), actor(1, true)],
            Instruction::Initialize {
                creator: actor(3, true).account_id,
                custody: actor(4, false).account_id,
            },
        )
        .unwrap_err();
    assert_eq!(error, "Pre-state account IDs are not unique");
}

#[test]
fn guest_initializes_real_components_and_returns_valid_namespace_effects() {
    let guest = Guest::load();
    let namespace = initialize(&guest);
    assert_eq!(
        authority_probe::references(&namespace).unwrap().creator,
        actor(3, true).account_id
    );
    let output = guest
        .run(
            SEED,
            vec![namespace.clone(), actor(1, true)],
            Instruction::CheckAdmin,
        )
        .unwrap();
    assert_eq!(output.post_states[0].account(), &namespace.account);
}
