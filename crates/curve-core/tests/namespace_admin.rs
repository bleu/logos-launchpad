use curve_core::{
    Config, INITIAL_EMERGENCY_AUTHORITY, Instruction, compute_config_pda, process_instruction,
};
use lee_core::{
    account::{Account, AccountId, AccountWithMetadata},
    program::ProgramId,
};

const PROGRAM: ProgramId = [42; 8];

fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}

fn signer(account_id: AccountId) -> AccountWithMetadata {
    AccountWithMetadata::new(Account::default(), true, account_id)
}

fn initialize(namespace: AccountId) -> AccountWithMetadata {
    let (posts, _) = process_instruction(
        vec![
            AccountWithMetadata::new(
                Account::default(),
                false,
                compute_config_pda(namespace, PROGRAM),
            ),
            signer(namespace),
        ],
        Instruction::InitializeNamespace {
            namespace,
            protocol_fee_bps: 500,
            treasury: id(6),
        },
        PROGRAM,
    );
    let mut account = posts[0].account().clone();
    account.program_owner = PROGRAM;
    AccountWithMetadata::new(account, false, compute_config_pda(namespace, PROGRAM))
}

#[test]
fn namespace_signer_creates_its_config_without_an_authority_nft() {
    let namespace = id(1);
    let creator = signer(namespace);
    let (posts, calls) = process_instruction(
        vec![
            AccountWithMetadata::new(
                Account::default(),
                false,
                compute_config_pda(namespace, PROGRAM),
            ),
            creator.clone(),
        ],
        Instruction::InitializeNamespace {
            namespace,
            protocol_fee_bps: 500,
            treasury: id(6),
        },
        PROGRAM,
    );
    let state = Config::try_from(&posts[0].account().data).unwrap();
    assert_eq!(state.current_admin(), Some(namespace));
    assert_eq!(state.protocol_fee_bps, 500);
    assert_eq!(state.treasury, id(6));
    assert_eq!(state.emergency.authority(), INITIAL_EMERGENCY_AUTHORITY);
    assert!(!state.emergency.is_frozen);
    assert_eq!(posts[1].account(), &creator.account);
    assert!(calls.is_empty());
}

fn control(
    config: AccountWithMetadata,
    caller: AccountWithMetadata,
    instruction: Instruction,
) -> AccountWithMetadata {
    let (posts, calls) =
        process_instruction(vec![config.clone(), caller.clone()], instruction, PROGRAM);
    assert!(calls.is_empty());
    assert_eq!(posts[1].account(), &caller.account);
    AccountWithMetadata::new(posts[0].account().clone(), false, config.account_id)
}

#[test]
fn administrator_changes_only_the_fee_while_frozen() {
    let namespace = id(1);
    let config = control(
        initialize(namespace),
        signer(INITIAL_EMERGENCY_AUTHORITY),
        Instruction::SetFrozen {
            namespace,
            frozen: true,
        },
    );
    let config = control(
        config,
        signer(namespace),
        Instruction::UpdateFee {
            namespace,
            protocol_fee_bps: 123,
        },
    );
    let state = Config::try_from(&config.account.data).unwrap();
    assert_eq!(state.protocol_fee_bps, 123);
    assert_eq!(state.treasury, id(6));
    assert_eq!(state.current_admin(), Some(namespace));
    assert_eq!(state.emergency.authority(), INITIAL_EMERGENCY_AUTHORITY);
    assert!(state.emergency.is_frozen);
}

#[test]
fn administrator_changes_only_the_treasury_while_frozen() {
    let namespace = id(1);
    let config = control(
        initialize(namespace),
        signer(INITIAL_EMERGENCY_AUTHORITY),
        Instruction::SetFrozen {
            namespace,
            frozen: true,
        },
    );
    let config = control(
        config,
        signer(namespace),
        Instruction::UpdateTreasury {
            namespace,
            treasury: id(7),
        },
    );
    let state = Config::try_from(&config.account.data).unwrap();
    assert_eq!(state.treasury, id(7));
    assert_eq!(state.protocol_fee_bps, 500);
    assert_eq!(state.current_admin(), Some(namespace));
    assert_eq!(state.emergency.authority(), INITIAL_EMERGENCY_AUTHORITY);
    assert!(state.emergency.is_frozen);
}

#[test]
fn cosigned_transfer_moves_admin_power_without_changing_settings() {
    let namespace = id(1);
    let config = control(
        initialize(namespace),
        signer(INITIAL_EMERGENCY_AUTHORITY),
        Instruction::SetFrozen {
            namespace,
            frozen: true,
        },
    );
    let (posts, calls) = process_instruction(
        vec![config.clone(), signer(namespace), signer(id(2))],
        Instruction::TransferAdmin {
            namespace,
            new_admin: id(2),
            candidate: curve_core::AdminCandidate::Signer,
        },
        PROGRAM,
    );
    assert!(calls.is_empty());
    let config = AccountWithMetadata::new(posts[0].account().clone(), false, config.account_id);
    let state = Config::try_from(&config.account.data).unwrap();
    assert_eq!(state.current_admin(), Some(id(2)));
    assert_eq!(state.protocol_fee_bps, 500);
    assert_eq!(state.treasury, id(6));
    assert_eq!(state.emergency.authority(), INITIAL_EMERGENCY_AUTHORITY);
    assert!(state.emergency.is_frozen);
    assert!(
        std::panic::catch_unwind(|| {
            process_instruction(
                vec![config.clone(), signer(namespace)],
                Instruction::UpdateFee {
                    namespace,
                    protocol_fee_bps: 99,
                },
                PROGRAM,
            )
        })
        .is_err()
    );
    let updated = control(
        config,
        signer(id(2)),
        Instruction::UpdateFee {
            namespace,
            protocol_fee_bps: 99,
        },
    );
    assert_eq!(
        Config::try_from(&updated.account.data)
            .unwrap()
            .protocol_fee_bps,
        99
    );
}

#[test]
fn renunciation_is_terminal_and_keeps_fees_treasury_and_emergency_control() {
    let namespace = id(1);
    let config = control(
        initialize(namespace),
        signer(INITIAL_EMERGENCY_AUTHORITY),
        Instruction::SetFrozen {
            namespace,
            frozen: true,
        },
    );
    let config = control(
        config,
        signer(namespace),
        Instruction::RenounceAdmin { namespace },
    );
    let state = Config::try_from(&config.account.data).unwrap();
    assert_eq!(state.current_admin(), None);
    assert_eq!(state.protocol_fee_bps, 500);
    assert_eq!(state.treasury, id(6));
    assert_eq!(state.emergency.authority(), INITIAL_EMERGENCY_AUTHORITY);
    assert!(state.emergency.is_frozen);
    for instruction in [
        Instruction::UpdateFee {
            namespace,
            protocol_fee_bps: 0,
        },
        Instruction::UpdateTreasury {
            namespace,
            treasury: id(7),
        },
        Instruction::RenounceAdmin { namespace },
        Instruction::TransferAdmin {
            namespace,
            new_admin: id(2),
            candidate: curve_core::AdminCandidate::Signer,
        },
    ] {
        let mut accounts = vec![config.clone(), signer(namespace)];
        if matches!(instruction, Instruction::TransferAdmin { .. }) {
            accounts.push(signer(id(2)));
        }
        assert!(
            std::panic::catch_unwind(|| process_instruction(accounts, instruction, PROGRAM))
                .is_err()
        );
    }
    let unfrozen = control(
        config,
        signer(INITIAL_EMERGENCY_AUTHORITY),
        Instruction::SetFrozen {
            namespace,
            frozen: false,
        },
    );
    assert!(
        !Config::try_from(&unfrozen.account.data)
            .unwrap()
            .emergency
            .is_frozen
    );
}

#[test]
fn only_the_namespace_account_can_authorize_creation() {
    let namespace = id(1);
    let mut unsigned = signer(namespace);
    unsigned.is_authorized = false;
    let nft_holder = AccountWithMetadata::new(
        Account {
            program_owner: curve_core::TOKEN_PROGRAM_ID,
            data: lee_core::account::Data::from(&token_core::TokenHolding::NftMaster {
                definition_id: namespace,
                print_balance: 1,
            }),
            ..Account::default()
        },
        true,
        id(20),
    );
    for creator in [unsigned, signer(id(2)), nft_holder] {
        let accounts = vec![
            AccountWithMetadata::new(
                Account::default(),
                false,
                compute_config_pda(namespace, PROGRAM),
            ),
            creator,
        ];
        assert!(
            std::panic::catch_unwind(|| {
                process_instruction(
                    accounts,
                    Instruction::InitializeNamespace {
                        namespace,
                        protocol_fee_bps: 500,
                        treasury: id(6),
                    },
                    PROGRAM,
                )
            })
            .is_err()
        );
    }
}

#[test]
fn namespace_creation_cannot_replace_existing_or_renounced_administration() {
    let namespace = id(1);
    let existing = initialize(namespace);
    let renounced = control(
        existing.clone(),
        signer(namespace),
        Instruction::RenounceAdmin { namespace },
    );
    for config in [existing, renounced] {
        assert!(
            std::panic::catch_unwind(|| {
                process_instruction(
                    vec![config, signer(namespace)],
                    Instruction::InitializeNamespace {
                        namespace,
                        protocol_fee_bps: 0,
                        treasury: id(7),
                    },
                    PROGRAM,
                )
            })
            .is_err()
        );
    }
}

#[test]
fn every_admin_action_requires_the_stored_role_signature_even_for_no_op_updates() {
    let namespace = id(1);
    let config = initialize(namespace);
    let mut unsigned = signer(namespace);
    unsigned.is_authorized = false;
    let nft_holder = AccountWithMetadata::new(
        Account {
            program_owner: curve_core::TOKEN_PROGRAM_ID,
            data: lee_core::account::Data::from(&token_core::TokenHolding::NftMaster {
                definition_id: namespace,
                print_balance: 1,
            }),
            ..Account::default()
        },
        true,
        id(20),
    );
    for caller in [
        unsigned,
        signer(id(2)),
        nft_holder,
        signer(INITIAL_EMERGENCY_AUTHORITY),
    ] {
        for instruction in [
            Instruction::UpdateFee {
                namespace,
                protocol_fee_bps: 500,
            },
            Instruction::UpdateTreasury {
                namespace,
                treasury: id(6),
            },
            Instruction::RenounceAdmin { namespace },
            Instruction::TransferAdmin {
                namespace,
                new_admin: id(3),
                candidate: curve_core::AdminCandidate::Signer,
            },
        ] {
            let mut accounts = vec![config.clone(), caller.clone()];
            if matches!(instruction, Instruction::TransferAdmin { .. }) {
                accounts.push(signer(id(3)));
            }
            assert!(
                std::panic::catch_unwind(|| process_instruction(accounts, instruction, PROGRAM))
                    .is_err()
            );
        }
    }
}

fn assert_rejected(accounts: Vec<AccountWithMetadata>, instruction: Instruction, expected: &str) {
    let error = std::panic::catch_unwind(|| process_instruction(accounts, instruction, PROGRAM))
        .expect_err("instruction must reject");
    let message = error
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| error.downcast_ref::<&str>().copied())
        .expect("panic message");
    assert!(message.contains(expected), "{message}");
}

fn admin_instructions(namespace: AccountId) -> [Instruction; 4] {
    [
        Instruction::UpdateFee {
            namespace,
            protocol_fee_bps: 123,
        },
        Instruction::UpdateTreasury {
            namespace,
            treasury: id(7),
        },
        Instruction::RenounceAdmin { namespace },
        Instruction::TransferAdmin {
            namespace,
            new_admin: id(3),
            candidate: curve_core::AdminCandidate::Signer,
        },
    ]
}

#[test]
fn namespace_admin_cannot_change_another_namespace() {
    let other = id(2);
    let config = initialize(other);
    for instruction in admin_instructions(other) {
        let mut accounts = vec![config.clone(), signer(id(1))];
        if matches!(instruction, Instruction::TransferAdmin { .. }) {
            accounts.push(signer(id(3)));
        }
        assert_rejected(accounts, instruction, "NotAdmin");
    }
}

#[test]
fn namespace_admin_actions_reject_wrong_config_address_or_owner() {
    let namespace = id(1);
    let config = initialize(namespace);
    let foreign = initialize(id(2));
    let mut forged = config.clone();
    forged.account_id = id(99);
    let mut wrong_owner = config;
    wrong_owner.account.program_owner = [43; 8];
    for (invalid, expected) in [
        (foreign, "Config account ID does not match PDA"),
        (forged, "Config account ID does not match PDA"),
        (wrong_owner, "Config owner does not match program"),
    ] {
        for instruction in admin_instructions(namespace) {
            let mut accounts = vec![invalid.clone(), signer(namespace)];
            if matches!(instruction, Instruction::TransferAdmin { .. }) {
                accounts.push(signer(id(3)));
            }
            assert_rejected(accounts, instruction, expected);
        }
    }
}

#[test]
fn namespace_admin_actions_reject_malformed_and_legacy_config_state() {
    let namespace = id(1);
    let config = initialize(namespace);
    let mut truncated = config.account.data.to_vec();
    truncated.pop();
    let mut legacy = namespace.to_bytes().to_vec();
    legacy.extend_from_slice(&500_u16.to_le_bytes());
    legacy.extend_from_slice(&id(6).to_bytes());
    let legacy_without_emergency = legacy.clone();
    legacy.extend_from_slice(&INITIAL_EMERGENCY_AUTHORITY.to_bytes());
    legacy.push(0);
    for data in [vec![], vec![1], truncated, legacy_without_emergency, legacy] {
        let mut invalid = config.clone();
        invalid.account.data = data.try_into().unwrap();
        for instruction in admin_instructions(namespace) {
            let mut accounts = vec![invalid.clone(), signer(namespace)];
            if matches!(instruction, Instruction::TransferAdmin { .. }) {
                accounts.push(signer(id(3)));
            }
            assert_rejected(accounts, instruction, "Config account holds invalid data");
        }
    }
}

#[test]
fn namespace_creation_requires_a_non_default_identity_and_empty_config_pda() {
    let namespace = id(1);
    let empty = AccountWithMetadata::new(
        Account::default(),
        false,
        compute_config_pda(namespace, PROGRAM),
    );
    let mut forged = empty.clone();
    forged.account_id = id(99);
    let mut foreign = empty.clone();
    foreign.account_id = compute_config_pda(id(2), PROGRAM);
    let mut wrong_owner = empty.clone();
    wrong_owner.account.program_owner = [43; 8];
    let mut funded = empty.clone();
    funded.account.balance = 1;
    let mut nonempty = empty;
    nonempty.account.data = vec![1].try_into().unwrap();
    for (namespace, config, expected) in [
        (namespace, forged, "Config account ID does not match PDA"),
        (namespace, foreign, "Config account ID does not match PDA"),
        (namespace, wrong_owner, "Namespace config already exists"),
        (namespace, funded, "Namespace config already exists"),
        (namespace, nonempty, "Namespace config already exists"),
        (
            AccountId::default(),
            AccountWithMetadata::new(
                Account::default(),
                false,
                compute_config_pda(AccountId::default(), PROGRAM),
            ),
            "Namespace must not be the default key",
        ),
    ] {
        assert_rejected(
            vec![config, signer(namespace)],
            Instruction::InitializeNamespace {
                namespace,
                protocol_fee_bps: 500,
                treasury: id(6),
            },
            expected,
        );
    }
}

#[test]
fn namespace_creation_and_treasury_update_reject_the_default_treasury() {
    let namespace = id(1);
    assert_rejected(
        vec![
            AccountWithMetadata::new(
                Account::default(),
                false,
                compute_config_pda(namespace, PROGRAM),
            ),
            signer(namespace),
        ],
        Instruction::InitializeNamespace {
            namespace,
            protocol_fee_bps: 500,
            treasury: AccountId::default(),
        },
        "Treasury must not be the default key",
    );
    assert_rejected(
        vec![initialize(namespace), signer(namespace)],
        Instruction::UpdateTreasury {
            namespace,
            treasury: AccountId::default(),
        },
        "Treasury must not be the default key",
    );
}

#[test]
fn admin_transfer_requires_the_matching_non_default_successor_signature() {
    let namespace = id(1);
    let config = initialize(namespace);
    let mut unsigned = signer(id(3));
    unsigned.is_authorized = false;
    for (new_admin, successor, expected) in [
        (id(3), None, "TransferAdmin requires exactly three accounts"),
        (
            id(3),
            Some(unsigned),
            "Namespace administration transfer failed: InvalidCandidate",
        ),
        (
            AccountId::default(),
            Some(signer(AccountId::default())),
            "Namespace administration transfer failed: InvalidCandidate",
        ),
        (
            id(3),
            Some(signer(id(2))),
            "Successor account ID does not match new admin",
        ),
    ] {
        let mut accounts = vec![config.clone(), signer(namespace)];
        if let Some(successor) = successor {
            accounts.push(successor);
        }
        assert_rejected(
            accounts,
            Instruction::TransferAdmin {
                namespace,
                new_admin,
                candidate: curve_core::AdminCandidate::Signer,
            },
            expected,
        );
    }
}

fn deployed_pda(program_id: ProgramId, seed: [u8; 32]) -> AccountWithMetadata {
    AccountWithMetadata::new(
        Account {
            program_owner: program_id,
            ..Account::default()
        },
        false,
        AccountId::for_public_pda(&program_id, &lee_core::program::PdaSeed::new(seed)),
    )
}

#[test]
fn admin_transfer_accepts_a_deployed_pda_without_successor_signature() {
    let namespace = id(1);
    let program_id = [8; 8];
    let seed = [9; 32];
    let successor = deployed_pda(program_id, seed);
    let accounts = vec![initialize(namespace), signer(namespace), successor.clone()];
    let (posts, calls) = process_instruction(
        accounts.clone(),
        Instruction::TransferAdmin {
            namespace,
            new_admin: successor.account_id,
            candidate: curve_core::AdminCandidate::Pda { program_id, seed },
        },
        PROGRAM,
    );
    lee_core::program::validate_execution(&accounts, &posts, PROGRAM).unwrap();
    assert!(calls.is_empty());
    assert_eq!(posts[1].account(), &accounts[1].account);
    assert_eq!(posts[2].account(), &successor.account);
    let config =
        AccountWithMetadata::new(posts[0].account().clone(), false, accounts[0].account_id);
    let state = Config::try_from(&config.account.data).unwrap();
    assert_eq!(state.current_admin(), Some(successor.account_id));
    assert_eq!(state.protocol_fee_bps, 500);
    assert_eq!(state.treasury, id(6));
    assert_eq!(state.emergency.authority(), INITIAL_EMERGENCY_AUTHORITY);
    assert!(!state.emergency.is_frozen);
    assert_rejected(
        vec![config.clone(), successor.clone()],
        Instruction::UpdateFee {
            namespace,
            protocol_fee_bps: 123,
        },
        "Namespace administrator authorization failed: MissingSignature",
    );
    let mut authorized = successor;
    authorized.is_authorized = true;
    let updated = control(
        config,
        authorized,
        Instruction::UpdateFee {
            namespace,
            protocol_fee_bps: 123,
        },
    );
    assert_eq!(
        Config::try_from(&updated.account.data)
            .unwrap()
            .protocol_fee_bps,
        123
    );
}

#[test]
fn admin_transfer_rejects_pda_candidate_derivation_mismatch() {
    let namespace = id(1);
    let program_id = [8; 8];
    let seed = [9; 32];
    let successor = deployed_pda(program_id, seed);
    let mut wrong_address = successor.clone();
    wrong_address.account_id = id(99);
    let mut wrong_program = successor.clone();
    wrong_program.account.program_owner = [9; 8];
    for (successor, program_id, seed) in [
        (wrong_address, program_id, seed),
        (successor, program_id, [10; 32]),
        (wrong_program, [9; 8], seed),
    ] {
        assert_rejected(
            vec![initialize(namespace), signer(namespace), successor.clone()],
            Instruction::TransferAdmin {
                namespace,
                new_admin: successor.account_id,
                candidate: curve_core::AdminCandidate::Pda { program_id, seed },
            },
            "Namespace administration transfer failed: CandidateMismatch",
        );
    }
}

#[test]
fn admin_transfer_rejects_unclaimed_and_wrong_owner_pda_candidates() {
    let namespace = id(1);
    let program_id = [8; 8];
    let seed = [9; 32];
    let mut wrong_owner = deployed_pda(program_id, seed);
    wrong_owner.account.program_owner = [9; 8];
    let mut unclaimed = deployed_pda(program_id, seed);
    unclaimed.account = Account::default();
    let mut funded = unclaimed.clone();
    funded.account.balance = 1;
    for successor in [wrong_owner, unclaimed, funded] {
        assert_rejected(
            vec![initialize(namespace), signer(namespace), successor.clone()],
            Instruction::TransferAdmin {
                namespace,
                new_admin: successor.account_id,
                candidate: curve_core::AdminCandidate::Pda { program_id, seed },
            },
            "PDA administrator belongs to another program",
        );
    }
    for balance in [0, 1] {
        let program_id = ProgramId::default();
        let mut successor = deployed_pda(program_id, seed);
        successor.account.balance = balance;
        assert_rejected(
            vec![initialize(namespace), signer(namespace), successor.clone()],
            Instruction::TransferAdmin {
                namespace,
                new_admin: successor.account_id,
                candidate: curve_core::AdminCandidate::Pda { program_id, seed },
            },
            "Namespace administration transfer failed: UndeployedPda",
        );
    }
}

#[test]
fn namespace_controls_reject_an_unknown_config_version() {
    let namespace = id(1);
    let mut config = initialize(namespace);
    let mut data = config.account.data.to_vec();
    data[8] = 99;
    config.account.data = data.try_into().unwrap();
    let result = std::panic::catch_unwind(|| {
        process_instruction(
            vec![config, signer(INITIAL_EMERGENCY_AUTHORITY)],
            Instruction::SetFrozen {
                namespace,
                frozen: true,
            },
            PROGRAM,
        )
    });
    assert!(result.is_err(), "unsupported namespace state must reject");
}
