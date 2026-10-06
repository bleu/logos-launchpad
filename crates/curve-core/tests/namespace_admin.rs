use curve_core::{Instruction, compute_config_pda, process_instruction};
use lee_core::{
    account::{Account, AccountId, AccountWithMetadata},
    program::ProgramId,
};

const PROGRAM: ProgramId = [7; 8];
fn account(id: AccountId, signed: bool) -> AccountWithMetadata {
    AccountWithMetadata {
        account: Account::default(),
        account_id: id,
        is_authorized: signed,
    }
}

#[test]
#[should_panic(expected = "UpdateConfig is retired")]
fn retired_whole_config_update_rejects_explicitly() {
    let namespace = AccountId::new([1; 32]);
    let _ = process_instruction(
        vec![
            account(compute_config_pda(namespace, PROGRAM), false),
            account(namespace, true),
        ],
        Instruction::UpdateConfig {
            namespace,
            admin: namespace,
            protocol_fee_bps: 30,
            treasury: namespace,
        },
        PROGRAM,
    );
}

#[test]
fn initialization_installs_a_separate_fixed_emergency_authority() {
    let namespace = AccountId::new([41; 32]);
    let admin = AccountId::new([42; 32]);
    let emergency_authority = AccountId::new([43; 32]);
    let mut proof = account(AccountId::new([44; 32]), true);
    proof.account.program_owner = curve_core::TOKEN_PROGRAM_ID;
    proof.account.data = (&token_core::TokenHolding::NftMaster {
        definition_id: namespace,
        print_balance: 1,
    })
        .into();
    let (posts, calls) = process_instruction(
        vec![
            account(compute_config_pda(namespace, PROGRAM), false),
            proof,
            account(admin, true),
            account(emergency_authority, true),
        ],
        Instruction::InitializeNamespace {
            namespace,
            admin,
            emergency_authority,
            emergency_candidate: curve_core::EmergencyCandidate::Signer,
            protocol_fee_bps: 30,
            treasury: namespace,
        },
        PROGRAM,
    );
    let config = curve_core::Config::try_from(&posts[0].account().data).unwrap();
    assert_eq!(config.current_admin(), Some(admin));
    assert_eq!(config.emergency_authority(), Some(emergency_authority));
    assert!(!config.is_frozen());
    assert!(calls.is_empty());
    assert_eq!(posts.len(), 4);
}

#[test]
fn initialization_requires_namespace_proof_and_installs_accepted_admin() {
    let namespace = AccountId::new([11; 32]);
    let admin = AccountId::new([12; 32]);
    let treasury = AccountId::new([13; 32]);
    let mut proof = account(AccountId::new([14; 32]), true);
    proof.account.program_owner = curve_core::TOKEN_PROGRAM_ID;
    proof.account.data = (&token_core::TokenHolding::NftMaster {
        definition_id: namespace,
        print_balance: 1,
    })
        .into();
    let (posts, calls) = process_instruction(
        vec![
            account(compute_config_pda(namespace, PROGRAM), false),
            proof,
            account(admin, true),
        ],
        Instruction::InitializeNamespace {
            namespace,
            admin,
            emergency_authority: admin,
            emergency_candidate: curve_core::EmergencyCandidate::Signer,
            protocol_fee_bps: 30,
            treasury,
        },
        PROGRAM,
    );
    assert!(calls.is_empty());
    let config = curve_core::Config::try_from(&posts[0].account().data).unwrap();
    assert_eq!(config.current_admin(), Some(admin));
    assert_eq!(config.protocol_fee_bps, 30);
    assert_eq!(config.treasury, treasury);
}

fn initialized() -> (AccountId, AccountWithMetadata, AccountWithMetadata) {
    let namespace = AccountId::new([11; 32]);
    let admin = account(AccountId::new([12; 32]), true);
    let mut proof = account(AccountId::new([14; 32]), true);
    proof.account.program_owner = curve_core::TOKEN_PROGRAM_ID;
    proof.account.data = (&token_core::TokenHolding::NftMaster {
        definition_id: namespace,
        print_balance: 1,
    })
        .into();
    let (posts, _) = process_instruction(
        vec![
            account(compute_config_pda(namespace, PROGRAM), false),
            proof,
            admin.clone(),
        ],
        Instruction::InitializeNamespace {
            namespace,
            admin: admin.account_id,
            emergency_authority: admin.account_id,
            emergency_candidate: curve_core::EmergencyCandidate::Signer,
            protocol_fee_bps: 30,
            treasury: AccountId::new([13; 32]),
        },
        PROGRAM,
    );
    let mut config = account(compute_config_pda(namespace, PROGRAM), false);
    config.account = posts[0].account().clone();
    // Dispatcher tests supply the runtime-applied owner; signed guest tests verify the claim.
    config.account.program_owner = PROGRAM;
    (namespace, config, admin)
}

#[test]
fn fee_update_preserves_treasury_and_administration() {
    let (namespace, config, admin) = initialized();
    let (posts, _) = process_instruction(
        vec![config, admin.clone()],
        Instruction::UpdateFee {
            namespace,
            protocol_fee_bps: 75,
        },
        PROGRAM,
    );
    let state = curve_core::Config::try_from(&posts[0].account().data).unwrap();
    assert_eq!(state.protocol_fee_bps, 75);
    assert_eq!(state.treasury, AccountId::new([13; 32]));
    assert_eq!(state.current_admin(), Some(admin.account_id));
}

#[test]
fn treasury_update_preserves_fee_and_administration() {
    let (namespace, config, admin) = initialized();
    let treasury = AccountId::new([21; 32]);
    let (posts, _) = process_instruction(
        vec![config, admin.clone()],
        Instruction::UpdateTreasury {
            namespace,
            treasury,
        },
        PROGRAM,
    );
    let state = curve_core::Config::try_from(&posts[0].account().data).unwrap();
    assert_eq!(state.protocol_fee_bps, 30);
    assert_eq!(state.treasury, treasury);
    assert_eq!(state.current_admin(), Some(admin.account_id));
}

#[test]
fn renunciation_is_terminal_and_preserves_settings() {
    let (namespace, mut config, admin) = initialized();
    let (posts, _) = process_instruction(
        vec![config.clone(), admin.clone()],
        Instruction::RenounceAdmin { namespace },
        PROGRAM,
    );
    config.account = posts[0].account().clone();
    let state = curve_core::Config::try_from(&config.account.data).unwrap();
    assert_eq!(state.current_admin(), None);
    assert_eq!(state.protocol_fee_bps, 30);
    assert_eq!(state.treasury, AccountId::new([13; 32]));
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            vec![config, admin],
            Instruction::UpdateFee {
                namespace,
                protocol_fee_bps: 99
            },
            PROGRAM
        ))
        .is_err()
    );
}

#[test]
fn signer_transfer_requires_acceptance_and_removes_old_admin_rights() {
    let (namespace, mut config, admin) = initialized();
    let mut successor = account(AccountId::new([21; 32]), false);
    let instruction = Instruction::TransferAdmin {
        namespace,
        new_admin: successor.account_id,
        candidate: curve_core::AdminCandidate::Signer,
    };
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            vec![config.clone(), admin.clone(), successor.clone()],
            instruction.clone(),
            PROGRAM
        ))
        .is_err()
    );
    successor.is_authorized = true;
    let (posts, _) = process_instruction(
        vec![config.clone(), admin.clone(), successor.clone()],
        instruction,
        PROGRAM,
    );
    config.account = posts[0].account().clone();
    let state = curve_core::Config::try_from(&config.account.data).unwrap();
    assert_eq!(state.current_admin(), Some(successor.account_id));
    assert_eq!(state.protocol_fee_bps, 30);
    assert_eq!(state.treasury, AccountId::new([13; 32]));
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            vec![config.clone(), admin],
            Instruction::UpdateFee {
                namespace,
                protocol_fee_bps: 40
            },
            PROGRAM
        ))
        .is_err()
    );
    let _ = process_instruction(
        vec![config, successor],
        Instruction::UpdateFee {
            namespace,
            protocol_fee_bps: 40,
        },
        PROGRAM,
    );
}

#[test]
fn config_reader_rejects_legacy_malformed_and_unknown_formats() {
    let (_, config, _) = initialized();
    let original = config.account.data.to_vec();
    let mut bad_magic = original.clone();
    bad_magic[0] ^= 1;
    let mut bad_version = original.clone();
    bad_version[8] = 1;
    let mut trailing = original.clone();
    trailing.push(0);
    for bytes in [
        vec![],
        vec![0; 66],
        original[..10].to_vec(),
        bad_magic,
        bad_version,
        trailing,
    ] {
        let data: lee_core::account::Data = bytes.try_into().unwrap();
        assert!(curve_core::Config::try_from(&data).is_err());
    }
}

fn create_namespace_with_emergency(
    emergency: AccountWithMetadata,
    candidate: curve_core::EmergencyCandidate,
) -> (AccountId, AccountWithMetadata, AccountWithMetadata) {
    let namespace = AccountId::new([11; 32]);
    let admin = account(AccountId::new([12; 32]), true);
    let mut proof = account(AccountId::new([14; 32]), true);
    proof.account.program_owner = curve_core::TOKEN_PROGRAM_ID;
    proof.account.data = (&token_core::TokenHolding::NftMaster {
        definition_id: namespace,
        print_balance: 1,
    })
        .into();
    let mut pre = vec![
        account(compute_config_pda(namespace, PROGRAM), false),
        proof,
        admin.clone(),
    ];
    if !pre.iter().any(|a| a.account_id == emergency.account_id) {
        pre.push(emergency.clone());
    }
    let (posts, _) = process_instruction(
        pre,
        Instruction::InitializeNamespace {
            namespace,
            admin: admin.account_id,
            emergency_authority: emergency.account_id,
            emergency_candidate: candidate,
            protocol_fee_bps: 30,
            treasury: AccountId::new([13; 32]),
        },
        PROGRAM,
    );
    let mut config = account(compute_config_pda(namespace, PROGRAM), false);
    config.account = posts[0].account().clone();
    config.account.program_owner = PROGRAM;
    (namespace, config, admin)
}

#[test]
fn emergency_authority_can_freeze_namespace() {
    let emergency = account(AccountId::new([51; 32]), true);
    let (namespace, config, admin) =
        create_namespace_with_emergency(emergency.clone(), curve_core::EmergencyCandidate::Signer);
    let (posts, calls) = process_instruction(
        vec![config, emergency.clone()],
        Instruction::FreezeNamespace { namespace },
        PROGRAM,
    );
    let state = curve_core::Config::try_from(&posts[0].account().data).unwrap();
    assert!(state.is_frozen());
    assert_eq!(state.emergency_authority(), Some(emergency.account_id));
    assert_eq!(state.current_admin(), Some(admin.account_id));
    assert_eq!(state.protocol_fee_bps, 30);
    assert_eq!(state.treasury, AccountId::new([13; 32]));
    assert_eq!(posts[1].account(), &emergency.account);
    assert!(calls.is_empty());
}

#[test]
fn emergency_authority_can_unfreeze_namespace() {
    let emergency = account(AccountId::new([51; 32]), true);
    let (namespace, mut config, _) =
        create_namespace_with_emergency(emergency.clone(), curve_core::EmergencyCandidate::Signer);
    let initial = config.account.clone();
    let (posts, _) = process_instruction(
        vec![config.clone(), emergency.clone()],
        Instruction::FreezeNamespace { namespace },
        PROGRAM,
    );
    config.account = posts[0].account().clone();
    let (posts, calls) = process_instruction(
        vec![config, emergency],
        Instruction::UnfreezeNamespace { namespace },
        PROGRAM,
    );
    assert_eq!(posts[0].account(), &initial);
    assert!(calls.is_empty());
}

#[test]
fn repeated_emergency_calls_still_require_the_fixed_authority() {
    let emergency = account(AccountId::new([51; 32]), true);
    let (namespace, mut config, admin) =
        create_namespace_with_emergency(emergency.clone(), curve_core::EmergencyCandidate::Signer);
    for instruction in [
        Instruction::UnfreezeNamespace { namespace },
        Instruction::FreezeNamespace { namespace },
        Instruction::UnfreezeNamespace { namespace },
    ] {
        for _ in 0..2 {
            let mut unsigned = emergency.clone();
            unsigned.is_authorized = false;
            for caller in [admin.clone(), unsigned] {
                assert!(
                    std::panic::catch_unwind(|| process_instruction(
                        vec![config.clone(), caller],
                        instruction.clone(),
                        PROGRAM,
                    ))
                    .is_err()
                );
            }
            let (posts, calls) = process_instruction(
                vec![config.clone(), emergency.clone()],
                instruction.clone(),
                PROGRAM,
            );
            config.account = posts[0].account().clone();
            let state = curve_core::Config::try_from(&config.account.data).unwrap();
            assert_eq!(
                state.is_frozen(),
                matches!(instruction, Instruction::FreezeNamespace { .. })
            );
            assert!(calls.is_empty());
        }
    }
}

#[test]
fn admin_lifecycle_preserves_fixed_emergency_control_including_shared_roles() {
    for emergency_id in [AccountId::new([12; 32]), AccountId::new([51; 32])] {
        let emergency = account(emergency_id, true);
        let (namespace, mut config, admin) = create_namespace_with_emergency(
            emergency.clone(),
            curve_core::EmergencyCandidate::Signer,
        );
        let (posts, _) = process_instruction(
            vec![config.clone(), emergency.clone()],
            Instruction::FreezeNamespace { namespace },
            PROGRAM,
        );
        config.account = posts[0].account().clone();
        let successor = account(AccountId::new([21; 32]), true);
        for (instruction, authorities) in [
            (
                Instruction::UpdateFee {
                    namespace,
                    protocol_fee_bps: 75,
                },
                vec![admin.clone()],
            ),
            (
                Instruction::UpdateTreasury {
                    namespace,
                    treasury: successor.account_id,
                },
                vec![admin.clone()],
            ),
            (
                Instruction::TransferAdmin {
                    namespace,
                    new_admin: successor.account_id,
                    candidate: curve_core::AdminCandidate::Signer,
                },
                vec![admin.clone(), successor.clone()],
            ),
            (
                Instruction::RenounceAdmin { namespace },
                vec![successor.clone()],
            ),
        ] {
            let mut pre = vec![config.clone()];
            pre.extend(authorities);
            let (posts, _) = process_instruction(pre, instruction, PROGRAM);
            config.account = posts[0].account().clone();
            let state = curve_core::Config::try_from(&config.account.data).unwrap();
            assert_eq!(state.emergency_authority(), Some(emergency_id));
            assert!(state.is_frozen());
        }
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                vec![config.clone(), successor],
                Instruction::UnfreezeNamespace { namespace },
                PROGRAM,
            ))
            .is_err()
        );
        let (posts, _) = process_instruction(
            vec![config, emergency],
            Instruction::UnfreezeNamespace { namespace },
            PROGRAM,
        );
        let state = curve_core::Config::try_from(&posts[0].account().data).unwrap();
        assert_eq!(state.current_admin(), None);
        assert_eq!(state.emergency_authority(), Some(emergency_id));
        assert!(!state.is_frozen());
    }
}

#[test]
#[should_panic(expected = "PDA emergency authority belongs to another program")]
fn initialization_rejects_emergency_pda_owned_by_another_program() {
    let owner = [42; 8];
    let seed = [31; 32];
    let mut emergency = account(
        AccountId::for_public_pda(&owner, &lee_core::program::PdaSeed::new(seed)),
        false,
    );
    emergency.account.program_owner = [99; 8];
    create_namespace_with_emergency(
        emergency,
        curve_core::EmergencyCandidate::Pda {
            program_id: owner,
            seed,
        },
    );
}

#[test]
fn pda_successor_must_belong_to_its_candidate_program() {
    let (namespace, config, admin) = initialized();
    let owner = [42; 8];
    let seed = [31; 32];
    let mut successor = account(
        AccountId::for_public_pda(&owner, &lee_core::program::PdaSeed::new(seed)),
        false,
    );
    successor.account.program_owner = [99; 8];
    let instruction = Instruction::TransferAdmin {
        namespace,
        new_admin: successor.account_id,
        candidate: curve_core::AdminCandidate::Pda {
            program_id: owner,
            seed,
        },
    };
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            vec![config.clone(), admin.clone(), successor.clone()],
            instruction.clone(),
            PROGRAM
        ))
        .is_err()
    );
    successor.account.program_owner = owner;
    let (posts, _) =
        process_instruction(vec![config, admin, successor.clone()], instruction, PROGRAM);
    assert_eq!(
        curve_core::Config::try_from(&posts[0].account().data)
            .unwrap()
            .current_admin(),
        Some(successor.account_id)
    );
}
