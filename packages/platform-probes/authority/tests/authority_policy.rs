use authority_probe::{Instruction, dispatch, namespace_id};
use spel_framework::prelude::*;

const PROGRAM: ProgramId = [42; 8];
const SEED: [u8; 32] = [10; 32];

// Host authorization flags are fixtures, not cryptographic signature evidence.
fn actor(id: u8, authorized: bool) -> AccountWithMetadata {
    AccountWithMetadata {
        account_id: AccountId::new([id; 32]),
        account: Account::default(),
        is_authorized: authorized,
    }
}

fn empty_namespace(seed: [u8; 32]) -> AccountWithMetadata {
    AccountWithMetadata {
        account_id: namespace_id(&PROGRAM, seed),
        account: Account::default(),
        is_authorized: false,
    }
}

fn initialize(seed: [u8; 32]) -> AccountWithMetadata {
    let mut namespace = empty_namespace(seed);
    let output = dispatch(
        PROGRAM,
        seed,
        vec![namespace.clone(), actor(1, true), actor(2, true)],
        Instruction::Initialize {
            creator: actor(3, true).account_id,
            custody: actor(4, false).account_id,
        },
    )
    .expect("atomic namespace initialization");
    assert_eq!(output.post_states.len(), 3);
    assert_eq!(
        output.post_states[0].required_claim(),
        Some(Claim::Pda(PdaSeed::new(seed)))
    );
    namespace.account = output.post_states[0].account().clone();
    assert_eq!(namespace.account.program_owner, ProgramId::default());
    // Later calls use a claimed-account fixture; this does not apply a LEZ transaction.
    namespace.account.program_owner = PROGRAM;
    namespace
}

#[test]
fn namespace_accounts_must_match_the_requested_pda_and_program_owner() {
    let namespace = initialize(SEED);
    assert!(
        dispatch(
            PROGRAM,
            [11; 32],
            vec![namespace.clone(), actor(1, true)],
            Instruction::CheckAdmin
        )
        .is_err()
    );
    let mut wrong_owner = namespace;
    wrong_owner.account.program_owner = [99; 8];
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![wrong_owner, actor(1, true)],
            Instruction::CheckAdmin
        )
        .is_err()
    );
    let mut wrong_address = empty_namespace(SEED);
    wrong_address.account_id = actor(99, false).account_id;
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![wrong_address, actor(1, true), actor(2, true)],
            Instruction::Initialize {
                creator: actor(3, true).account_id,
                custody: actor(4, false).account_id
            }
        )
        .is_err()
    );
}

#[test]
fn namespace_initialization_cannot_reappoint_roles_or_accept_unsigned_authorities() {
    let instruction = Instruction::Initialize {
        creator: actor(3, true).account_id,
        custody: actor(4, false).account_id,
    };
    let namespace = initialize(SEED);
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![namespace, actor(9, true), actor(8, true)],
            instruction.clone()
        )
        .is_err()
    );
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![empty_namespace(SEED), actor(1, false), actor(2, true)],
            instruction.clone()
        )
        .is_err()
    );
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![empty_namespace(SEED), actor(1, true), actor(2, false)],
            instruction.clone()
        )
        .is_err()
    );
    let mut claimed = empty_namespace(SEED);
    claimed.account.program_owner = [99; 8];
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![claimed, actor(1, true), actor(2, true)],
            instruction
        )
        .is_err()
    );
}

#[test]
fn admin_transfer_requires_both_signers_and_preserves_namespace_references() {
    let mut namespace = initialize(SEED);
    let before = authority_probe::references(&namespace).unwrap();
    let transfer = Instruction::TransferAdmin {
        candidate: admin_authority::AdminCandidate::Signer,
    };
    for (current, successor) in [
        (actor(1, false), actor(5, true)),
        (actor(9, true), actor(5, true)),
        (actor(1, true), actor(5, false)),
    ] {
        assert!(
            dispatch(
                PROGRAM,
                SEED,
                vec![namespace.clone(), current, successor],
                transfer.clone()
            )
            .is_err()
        );
    }
    let output = dispatch(
        PROGRAM,
        SEED,
        vec![namespace.clone(), actor(1, true), actor(5, true)],
        transfer,
    )
    .expect("co-signed transfer");
    assert!(output.post_states[0].required_claim().is_none());
    namespace.account = output.post_states[0].account().clone();
    assert_eq!(namespace.account_id, namespace_id(&PROGRAM, SEED));
    assert_eq!(authority_probe::references(&namespace).unwrap(), before);
    for caller in [actor(1, true), actor(5, false), actor(9, true)] {
        assert!(
            dispatch(
                PROGRAM,
                SEED,
                vec![namespace.clone(), caller],
                Instruction::CheckAdmin
            )
            .is_err()
        );
    }
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![namespace, actor(5, true)],
            Instruction::CheckAdmin
        )
        .is_ok()
    );
}

#[test]
fn fixed_emergency_signer_can_accept_admin_without_changing_its_emergency_role() {
    let mut namespace = initialize(SEED);
    let transfer = Instruction::TransferAdmin {
        candidate: admin_authority::AdminCandidate::Signer,
    };
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![namespace.clone(), actor(1, true), actor(2, false)],
            transfer.clone()
        )
        .is_err()
    );
    let output = dispatch(
        PROGRAM,
        SEED,
        vec![namespace.clone(), actor(1, true), actor(2, true)],
        transfer,
    )
    .expect("emergency signer can accept the separate admin role");
    namespace.account = output.post_states[0].account().clone();
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![namespace.clone(), actor(2, true)],
            Instruction::CheckAdmin
        )
        .is_ok()
    );
    change(&mut namespace, actor(2, true), Instruction::Freeze);
    change(&mut namespace, actor(2, true), Instruction::RenounceAdmin);
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![namespace.clone(), actor(2, true)],
            Instruction::CheckAdmin
        )
        .is_err()
    );
    change(&mut namespace, actor(2, true), Instruction::Release);
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![namespace.clone(), actor(1, true)],
            Instruction::Freeze
        )
        .is_err()
    );
    assert_eq!(
        authority_probe::references(&namespace).unwrap().creator,
        actor(3, true).account_id
    );
}

#[test]
fn admin_renunciation_is_terminal_and_preserves_unrelated_references() {
    let mut namespace = initialize(SEED);
    let before = authority_probe::references(&namespace).unwrap();
    for caller in [actor(1, false), actor(2, true), actor(9, true)] {
        assert!(
            dispatch(
                PROGRAM,
                SEED,
                vec![namespace.clone(), caller],
                Instruction::RenounceAdmin
            )
            .is_err()
        );
    }
    let output = dispatch(
        PROGRAM,
        SEED,
        vec![namespace.clone(), actor(1, true)],
        Instruction::RenounceAdmin,
    )
    .unwrap();
    namespace.account = output.post_states[0].account().clone();
    assert_eq!(authority_probe::references(&namespace).unwrap(), before);
    for caller in [actor(1, true), actor(0, true)] {
        assert!(
            dispatch(
                PROGRAM,
                SEED,
                vec![namespace.clone(), caller.clone()],
                Instruction::CheckAdmin
            )
            .is_err()
        );
        assert!(
            dispatch(
                PROGRAM,
                SEED,
                vec![namespace.clone(), caller, actor(5, true)],
                Instruction::TransferAdmin {
                    candidate: admin_authority::AdminCandidate::Signer
                }
            )
            .is_err()
        );
    }
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![namespace, actor(1, true), actor(2, true)],
            Instruction::Initialize {
                creator: before.creator,
                custody: before.custody
            }
        )
        .is_err()
    );
}

#[test]
fn pda_successor_requires_derived_address_and_claimed_program_owner() {
    let mut namespace = initialize(SEED);
    let owner = [77; 8];
    let seed = [88; 32];
    let mut successor = AccountWithMetadata {
        account_id: AccountId::for_public_pda(&owner, &PdaSeed::new(seed)),
        account: Account::default(),
        is_authorized: false,
    };
    let transfer = Instruction::TransferAdmin {
        candidate: admin_authority::AdminCandidate::Pda {
            program_id: owner,
            seed,
        },
    };
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![namespace.clone(), actor(1, true), actor(9, false)],
            transfer.clone()
        )
        .is_err()
    );
    successor.account.balance = 1;
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![namespace.clone(), actor(1, true), successor.clone()],
            transfer.clone()
        )
        .is_err()
    );
    successor.account.program_owner = [76; 8];
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![namespace.clone(), actor(1, true), successor.clone()],
            transfer.clone()
        )
        .is_err()
    );
    successor.account.program_owner = owner;
    let output = dispatch(
        PROGRAM,
        SEED,
        vec![namespace.clone(), actor(1, true), successor.clone()],
        transfer,
    )
    .expect("claimed PDA accepts without signer co-signature");
    namespace.account = output.post_states[0].account().clone();
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![namespace.clone(), successor.clone()],
            Instruction::CheckAdmin
        )
        .is_err()
    );
    successor.is_authorized = true;
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![namespace, successor],
            Instruction::CheckAdmin
        )
        .is_ok()
    );
}

fn change(
    namespace: &mut AccountWithMetadata,
    caller: AccountWithMetadata,
    instruction: Instruction,
) {
    let output = dispatch(PROGRAM, SEED, vec![namespace.clone(), caller], instruction).unwrap();
    namespace.account = output.post_states[0].account().clone();
}

#[test]
fn fixed_emergency_signer_alone_can_freeze_and_release_after_admin_changes() {
    let mut namespace = initialize(SEED);
    for instruction in [Instruction::Freeze, Instruction::Release] {
        for caller in [
            actor(1, true),
            actor(3, true),
            actor(2, false),
            actor(9, true),
        ] {
            assert!(
                dispatch(
                    PROGRAM,
                    SEED,
                    vec![namespace.clone(), caller],
                    instruction.clone()
                )
                .is_err()
            );
        }
    }
    change(&mut namespace, actor(2, true), Instruction::Freeze);
    assert!(
        freeze_authority::FreezeConfig::from_account_at(
            &namespace,
            admin_authority::AdminConfig::SIZE
        )
        .unwrap()
        .is_frozen
    );
    let output = dispatch(
        PROGRAM,
        SEED,
        vec![namespace.clone(), actor(1, true), actor(5, true)],
        Instruction::TransferAdmin {
            candidate: admin_authority::AdminCandidate::Signer,
        },
    )
    .unwrap();
    namespace.account = output.post_states[0].account().clone();
    change(&mut namespace, actor(5, true), Instruction::RenounceAdmin);
    assert!(
        dispatch(
            PROGRAM,
            SEED,
            vec![namespace.clone(), actor(5, true)],
            Instruction::Release
        )
        .is_err()
    );
    change(&mut namespace, actor(2, true), Instruction::Release);
    assert!(
        !freeze_authority::FreezeConfig::from_account_at(
            &namespace,
            admin_authority::AdminConfig::SIZE
        )
        .unwrap()
        .is_frozen
    );
    change(&mut namespace, actor(2, true), Instruction::Freeze);
    assert_eq!(
        authority_probe::references(&namespace).unwrap().creator,
        actor(3, true).account_id
    );
}

fn create_sale(
    namespace: &AccountWithMetadata,
    seed: [u8; 32],
    sale_seed: [u8; 32],
) -> AccountWithMetadata {
    let mut sale = AccountWithMetadata {
        account_id: authority_probe::sale_id(&PROGRAM, namespace.account_id, sale_seed),
        account: Account::default(),
        is_authorized: false,
    };
    let output = dispatch(
        PROGRAM,
        seed,
        vec![namespace.clone(), sale.clone(), actor(3, true)],
        Instruction::CreateSaleReference { sale_seed },
    )
    .unwrap();
    assert_eq!(output.post_states[0].account(), &namespace.account);
    assert!(output.post_states[1].required_claim().is_some());
    sale.account = output.post_states[1].account().clone();
    assert_eq!(sale.account.program_owner, ProgramId::default());
    sale.account.program_owner = PROGRAM;
    sale
}

#[test]
fn namespace_freeze_blocks_both_mechanisms_including_sales_created_while_frozen() {
    let mut namespace = initialize(SEED);
    let other = initialize([11; 32]);
    let first = create_sale(&namespace, SEED, [20; 32]);
    let other_sale = create_sale(&other, [11; 32], [20; 32]);
    for operation in [
        authority_probe::Operation::CurveSwap,
        authority_probe::Operation::LbpSwap,
    ] {
        assert!(
            dispatch(
                PROGRAM,
                SEED,
                vec![namespace.clone(), first.clone(), actor(9, true)],
                Instruction::Admit {
                    sale_seed: [20; 32],
                    operation
                }
            )
            .is_ok()
        );
    }
    change(&mut namespace, actor(2, true), Instruction::Freeze);
    let late = create_sale(&namespace, SEED, [21; 32]);
    for operation in [
        authority_probe::Operation::CurveSwap,
        authority_probe::Operation::LbpSwap,
    ] {
        for (sale, sale_seed) in [(first.clone(), [20; 32]), (late.clone(), [21; 32])] {
            assert!(
                dispatch(
                    PROGRAM,
                    SEED,
                    vec![namespace.clone(), sale, actor(9, true)],
                    Instruction::Admit {
                        sale_seed,
                        operation
                    }
                )
                .is_err()
            );
        }
        assert!(
            dispatch(
                PROGRAM,
                [11; 32],
                vec![other.clone(), other_sale.clone(), actor(9, true)],
                Instruction::Admit {
                    sale_seed: [20; 32],
                    operation
                }
            )
            .is_ok()
        );
    }
    change(&mut namespace, actor(2, true), Instruction::Release);
    for operation in [
        authority_probe::Operation::CurveSwap,
        authority_probe::Operation::LbpSwap,
    ] {
        assert!(
            dispatch(
                PROGRAM,
                SEED,
                vec![namespace.clone(), late.clone(), actor(9, true)],
                Instruction::Admit {
                    sale_seed: [21; 32],
                    operation
                }
            )
            .is_ok()
        );
    }
}

#[test]
fn frozen_namespaces_keep_fee_settlement_and_recovery_authority_checks() {
    use authority_probe::Operation;
    let mut namespace = initialize(SEED);
    let sale = create_sale(&namespace, SEED, [20; 32]);
    change(&mut namespace, actor(2, true), Instruction::Freeze);
    for (operation, caller) in [
        (Operation::CollectFees, actor(9, false)),
        (Operation::Settle, actor(3, true)),
        (Operation::Recover, actor(3, true)),
    ] {
        let accounts = vec![namespace.clone(), sale.clone(), caller];
        let output = dispatch(
            PROGRAM,
            SEED,
            accounts.clone(),
            Instruction::Admit {
                sale_seed: [20; 32],
                operation,
            },
        )
        .expect("freeze does not block non-swap admission");
        for (post, pre) in output.post_states.iter().zip(&accounts) {
            assert_eq!(
                post.account(),
                &pre.account,
                "admission must not perform asset transfers"
            );
        }
    }
    for operation in [Operation::Settle, Operation::Recover] {
        for caller in [
            actor(1, true),
            actor(2, true),
            actor(3, false),
            actor(9, true),
        ] {
            assert!(
                dispatch(
                    PROGRAM,
                    SEED,
                    vec![namespace.clone(), sale.clone(), caller],
                    Instruction::Admit {
                        sale_seed: [20; 32],
                        operation
                    }
                )
                .is_err()
            );
        }
    }
}

#[test]
fn swap_admission_requires_participant_authorization_and_bound_sale_accounts() {
    let namespace = initialize(SEED);
    let other = initialize([11; 32]);
    let sale = create_sale(&namespace, SEED, [20; 32]);
    for operation in [
        authority_probe::Operation::CurveSwap,
        authority_probe::Operation::LbpSwap,
    ] {
        let instruction = Instruction::Admit {
            sale_seed: [20; 32],
            operation,
        };
        assert!(
            dispatch(
                PROGRAM,
                SEED,
                vec![namespace.clone(), sale.clone(), actor(9, false)],
                instruction.clone()
            )
            .is_err()
        );
        assert!(
            dispatch(
                PROGRAM,
                [11; 32],
                vec![other.clone(), sale.clone(), actor(9, true)],
                instruction.clone()
            )
            .is_err()
        );
        let mut wrong_owner = sale.clone();
        wrong_owner.account.program_owner = [99; 8];
        assert!(
            dispatch(
                PROGRAM,
                SEED,
                vec![namespace.clone(), wrong_owner, actor(9, true)],
                instruction.clone()
            )
            .is_err()
        );
        assert!(
            dispatch(
                PROGRAM,
                SEED,
                vec![namespace.clone(), sale.clone(), actor(9, true)],
                instruction
            )
            .is_ok()
        );
    }
}

#[test]
fn manual_instruction_set_has_no_emergency_role_management_endpoints() {
    for name in [
        "FreezeAuthorityTransfer",
        "FreezeAuthorityRenounce",
        "FreezeInitialize",
        "ReappointEmergency",
        "CreatorPause",
        "CreatorResume",
    ] {
        let encoded = format!("\"{name}\"");
        assert!(spel_framework::serde_json::from_str::<Instruction>(&encoded).is_err());
    }
}

#[test]
fn admin_changes_are_local_to_the_requested_namespace() {
    let namespace = initialize(SEED);
    let other = initialize([11; 32]);
    let output = dispatch(
        PROGRAM,
        SEED,
        vec![namespace, actor(1, true), actor(5, true)],
        Instruction::TransferAdmin {
            candidate: admin_authority::AdminCandidate::Signer,
        },
    )
    .unwrap();
    assert_eq!(output.post_states.len(), 3);
    assert!(
        dispatch(
            PROGRAM,
            [11; 32],
            vec![other.clone(), actor(1, true)],
            Instruction::CheckAdmin
        )
        .is_ok()
    );
    assert!(
        dispatch(
            PROGRAM,
            [11; 32],
            vec![other, actor(5, true)],
            Instruction::CheckAdmin
        )
        .is_err()
    );
}

#[test]
fn namespace_initialization_establishes_admin_without_a_second_claim_step() {
    let namespace = initialize(SEED);
    let output = dispatch(
        PROGRAM,
        SEED,
        vec![namespace.clone(), actor(1, true)],
        Instruction::CheckAdmin,
    )
    .unwrap();
    assert_eq!(output.post_states[0].account(), &namespace.account);
}
