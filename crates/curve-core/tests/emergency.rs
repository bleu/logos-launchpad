use associated_token_account_core::{compute_ata_seed, get_associated_token_account_id};
use curve_core::{
    ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID, AdminCandidate, Config, Instruction, PoolAccount,
    TOKEN_PROGRAM_ID, compute_config_pda, compute_pool_pda, process_instruction,
};
use lee_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::ProgramId,
};
use pool::{Pool, PoolCreateParams};
use token_core::TokenHolding;

const PROGRAM: ProgramId = [42; 8];

fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}

fn emergency_authority() -> AccountId {
    AccountId::from(&lee::PublicKey::new_from_private_key(
        &lee::PrivateKey::try_new([101; 32]).unwrap(),
    ))
}

fn administrator(namespace: AccountId) -> AccountWithMetadata {
    signer(namespace)
}

fn initialize(namespace: AccountId) -> AccountWithMetadata {
    let (posts, calls) = process_instruction(
        vec![
            AccountWithMetadata::new(
                Account::default(),
                false,
                compute_config_pda(namespace, PROGRAM),
            ),
            administrator(namespace),
        ],
        Instruction::InitializeNamespace {
            namespace,
            protocol_fee_bps: 500,
            treasury: id(6),
        },
        PROGRAM,
    );
    assert!(calls.is_empty());
    AccountWithMetadata::new(
        Account {
            program_owner: PROGRAM,
            ..posts[0].account().clone()
        },
        false,
        compute_config_pda(namespace, PROGRAM),
    )
}

fn signer(authority: AccountId) -> AccountWithMetadata {
    AccountWithMetadata::new(Account::default(), true, authority)
}

fn control(
    config: AccountWithMetadata,
    authority: AccountWithMetadata,
    instruction: Instruction,
) -> AccountWithMetadata {
    let (posts, calls) = process_instruction(
        vec![config.clone(), authority.clone()],
        instruction,
        PROGRAM,
    );
    assert!(calls.is_empty());
    assert_eq!(posts[1].account(), &authority.account);
    AccountWithMetadata::new(posts[0].account().clone(), false, config.account_id)
}

fn transfer_admin(
    namespace: AccountId,
    config: AccountWithMetadata,
    current: AccountWithMetadata,
    successor: AccountId,
) -> AccountWithMetadata {
    let successor = signer(successor);
    let (posts, calls) = process_instruction(
        vec![config.clone(), current.clone(), successor.clone()],
        Instruction::TransferAdmin {
            namespace,
            new_admin: successor.account_id,
            candidate: AdminCandidate::Signer,
        },
        PROGRAM,
    );
    assert!(calls.is_empty());
    assert_eq!(posts[1].account(), &current.account);
    assert_eq!(posts[2].account(), &successor.account);
    AccountWithMetadata::new(posts[0].account().clone(), false, config.account_id)
}

fn rejection(accounts: Vec<AccountWithMetadata>, instruction: Instruction) -> String {
    let error = std::panic::catch_unwind(|| process_instruction(accounts, instruction, PROGRAM))
        .expect_err("instruction must reject");
    error
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| error.downcast_ref::<&str>().map(|text| text.to_string()))
        .expect("panic message")
}

fn holding(owner: AccountId, definition: AccountId, balance: u128) -> AccountWithMetadata {
    AccountWithMetadata::new(
        Account {
            program_owner: TOKEN_PROGRAM_ID,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: definition,
                balance,
            }),
            ..Account::default()
        },
        false,
        get_associated_token_account_id(
            &ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
            &compute_ata_seed(owner, definition),
        ),
    )
}

fn swap_accounts(
    namespace: AccountId,
    config: AccountWithMetadata,
    token_in: AccountId,
) -> Vec<AccountWithMetadata> {
    let owner = id(3);
    let participant = id(99);
    let pool_id = compute_pool_pda(namespace, PROGRAM, id(4), id(5), owner);
    let state = PoolAccount {
        allowed_input_token: None,
        funded: true,
        namespace,
        token0_definition_id: id(4),
        token1_definition_id: id(5),
        owner,
        owner_program: None,
        pool: Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 100,
            virtual_reserve0: 1_000,
            virtual_reserve1: 100,
            close_timestamp: Some(42),
            close_on_depletion: None,
        })
        .unwrap(),
    };
    let (token_out, input_reserve, output_reserve) = if token_in == id(5) {
        (id(4), 100, 800)
    } else {
        (id(5), 800, 100)
    };
    vec![
        AccountWithMetadata::new(
            Account {
                program_owner: PROGRAM,
                data: Data::from(&state),
                ..Account::default()
            },
            false,
            pool_id,
        ),
        config,
        signer(participant),
        holding(participant, token_in, 1_000),
        holding(pool_id, token_in, input_reserve),
        holding(pool_id, token_out, output_reserve),
        holding(participant, token_out, 0),
        AccountWithMetadata::new(
            Account {
                program_owner: [88; 8],
                data: Data::try_from(
                    clock_core::ClockAccountData {
                        block_id: 7,
                        timestamp: 1,
                    }
                    .to_bytes(),
                )
                .unwrap(),
                ..Account::default()
            },
            false,
            clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
        ),
    ]
}

#[test]
fn frozen_namespace_rejects_exact_input_buy() {
    let namespace = id(1);
    let config = control(
        initialize(namespace),
        signer(emergency_authority()),
        Instruction::SetFrozen {
            namespace,
            frozen: true,
        },
    );
    let error = rejection(
        swap_accounts(namespace, config, id(5)),
        Instruction::SwapExactInput {
            amount_in: 25,
            min_amount_out: 0,
            token_in: id(5),
        },
    );
    assert_eq!(error, "Namespace swaps are frozen");
}

#[test]
fn frozen_namespace_rejects_exact_output_buy() {
    let namespace = id(1);
    let config = control(
        initialize(namespace),
        signer(emergency_authority()),
        Instruction::SetFrozen {
            namespace,
            frozen: true,
        },
    );
    let error = rejection(
        swap_accounts(namespace, config, id(5)),
        Instruction::SwapExactOutput {
            amount_out: 200,
            max_amount_in: 30,
            token_in: id(5),
        },
    );
    assert_eq!(error, "Namespace swaps are frozen");
}

#[test]
fn frozen_namespace_rejects_sell_and_private_wrapped_swaps() {
    let namespace = id(1);
    let config = control(
        initialize(namespace),
        signer(emergency_authority()),
        Instruction::SetFrozen {
            namespace,
            frozen: true,
        },
    );
    for (token_in, instruction) in [
        (
            id(4),
            Instruction::SwapExactInput {
                amount_in: 250,
                min_amount_out: 0,
                token_in: id(4),
            },
        ),
        (
            id(5),
            Instruction::SwapExactInput {
                amount_in: 25,
                min_amount_out: 0,
                token_in: id(5),
            },
        ),
        (
            id(5),
            Instruction::SwapExactOutput {
                amount_out: 200,
                max_amount_in: 30,
                token_in: id(5),
            },
        ),
    ] {
        for private in [false, true] {
            let instruction = if private {
                Instruction::Private {
                    instruction: Box::new(instruction.clone()),
                }
            } else {
                instruction.clone()
            };
            assert_eq!(
                rejection(
                    swap_accounts(namespace, config.clone(), token_in),
                    instruction
                ),
                "Namespace swaps are frozen",
            );
        }
    }
}

#[test]
fn swap_checks_permission_and_identity_before_freeze_then_trade_rules() {
    let namespace = id(1);
    let config = control(
        initialize(namespace),
        signer(emergency_authority()),
        Instruction::SetFrozen {
            namespace,
            frozen: true,
        },
    );
    for instruction in [
        Instruction::SwapExactInput {
            amount_in: 0,
            min_amount_out: u128::MAX,
            token_in: id(90),
        },
        Instruction::SwapExactOutput {
            amount_out: u128::MAX,
            max_amount_in: 0,
            token_in: id(4),
        },
    ] {
        let mut accounts = swap_accounts(namespace, config.clone(), id(5));
        let mut state = PoolAccount::try_from(&accounts[0].account.data).unwrap();
        state.funded = false;
        accounts[0].account.data = Data::from(&state);
        let mut unsigned = accounts.clone();
        unsigned[2].is_authorized = false;
        assert_eq!(
            rejection(unsigned, instruction.clone()),
            "Participant authorization is missing"
        );
        for (index, expected) in [
            (0, "Pool account ID does not match PDA"),
            (1, "Config account ID does not match PDA"),
        ] {
            let mut invalid = accounts.clone();
            invalid[index].account_id = id(90);
            assert!(rejection(invalid, instruction.clone()).contains(expected));
        }
        let mut invalid_trade = accounts;
        let mut state = PoolAccount::try_from(&invalid_trade[0].account.data).unwrap();
        state.allowed_input_token = Some(pool::TokenSide::Token1);
        state.pool.lifecycle = pool::PoolLifecycle::Closed;
        invalid_trade[0].account.data = Data::from(&state);
        invalid_trade[3].account.data = Data::default();
        invalid_trade[4].account_id = id(91);
        invalid_trade[7].account_id = id(92);
        assert_eq!(
            rejection(invalid_trade, instruction),
            "Namespace swaps are frozen"
        );
    }
}

#[test]
fn freeze_is_namespace_local_and_unfreeze_restores_all_supported_swaps() {
    let namespace = id(1);
    let frozen = control(
        initialize(namespace),
        signer(emergency_authority()),
        Instruction::SetFrozen {
            namespace,
            frozen: true,
        },
    );
    let other = initialize(id(2));
    let unfrozen = control(
        frozen,
        signer(emergency_authority()),
        Instruction::SetFrozen {
            namespace,
            frozen: false,
        },
    );
    for (token_in, instruction, reserves) in [
        (
            id(4),
            Instruction::SwapExactInput {
                amount_in: 250,
                min_amount_out: 19,
                token_in: id(4),
            },
            (1_050, 80),
        ),
        (
            id(5),
            Instruction::SwapExactInput {
                amount_in: 25,
                min_amount_out: 186,
                token_in: id(5),
            },
            (614, 123),
        ),
        (
            id(5),
            Instruction::SwapExactOutput {
                amount_out: 200,
                max_amount_in: 27,
                token_in: id(5),
            },
            (600, 125),
        ),
    ] {
        for (ns, config) in [(namespace, unfrozen.clone()), (id(2), other.clone())] {
            let accounts = swap_accounts(ns, config, token_in);
            let (posts, calls) =
                process_instruction(accounts.clone(), instruction.clone(), PROGRAM);
            lee_core::program::validate_execution(&accounts, &posts, PROGRAM).unwrap();
            let state = PoolAccount::try_from(&posts[0].account().data).unwrap();
            assert_eq!(
                (state.pool.real_reserve0, state.pool.real_reserve1),
                reserves
            );
            assert_eq!(calls.len(), 2);
        }
    }
}

#[test]
fn unfreeze_does_not_reopen_closed_expired_or_withdrawn_pools() {
    let namespace = id(1);
    let config = control(
        initialize(namespace),
        signer(emergency_authority()),
        Instruction::SetFrozen {
            namespace,
            frozen: true,
        },
    );
    let config = control(
        config,
        signer(emergency_authority()),
        Instruction::SetFrozen {
            namespace,
            frozen: false,
        },
    );
    for (lifecycle, now) in [
        (pool::PoolLifecycle::Closed, 1),
        (pool::PoolLifecycle::Open, 42),
        (pool::PoolLifecycle::Withdrawn, 1),
    ] {
        let mut accounts = swap_accounts(namespace, config.clone(), id(5));
        let mut state = PoolAccount::try_from(&accounts[0].account.data).unwrap();
        state.pool.lifecycle = lifecycle;
        accounts[0].account.data = Data::from(&state);
        accounts[7].account.data = Data::try_from(
            clock_core::ClockAccountData {
                block_id: 7,
                timestamp: now,
            }
            .to_bytes(),
        )
        .unwrap();
        for instruction in [
            Instruction::SwapExactInput {
                amount_in: 25,
                min_amount_out: 0,
                token_in: id(5),
            },
            Instruction::SwapExactOutput {
                amount_out: 200,
                max_amount_in: 30,
                token_in: id(5),
            },
        ] {
            let error = rejection(accounts.clone(), instruction);
            assert!(error.contains("the pool is closed"), "{error}");
        }
    }
}

#[test]
fn emergency_transfer_is_immediate_without_successor_acceptance_and_keeps_freeze() {
    let namespace = id(1);
    let successor = id(102);
    for frozen in [false, true] {
        let config = control(
            initialize(namespace),
            signer(emergency_authority()),
            Instruction::SetFrozen { namespace, frozen },
        );
        let transferred = control(
            config,
            signer(emergency_authority()),
            Instruction::TransferEmergencyAuthority {
                namespace,
                new_authority: successor,
            },
        );
        let state = Config::try_from(&transferred.account.data).unwrap();
        assert_eq!(
            (
                state.current_admin(),
                state.protocol_fee_bps,
                state.treasury,
                state.emergency.authority(),
                state.emergency.is_frozen
            ),
            (Some(namespace), 500, id(6), successor, frozen),
        );
        for instruction in [
            Instruction::SetFrozen { namespace, frozen },
            Instruction::TransferEmergencyAuthority {
                namespace,
                new_authority: emergency_authority(),
            },
        ] {
            let error = rejection(
                vec![transferred.clone(), signer(emergency_authority())],
                instruction,
            );
            assert!(
                error.contains("signer is not the current authority"),
                "{error}"
            );
        }
        let changed = control(
            transferred,
            signer(successor),
            Instruction::SetFrozen {
                namespace,
                frozen: !frozen,
            },
        );
        assert_eq!(
            Config::try_from(&changed.account.data)
                .unwrap()
                .emergency
                .is_frozen,
            !frozen
        );
        assert_eq!(
            Config::try_from(&initialize(id(2)).account.data)
                .unwrap()
                .emergency
                .authority(),
            emergency_authority()
        );
    }
}

#[test]
fn emergency_authority_cannot_renounce_by_transfer_to_default() {
    let namespace = id(1);
    let error = rejection(
        vec![initialize(namespace), signer(emergency_authority())],
        Instruction::TransferEmergencyAuthority {
            namespace,
            new_authority: AccountId::default(),
        },
    );
    assert!(error.contains("invalid authority candidate"), "{error}");
}

#[test]
fn emergency_self_transfer_succeeds_but_unsigned_and_other_roles_cannot_transfer() {
    let namespace = id(1);
    let mut unsigned = signer(emergency_authority());
    unsigned.is_authorized = false;
    for frozen in [false, true] {
        let config = control(
            initialize(namespace),
            signer(emergency_authority()),
            Instruction::SetFrozen { namespace, frozen },
        );
        let self_transfer = control(
            config.clone(),
            signer(emergency_authority()),
            Instruction::TransferEmergencyAuthority {
                namespace,
                new_authority: emergency_authority(),
            },
        );
        assert_eq!(self_transfer.account, config.account);
        for (authority, expected) in [
            (unsigned.clone(), "authority signature missing"),
            (
                administrator(namespace),
                "signer is not the current authority",
            ),
            (signer(id(3)), "signer is not the current authority"),
        ] {
            for new_authority in [emergency_authority(), id(102), AccountId::default()] {
                let error = rejection(
                    vec![config.clone(), authority.clone()],
                    Instruction::TransferEmergencyAuthority {
                        namespace,
                        new_authority,
                    },
                );
                assert!(error.contains(expected), "{error}");
            }
        }
    }
}

#[test]
fn administrator_updates_preserve_transferred_emergency_authority_and_freeze() {
    let namespace = id(1);
    for frozen in [false, true] {
        let config = control(
            initialize(namespace),
            signer(emergency_authority()),
            Instruction::SetFrozen { namespace, frozen },
        );
        let config = control(
            config,
            signer(emergency_authority()),
            Instruction::TransferEmergencyAuthority {
                namespace,
                new_authority: id(102),
            },
        );
        let updated = control(
            config,
            administrator(namespace),
            Instruction::UpdateFee {
                namespace,
                protocol_fee_bps: 999,
            },
        );
        let updated = control(
            updated,
            administrator(namespace),
            Instruction::UpdateTreasury {
                namespace,
                treasury: id(8),
            },
        );
        let updated = transfer_admin(namespace, updated, administrator(namespace), id(7));
        let state = Config::try_from(&updated.account.data).unwrap();
        assert_eq!(
            (
                state.current_admin(),
                state.protocol_fee_bps,
                state.treasury,
                state.emergency.authority(),
                state.emergency.is_frozen
            ),
            (Some(id(7)), 999, id(8), id(102), frozen),
        );
    }
}

#[test]
fn administrator_renunciation_preserves_emergency_control() {
    let namespace = id(1);
    let config = control(
        initialize(namespace),
        signer(emergency_authority()),
        Instruction::SetFrozen {
            namespace,
            frozen: true,
        },
    );
    let config = control(
        config,
        signer(emergency_authority()),
        Instruction::TransferEmergencyAuthority {
            namespace,
            new_authority: id(102),
        },
    );
    let renounced = control(
        config,
        administrator(namespace),
        Instruction::RenounceAdmin { namespace },
    );
    let state = Config::try_from(&renounced.account.data).unwrap();
    assert_eq!(
        (
            state.current_admin(),
            state.protocol_fee_bps,
            state.treasury,
            state.emergency.authority(),
            state.emergency.is_frozen
        ),
        (None, 500, id(6), id(102), true),
    );
    let unfrozen = control(
        renounced.clone(),
        signer(id(102)),
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
    let error = rejection(
        vec![renounced, administrator(namespace)],
        Instruction::UpdateFee {
            namespace,
            protocol_fee_bps: 0,
        },
    );
    assert!(
        error.contains("Namespace administrator authorization failed: Renounced"),
        "{error}"
    );
}

#[test]
fn one_account_can_hold_both_roles_and_admin_transfer_keeps_emergency_power() {
    let namespace = id(1);
    let dual_holder = administrator(namespace);
    let config = control(
        initialize(namespace),
        signer(emergency_authority()),
        Instruction::TransferEmergencyAuthority {
            namespace,
            new_authority: dual_holder.account_id,
        },
    );
    let config = control(
        config,
        dual_holder.clone(),
        Instruction::SetFrozen {
            namespace,
            frozen: true,
        },
    );
    let config = transfer_admin(namespace, config, dual_holder.clone(), id(7));
    let error = rejection(
        vec![config.clone(), dual_holder.clone()],
        Instruction::RenounceAdmin { namespace },
    );
    assert!(
        error.contains("Namespace renunciation failed: NotAdmin"),
        "{error}"
    );
    let config = control(
        config,
        dual_holder.clone(),
        Instruction::SetFrozen {
            namespace,
            frozen: false,
        },
    );
    let state = Config::try_from(&config.account.data).unwrap();
    assert_eq!(
        (
            state.current_admin(),
            state.emergency.authority(),
            state.emergency.is_frozen
        ),
        (Some(id(7)), dual_holder.account_id, false)
    );
}

#[test]
fn frozen_namespace_keeps_permissionless_fee_collection_and_its_custody_checks() {
    let namespace = id(1);
    let config = control(
        initialize(namespace),
        signer(emergency_authority()),
        Instruction::SetFrozen {
            namespace,
            frozen: true,
        },
    );
    let mut swap = swap_accounts(namespace, config, id(5));
    let mut state = PoolAccount::try_from(&swap[0].account.data).unwrap();
    state.pool.fees_accrued = 7;
    swap[0].account.data = Data::from(&state);
    let pool_id = swap[0].account_id;
    let accounts = vec![
        swap[0].clone(),
        swap[1].clone(),
        holding(pool_id, id(5), 107),
        holding(id(6), id(5), 0),
    ];
    let (posts, calls) = process_instruction(accounts.clone(), Instruction::CollectFees, PROGRAM);
    lee_core::program::validate_execution(&accounts, &posts, PROGRAM).unwrap();
    let after = PoolAccount::try_from(&posts[0].account().data).unwrap();
    state.pool.fees_accrued = 0;
    state.pool.fees_collected = 7;
    assert_eq!(after, state);
    assert_eq!(posts[1].account(), &accounts[1].account);
    assert_eq!(calls.len(), 1);
    let instruction: associated_token_account_core::Instruction =
        risc0_zkvm::serde::from_slice(&calls[0].instruction_data).unwrap();
    assert!(matches!(
        instruction,
        associated_token_account_core::Instruction::Transfer { amount: 7, .. }
    ));
    let mut invalid = accounts;
    invalid[3] = holding(id(8), id(5), 0);
    assert!(
        rejection(invalid, Instruction::CollectFees)
            .contains("ATA account ID does not match expected derivation")
    );
}

#[test]
fn namespace_creation_sets_deployment_emergency_authority_unfrozen() {
    let config = initialize(id(1));
    let state = Config::try_from(&config.account.data).unwrap();
    assert_eq!(
        (state.emergency.authority(), state.emergency.is_frozen),
        (emergency_authority(), false),
    );
}

#[test]
fn emergency_authority_freezes_without_changing_namespace_settings() {
    let namespace = id(1);
    let config = control(
        initialize(namespace),
        signer(emergency_authority()),
        Instruction::SetFrozen {
            namespace,
            frozen: true,
        },
    );
    let state = Config::try_from(&config.account.data).unwrap();
    assert_eq!(
        (
            state.current_admin(),
            state.protocol_fee_bps,
            state.treasury,
            state.emergency.authority(),
            state.emergency.is_frozen,
        ),
        (Some(namespace), 500, id(6), emergency_authority(), true),
    );
}

#[test]
fn administrator_cannot_change_or_repeat_emergency_freeze() {
    let namespace = id(1);
    for current in [false, true] {
        let config = control(
            initialize(namespace),
            signer(emergency_authority()),
            Instruction::SetFrozen {
                namespace,
                frozen: current,
            },
        );
        for frozen in [false, true] {
            let error = rejection(
                vec![config.clone(), administrator(namespace)],
                Instruction::SetFrozen { namespace, frozen },
            );
            assert!(
                error.contains("signer is not the current authority"),
                "{error}"
            );
        }
    }
}

#[test]
fn emergency_holder_without_authorization_cannot_change_or_repeat_freeze() {
    let namespace = id(1);
    for current in [false, true] {
        let config = control(
            initialize(namespace),
            signer(emergency_authority()),
            Instruction::SetFrozen {
                namespace,
                frozen: current,
            },
        );
        let mut unsigned = signer(emergency_authority());
        unsigned.is_authorized = false;
        for frozen in [false, true] {
            let error = rejection(
                vec![config.clone(), unsigned.clone()],
                Instruction::SetFrozen { namespace, frozen },
            );
            assert!(error.contains("authority signature missing"), "{error}");
        }
    }
}

#[test]
fn freeze_rejects_wrong_namespace_owner_and_old_or_invalid_config_data() {
    let namespace = id(1);
    let valid = initialize(namespace);
    let mut wrong_namespace = valid.clone();
    wrong_namespace.account_id = compute_config_pda(id(2), PROGRAM);
    let mut wrong_owner = valid.clone();
    wrong_owner.account.program_owner = [43; 8];
    let mut invalid = valid.clone();
    invalid.account.data = Data::try_from(vec![1]).unwrap();
    let mut old_layout = valid;
    old_layout.account.data = Data::try_from(vec![0; 66]).unwrap();
    for (config, expected) in [
        (wrong_namespace, "Config account ID does not match PDA"),
        (wrong_owner, "Config owner does not match program"),
        (invalid, "Config account holds invalid data"),
        (old_layout, "Config account holds invalid data"),
    ] {
        let error = rejection(
            vec![config, signer(emergency_authority())],
            Instruction::SetFrozen {
                namespace,
                frozen: true,
            },
        );
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn authorized_repeated_freeze_and_unfreeze_calls_succeed() {
    let namespace = id(1);
    let mut config = initialize(namespace);
    for frozen in [true, true, false, false] {
        config = control(
            config,
            signer(emergency_authority()),
            Instruction::SetFrozen { namespace, frozen },
        );
        assert_eq!(
            Config::try_from(&config.account.data)
                .unwrap()
                .emergency
                .is_frozen,
            frozen,
        );
    }
}
