use super::*;

const FACTORY: ProgramId = [7; 8];
const CURVE: ProgramId = [6; 8];
const ADAPTER: ProgramId = [9; 8];

fn blank(id: AccountId) -> AccountWithMetadata {
    AccountWithMetadata {
        account: Account::default(),
        account_id: id,
        is_authorized: false,
    }
}

fn creation() -> (Vec<AccountWithMetadata>, Instruction) {
    let namespace = AccountId::new([0xAD; 32]);
    let salt = [3; 32];
    let factory = compute_factory_pda(namespace, FACTORY, salt);
    let definition = compute_definition_pda(namespace, FACTORY, salt);
    let collateral = AccountId::new([4; 32]);
    let creator = AccountId::new([8; 32]);
    let pool = compute_pool_pda(namespace, CURVE, definition, collateral, factory);
    let ata = |owner, token| {
        associated_token_account_core::get_associated_token_account_id(
            &ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
            &associated_token_account_core::compute_ata_seed(owner, token),
        )
    };
    let mut accounts = vec![
        blank(factory),
        blank(definition),
        blank(compute_mint_pda(namespace, FACTORY, salt)),
        blank(compute_metadata_pda(namespace, FACTORY, salt)),
        blank(compute_escrow_pda(namespace, FACTORY, salt)),
        blank(creator),
        blank(ata(creator, definition)),
        blank(collateral),
        blank(ata(factory, definition)),
        blank(ata(factory, collateral)),
        blank(pool),
        blank(ata(pool, definition)),
        blank(ata(pool, collateral)),
        blank(clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID),
        blank(namespace),
    ];
    accounts[5].is_authorized = true;
    accounts[5].account.program_owner = curve_core::TOKEN_PROGRAM_ID;
    accounts[5].account.data = Data::from(&TokenHolding::NftMaster {
        definition_id: creator,
        print_balance: 1,
    });
    accounts[13].account.data = Data::try_from(
        clock_core::ClockAccountData {
            block_id: 1,
            timestamp: 1,
        }
        .to_bytes(),
    )
    .unwrap();
    accounts[14].account_id = curve_core::compute_config_pda(namespace, CURVE);
    accounts[14].account.program_owner = CURVE;
    accounts[14].account.data = Data::from(&curve_core::Config {
        admin: AccountId::new([0xAD; 32]),
        protocol_fee_bps: 17,
        treasury: AccountId::new([20; 32]),
    });
    (
        accounts,
        Instruction::CreateFactoryPool {
            trade_mode: 0,
            namespace,
            launch_salt: salt,
            name: "Launch".into(),
            uri: "https://example.test/token".into(),
            sale_reserve: 800,
            dex_seed_reserve: 100,
            creator_allocation: 100,
            virtual_token_reserve: 2000,
            virtual_collateral_reserve: 100,
            end_timestamp: Some(2),
            curve_program_id: CURVE,
            graduation_hook_program_id: Some(ADAPTER),
            graduation_hook_configuration: vec![11, 22, 33],
        },
    )
}

#[test]
fn creation_saves_hook_settings_and_continuation_preserves_them() {
    let (mut accounts, instruction) = creation();
    let (posts, _) = process_instruction(accounts.clone(), instruction, FACTORY);
    accounts[0].account = posts[0].account().clone();
    accounts[0].account.program_owner = FACTORY;
    accounts[1].account.program_owner = curve_core::TOKEN_PROGRAM_ID;
    accounts[1].account.data = Data::from(&token_core::TokenDefinition::Fungible {
        name: "Launch".into(),
        total_supply: 1000,
        metadata_id: None,
    });
    accounts[2].account.program_owner = curve_core::TOKEN_PROGRAM_ID;
    accounts[2].account.data = Data::from(&TokenHolding::Fungible {
        definition_id: accounts[1].account_id,
        balance: 1000,
    });
    let (posts, _) = process_instruction(accounts, Instruction::ContinueCreation, FACTORY);
    let state = FactoryState::try_from(&posts[0].account().data).unwrap();
    assert_eq!(
        state.graduation_hook,
        Some(GraduationHook {
            program_id: ADAPTER,
            configuration: vec![11, 22, 33]
        })
    );
    assert_eq!(state.trade_mode, TradeMode::BuyOnly);
}

#[test]
#[should_panic(expected = "Hook configuration requires a program")]
fn creation_rejects_configuration_without_a_hook_program() {
    let (accounts, mut instruction) = creation();
    if let Instruction::CreateFactoryPool {
        graduation_hook_program_id,
        ..
    } = &mut instruction
    {
        *graduation_hook_program_id = None;
    }
    let _ = process_instruction(accounts, instruction, FACTORY);
}

fn ata(owner: AccountId, definition: AccountId) -> AccountId {
    associated_token_account_core::get_associated_token_account_id(
        &ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
        &associated_token_account_core::compute_ata_seed(owner, definition),
    )
}

fn definition(id: AccountId) -> AccountWithMetadata {
    let mut account = blank(id);
    account.account.program_owner = curve_core::TOKEN_PROGRAM_ID;
    account.account.data = Data::from(&token_core::TokenDefinition::Fungible {
        name: "Token".into(),
        total_supply: 1000,
        metadata_id: None,
    });
    account
}

fn holding(owner: AccountId, token: AccountId, balance: u128) -> AccountWithMetadata {
    let mut account = blank(ata(owner, token));
    account.account.program_owner = curve_core::TOKEN_PROGRAM_ID;
    account.account.data = Data::from(&TokenHolding::Fungible {
        definition_id: token,
        balance,
    });
    account
}

fn settlement(stage: SettlementStage) -> Vec<AccountWithMetadata> {
    let (creation, instruction) = creation();
    let (posts, _) = process_instruction(creation.clone(), instruction, FACTORY);
    let mut state = FactoryState::try_from(&posts[0].account().data).unwrap();
    state.creation_stage = CreationStage::Active;
    state.settlement_stage = stage;
    let mut factory = creation[0].clone();
    factory.account = posts[0].account().clone();
    factory.account.program_owner = FACTORY;
    factory.account.data = Data::from(&state);
    let custody = compute_graduation_pda(ADAPTER, factory.account_id);
    let mut custody_account = blank(custody);
    custody_account.account.program_owner = ADAPTER;
    let mut pool = creation[10].clone();
    pool.account.program_owner = CURVE;
    let mut reserves = pool::Pool::create(pool::PoolCreateParams {
        token0_amount: 70,
        token1_amount: 30,
        virtual_reserve0: 2000,
        virtual_reserve1: 100,
        close_timestamp: Some(2),
        close_on_depletion: Some(pool::TokenSide::Token0),
    })
    .unwrap();
    reserves.fees_accrued = 7;
    pool.account.data = Data::from(&PoolAccount {
        allowed_input_token: Some(pool::TokenSide::Token1),
        funded: true,
        namespace: state.namespace,
        token0_definition_id: state.token_definition_id,
        token1_definition_id: state.collateral_definition_id,
        owner: factory.account_id,
        owner_program: Some((
            FACTORY,
            *compute_factory_seed(state.namespace, state.launch_salt).as_bytes(),
        )),
        pool: reserves,
    });
    let mut clock = creation[13].clone();
    clock.account.data = Data::try_from(
        clock_core::ClockAccountData {
            block_id: 1,
            timestamp: 2,
        }
        .to_bytes(),
    )
    .unwrap();
    vec![
        factory.clone(),
        pool,
        custody_account,
        definition(state.token_definition_id),
        definition(state.collateral_definition_id),
        holding(factory.account_id, state.token_definition_id, 170),
        holding(factory.account_id, state.collateral_definition_id, 43),
        holding(state.pool_id, state.token_definition_id, 70),
        holding(state.pool_id, state.collateral_definition_id, 37),
        blank(ata(custody, state.token_definition_id)),
        blank(ata(custody, state.collateral_definition_id)),
        clock,
        creation[14].clone(),
        holding(AccountId::new([20; 32]), state.collateral_definition_id, 0),
    ]
}

#[test]
fn noncreator_can_prepare_factory_collateral_for_configured_hook() {
    let accounts = settlement(SettlementStage::Unstarted);
    let (posts, calls) = process_instruction(
        accounts.clone(),
        Instruction::ExecuteGraduationHook {
            remaining_accounts: 0,
        },
        FACTORY,
    );
    assert_eq!(
        FactoryState::try_from(&posts[0].account().data)
            .unwrap()
            .settlement_stage,
        SettlementStage::Ready
    );
    assert_eq!(posts.len(), 14);
    assert_eq!(calls[0].pre_states[0].account_id, accounts[0].account_id);
    assert_eq!(calls[0].pre_states[2].account_id, accounts[6].account_id);
}

#[test]
fn hook_prepares_both_adapter_atas_without_a_payout() {
    let accounts = settlement(SettlementStage::Burned { collateral_due: 30 });
    let (posts, calls) = process_instruction(
        accounts.clone(),
        Instruction::ExecuteGraduationHook {
            remaining_accounts: 0,
        },
        FACTORY,
    );
    assert_eq!(
        FactoryState::try_from(&posts[0].account().data)
            .unwrap()
            .settlement_stage,
        SettlementStage::HookReady { collateral_due: 30 }
    );
    assert_eq!(calls.len(), 2);
    for (call, slot) in calls.iter().zip([9, 10]) {
        let instruction: associated_token_account_core::Instruction =
            risc0_zkvm::serde::from_slice(&call.instruction_data).unwrap();
        assert!(matches!(
            instruction,
            associated_token_account_core::Instruction::Create { .. }
        ));
        assert_eq!(call.pre_states[0].account_id, accounts[2].account_id);
        assert_eq!(call.pre_states[2].account_id, accounts[slot].account_id);
        assert!(call.pda_seeds.is_empty());
    }
}

fn hook_ready() -> Vec<AccountWithMetadata> {
    let mut accounts = settlement(SettlementStage::HookReady { collateral_due: 30 });
    accounts[9] = holding(accounts[2].account_id, accounts[3].account_id, 5);
    accounts[10] = holding(accounts[2].account_id, accounts[4].account_id, 9);
    accounts
}

#[test]
fn final_hook_transfers_recorded_amounts_and_supplies_credited_custody() {
    let accounts = hook_ready();
    let (posts, calls) = process_instruction(
        accounts.clone(),
        Instruction::ExecuteGraduationHook {
            remaining_accounts: 0,
        },
        FACTORY,
    );
    assert_eq!(
        FactoryState::try_from(&posts[0].account().data)
            .unwrap()
            .settlement_stage,
        SettlementStage::Graduated
    );
    assert_eq!(calls.len(), 3);
    for (call, expected) in calls[..2].iter().zip([100, 30]) {
        let instruction: associated_token_account_core::Instruction =
            risc0_zkvm::serde::from_slice(&call.instruction_data).unwrap();
        assert!(
            matches!(instruction, associated_token_account_core::Instruction::Transfer { amount, .. } if amount == expected)
        );
        assert_eq!(call.pda_seeds.len(), 1);
    }
    let callback = &calls[2];
    assert_eq!(callback.program_id, ADAPTER);
    assert!(callback.pda_seeds.is_empty());
    assert!(callback.pre_states.iter().all(|a| !a.is_authorized));
    assert_eq!(
        callback
            .pre_states
            .iter()
            .map(|a| a.account_id)
            .collect::<Vec<_>>(),
        [0, 2, 3, 4, 9, 10].map(|i| accounts[i].account_id)
    );
    for (account, expected) in callback.pre_states[4..].iter().zip([105, 39]) {
        assert!(
            matches!(TokenHolding::try_from(&account.account.data).unwrap(), TokenHolding::Fungible { balance, .. } if balance == expected)
        );
    }
    assert_eq!(
        FactoryState::try_from(&callback.pre_states[0].account.data)
            .unwrap()
            .settlement_stage,
        SettlementStage::Graduated
    );
    let request: GraduationRequest =
        risc0_zkvm::serde::from_slice(&callback.instruction_data).unwrap();
    assert_eq!(request.factory_program_id, FACTORY);
    assert_eq!(request.dex_seed_reserve, 100);
    assert_eq!(request.collateral_amount, 30);
    assert_eq!(request.configuration, [11, 22, 33]);
    assert_eq!(request.namespace, AccountId::new([0xAD; 32]));
    assert_eq!(request.launch_salt, [3; 32]);
    assert_eq!(request.token_definition_id, accounts[3].account_id);
    assert_eq!(request.collateral_definition_id, accounts[4].account_id);
}

fn manual(stage: SettlementStage) -> Vec<AccountWithMetadata> {
    let mut accounts = settlement(stage);
    accounts[2] = creation().0[5].clone();
    accounts[9] = blank(ata(accounts[2].account_id, accounts[3].account_id));
    accounts[10] = blank(ata(accounts[2].account_id, accounts[4].account_id));
    accounts
}

#[test]
fn creator_can_choose_manual_payout_after_hook_ata_preparation() {
    let (posts, calls) = process_instruction(
        manual(SettlementStage::HookReady { collateral_due: 30 }),
        Instruction::WithdrawFactoryProceeds,
        FACTORY,
    );
    assert_eq!(
        FactoryState::try_from(&posts[0].account().data)
            .unwrap()
            .settlement_stage,
        SettlementStage::TokenPaid { collateral_due: 30 }
    );
    let instruction: associated_token_account_core::Instruction =
        risc0_zkvm::serde::from_slice(&calls[1].instruction_data).unwrap();
    assert!(matches!(
        instruction,
        associated_token_account_core::Instruction::Transfer { amount: 100, .. }
    ));
}

#[test]
#[should_panic(expected = "Manual payout has started")]
fn manual_token_payout_blocks_hook_execution() {
    let _ = process_instruction(
        settlement(SettlementStage::TokenPaid { collateral_due: 30 }),
        Instruction::ExecuteGraduationHook {
            remaining_accounts: 0,
        },
        FACTORY,
    );
}

fn set_r(accounts: &mut [AccountWithMetadata], amount: u128) {
    let mut state = FactoryState::try_from(&accounts[0].account.data).unwrap();
    state.dex_seed_reserve = amount;
    state.total_supply = state.sale_reserve + amount + state.creator_allocation;
    accounts[0].account.data = Data::from(&state);
}

#[test]
fn zero_r_manual_settlement_pays_collateral_without_an_empty_token_marker() {
    let mut accounts = manual(SettlementStage::Burned { collateral_due: 30 });
    set_r(&mut accounts, 0);
    let (posts, calls) = process_instruction(
        accounts.clone(),
        Instruction::WithdrawFactoryProceeds,
        FACTORY,
    );
    assert_eq!(
        FactoryState::try_from(&posts[0].account().data)
            .unwrap()
            .settlement_stage,
        SettlementStage::Complete
    );
    let instruction: associated_token_account_core::Instruction =
        risc0_zkvm::serde::from_slice(&calls[1].instruction_data).unwrap();
    assert!(matches!(
        instruction,
        associated_token_account_core::Instruction::Transfer { amount: 30, .. }
    ));
    assert_eq!(calls[1].pre_states[1].account_id, accounts[6].account_id);
}

#[test]
fn zero_manual_payout_keeps_an_uncalled_zero_hook_available() {
    let mut accounts = manual(SettlementStage::Burned { collateral_due: 0 });
    set_r(&mut accounts, 0);
    let (posts, calls) = process_instruction(
        accounts.clone(),
        Instruction::WithdrawFactoryProceeds,
        FACTORY,
    );
    assert!(calls.is_empty());
    assert_eq!(
        FactoryState::try_from(&posts[0].account().data)
            .unwrap()
            .settlement_stage,
        SettlementStage::NoPayout
    );
    let mut hook_accounts = settlement(SettlementStage::NoPayout);
    hook_accounts[0].account = posts[0].account().clone();
    let (posts, calls) = process_instruction(
        hook_accounts.clone(),
        Instruction::ExecuteGraduationHook {
            remaining_accounts: 0,
        },
        FACTORY,
    );
    assert_eq!(calls.len(), 2);
    hook_accounts[0].account = posts[0].account().clone();
    hook_accounts[9] = holding(hook_accounts[2].account_id, hook_accounts[3].account_id, 0);
    hook_accounts[10] = holding(hook_accounts[2].account_id, hook_accounts[4].account_id, 0);
    let (posts, calls) = process_instruction(
        hook_accounts,
        Instruction::ExecuteGraduationHook {
            remaining_accounts: 0,
        },
        FACTORY,
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].program_id, ADAPTER);
    assert_eq!(
        FactoryState::try_from(&posts[0].account().data)
            .unwrap()
            .settlement_stage,
        SettlementStage::Graduated
    );
}

#[test]
fn hook_preserves_remaining_account_order_and_root_post_order() {
    let mut accounts = hook_ready();
    accounts.extend([
        blank(AccountId::new([31; 32])),
        blank(AccountId::new([32; 32])),
    ]);
    let (posts, calls) = process_instruction(
        accounts.clone(),
        Instruction::ExecuteGraduationHook {
            remaining_accounts: 2,
        },
        FACTORY,
    );
    assert_eq!(&calls.last().unwrap().pre_states[6..], &accounts[14..]);
    assert_eq!(posts.len(), 16);
    for (post, original) in posts[1..].iter().zip(&accounts[1..]) {
        assert_eq!(post.account(), &original.account);
    }
}

#[test]
fn hook_rejects_accounts_that_expose_factory_authority_or_protected_state() {
    for slot in [0, 2, 3, 4, 9, 10, 14] {
        let mut accounts = hook_ready();
        accounts.push(blank(AccountId::new([31; 32])));
        accounts[slot].is_authorized = true;
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                accounts,
                Instruction::ExecuteGraduationHook {
                    remaining_accounts: 1
                },
                FACTORY
            ))
            .is_err(),
            "authorized slot {slot} accepted"
        );
    }
    for slot in 0..14 {
        let mut accounts = hook_ready();
        accounts.push(accounts[slot].clone());
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                accounts,
                Instruction::ExecuteGraduationHook {
                    remaining_accounts: 1
                },
                FACTORY
            ))
            .is_err(),
            "protected alias {slot} accepted"
        );
    }
    let base = hook_ready();
    let state = FactoryState::try_from(&base[0].account.data).unwrap();
    for id in [
        state.creator_escrow_id,
        compute_mint_pda(state.namespace, FACTORY, state.launch_salt),
        compute_metadata_pda(state.namespace, FACTORY, state.launch_salt),
    ] {
        let mut accounts = base.clone();
        accounts.push(blank(id));
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                accounts,
                Instruction::ExecuteGraduationHook {
                    remaining_accounts: 1
                },
                FACTORY
            ))
            .is_err(),
            "factory identity accepted"
        );
    }
    let mut accounts = base.clone();
    let mut extra = blank(AccountId::new([31; 32]));
    extra.account.program_owner = FACTORY;
    accounts.push(extra);
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            accounts,
            Instruction::ExecuteGraduationHook {
                remaining_accounts: 1
            },
            FACTORY
        ))
        .is_err()
    );
    let mut accounts = base;
    let extra = blank(AccountId::new([31; 32]));
    accounts.extend([extra.clone(), extra]);
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            accounts,
            Instruction::ExecuteGraduationHook {
                remaining_accounts: 2
            },
            FACTORY
        ))
        .is_err()
    );
}

#[test]
fn settlement_rejects_noncanonical_pool_and_token_custody_before_children() {
    for route in [
        Instruction::WithdrawFactoryProceeds,
        Instruction::ExecuteGraduationHook {
            remaining_accounts: 0,
        },
    ] {
        let base = if route == Instruction::WithdrawFactoryProceeds {
            manual(SettlementStage::Ready)
        } else {
            settlement(SettlementStage::Ready)
        };
        for change in 0..8 {
            let mut accounts = base.clone();
            match change {
                0 => {
                    let mut state = PoolAccount::try_from(&accounts[1].account.data).unwrap();
                    state.owner = AccountId::new([55; 32]);
                    accounts[1].account.data = Data::from(&state);
                }
                1 => {
                    let mut state = PoolAccount::try_from(&accounts[1].account.data).unwrap();
                    state.token0_definition_id = AccountId::new([55; 32]);
                    accounts[1].account.data = Data::from(&state);
                }
                2 => accounts[7].account_id = AccountId::new([55; 32]),
                3 => accounts[13].account_id = AccountId::new([55; 32]),
                4 => accounts[5].account.program_owner = ADAPTER,
                5 => {
                    accounts[5].account.data = Data::from(&TokenHolding::Fungible {
                        definition_id: accounts[4].account_id,
                        balance: 170,
                    })
                }
                6 => accounts[3].account.program_owner = ADAPTER,
                7 => {
                    let mut state = PoolAccount::try_from(&accounts[1].account.data).unwrap();
                    state.owner_program = Some((ADAPTER, [0; 32]));
                    accounts[1].account.data = Data::from(&state);
                }
                _ => unreachable!(),
            }
            assert!(
                std::panic::catch_unwind(|| process_instruction(accounts, route.clone(), FACTORY))
                    .is_err(),
                "invalid settlement change {change} accepted for {route:?}"
            );
        }
    }
}

#[test]
fn hook_withdrawal_accepts_full_collateral_vault_when_it_is_the_treasury() {
    let mut accounts = settlement(SettlementStage::Ready);
    accounts[11].account.program_owner = [88; 8];
    let mut config = curve_core::Config::try_from(&accounts[12].account.data).unwrap();
    config.treasury = accounts[1].account_id;
    accounts[12].account.data = Data::from(&config);
    accounts[8] = holding(accounts[1].account_id, accounts[4].account_id, u128::MAX);
    accounts.pop();

    let (posts, calls) = process_instruction(
        accounts.clone(),
        Instruction::ExecuteGraduationHook {
            remaining_accounts: 0,
        },
        FACTORY,
    );
    lee_core::program::validate_execution(&accounts, &posts, FACTORY).unwrap();
    assert_eq!(posts.len(), 13);
    assert_eq!(
        FactoryState::try_from(&posts[0].account().data)
            .unwrap()
            .settlement_stage,
        SettlementStage::Withdrawn {
            unsold: 70,
            collateral_due: 30,
        }
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].program_id, CURVE);
    assert_eq!(calls[0].pre_states.len(), 8);
    assert_eq!(calls[0].pre_states[5], accounts[8]);
}

#[test]
fn settlement_checks_custody_liabilities_and_credits_before_children() {
    for change in 0..6 {
        let stage = if change < 4 {
            SettlementStage::Ready
        } else {
            SettlementStage::Withdrawn {
                unsold: 70,
                collateral_due: 30,
            }
        };
        let mut accounts = settlement(stage);
        match change {
            0 => accounts[7] = holding(accounts[1].account_id, accounts[3].account_id, 69),
            1 => accounts[8] = holding(accounts[1].account_id, accounts[4].account_id, 36),
            2 => accounts[6] = holding(accounts[0].account_id, accounts[4].account_id, u128::MAX),
            3 => {
                accounts[13] = holding(AccountId::new([20; 32]), accounts[4].account_id, u128::MAX)
            }
            4 => accounts[5] = holding(accounts[0].account_id, accounts[3].account_id, 69),
            5 => {
                accounts[3].account.data = Data::from(&token_core::TokenDefinition::Fungible {
                    name: "Token".into(),
                    total_supply: 69,
                    metadata_id: None,
                })
            }
            _ => unreachable!(),
        }
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                accounts,
                Instruction::ExecuteGraduationHook {
                    remaining_accounts: 0
                },
                FACTORY
            ))
            .is_err(),
            "unsafe custody change {change} accepted"
        );
    }
}

#[test]
#[should_panic(expected = "Graduation hook requires public execution")]
fn private_wrapped_hook_is_rejected_at_the_public_boundary() {
    let _ = process_instruction(
        hook_ready(),
        Instruction::Private {
            instruction: Box::new(Instruction::ExecuteGraduationHook {
                remaining_accounts: 0,
            }),
        },
        FACTORY,
    );
}

#[test]
fn private_factory_wraps_only_children_for_the_saved_curve_program() {
    let accounts = manual(SettlementStage::Unstarted);
    let state = FactoryState::try_from(&accounts[0].account.data).unwrap();
    let mut escrow = blank(state.creator_escrow_id);
    escrow.account.program_owner = ADAPTER;
    escrow.account.data = Data::from(&TokenHolding::Fungible {
        definition_id: state.token_definition_id,
        balance: 100,
    });
    let claim = vec![
        accounts[0].clone(),
        accounts[1].clone(),
        escrow,
        accounts[2].clone(),
        accounts[3].clone(),
        accounts[9].clone(),
        accounts[11].clone(),
    ];
    let (_, calls) = process_instruction(
        claim,
        Instruction::Private {
            instruction: Box::new(Instruction::ClaimCreatorAllocation),
        },
        FACTORY,
    );
    assert_eq!(calls[1].program_id, ADAPTER);
    let instruction: token_core::Instruction =
        risc0_zkvm::serde::from_slice(&calls[1].instruction_data).unwrap();
    assert!(matches!(
        instruction,
        token_core::Instruction::Transfer {
            amount_to_transfer: 100
        }
    ));
}

#[test]
fn omitted_treasury_is_reconstructed_from_fixed_accounts_without_reordering_remaining() {
    let mut accounts = hook_ready();
    let mut config = curve_core::Config::try_from(&accounts[12].account.data).unwrap();
    config.treasury = accounts[0].account_id;
    accounts[12].account.data = Data::from(&config);
    accounts.pop();
    accounts.extend([
        blank(AccountId::new([31; 32])),
        blank(AccountId::new([32; 32])),
    ]);
    let (posts, calls) = process_instruction(
        accounts.clone(),
        Instruction::ExecuteGraduationHook {
            remaining_accounts: 2,
        },
        FACTORY,
    );
    assert_eq!(posts.len(), 15);
    assert_eq!(&calls.last().unwrap().pre_states[6..], &accounts[13..]);
    for (post, original) in posts[1..].iter().zip(&accounts[1..]) {
        assert_eq!(post.account(), &original.account);
    }
}

#[test]
fn hook_validates_remaining_count_before_account_split() {
    for count in [1, 2, 14, 15, u32::MAX] {
        let accounts = hook_ready();
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                accounts,
                Instruction::ExecuteGraduationHook {
                    remaining_accounts: count
                },
                FACTORY
            ))
            .is_err(),
            "bad remaining count {count} accepted"
        );
    }
}

#[test]
fn successful_hook_blocks_replay_and_manual_payout() {
    for instruction in [
        Instruction::WithdrawFactoryProceeds,
        Instruction::ExecuteGraduationHook {
            remaining_accounts: 0,
        },
    ] {
        let accounts = if instruction == Instruction::WithdrawFactoryProceeds {
            manual(SettlementStage::Graduated)
        } else {
            settlement(SettlementStage::Graduated)
        };
        assert!(
            std::panic::catch_unwind(|| process_instruction(accounts, instruction, FACTORY))
                .is_err()
        );
    }
}

#[test]
fn hook_withdrawal_records_reserves_without_fees_and_burns_only_unsold_d() {
    let accounts = settlement(SettlementStage::Ready);
    let (posts, calls) = process_instruction(
        accounts.clone(),
        Instruction::ExecuteGraduationHook {
            remaining_accounts: 0,
        },
        FACTORY,
    );
    assert_eq!(
        FactoryState::try_from(&posts[0].account().data)
            .unwrap()
            .settlement_stage,
        SettlementStage::Withdrawn {
            unsold: 70,
            collateral_due: 30
        }
    );
    assert_eq!(calls[0].program_id, CURVE);
    assert_eq!(
        calls[0].pre_states.last().unwrap().account_id,
        accounts[13].account_id
    );
    let instruction: CurveInstruction =
        risc0_zkvm::serde::from_slice(&calls[0].instruction_data).unwrap();
    assert!(matches!(instruction, CurveInstruction::WithdrawReserves));
    let (posts, calls) = process_instruction(
        settlement(SettlementStage::Withdrawn {
            unsold: 70,
            collateral_due: 30,
        }),
        Instruction::ExecuteGraduationHook {
            remaining_accounts: 0,
        },
        FACTORY,
    );
    assert_eq!(
        FactoryState::try_from(&posts[0].account().data)
            .unwrap()
            .settlement_stage,
        SettlementStage::Burned { collateral_due: 30 }
    );
    let instruction: associated_token_account_core::Instruction =
        risc0_zkvm::serde::from_slice(&calls[0].instruction_data).unwrap();
    assert!(matches!(
        instruction,
        associated_token_account_core::Instruction::Burn { amount: 70, .. }
    ));
}

#[test]
fn creation_wire_rejects_invalid_or_missing_trade_mode() {
    let (accounts, mut instruction) = creation();
    if let Instruction::CreateFactoryPool { trade_mode, .. } = &mut instruction {
        *trade_mode = 2;
    }
    assert!(
        std::panic::catch_unwind(|| process_instruction(accounts, instruction.clone(), FACTORY))
            .is_err()
    );
    let mut words = risc0_zkvm::serde::to_vec(&instruction).unwrap();
    words.remove(1);
    assert!(risc0_zkvm::serde::from_slice::<Instruction, _>(&words).is_err());
}

#[test]
fn manual_payout_checks_source_balance_and_recipient_credit_before_children() {
    for (stage, source, destination, token_slot, amount) in [
        (SettlementStage::Burned { collateral_due: 30 }, 5, 9, 3, 100),
        (
            SettlementStage::TokenPaid { collateral_due: 30 },
            6,
            10,
            4,
            30,
        ),
    ] {
        for underfunded in [true, false] {
            let mut accounts = manual(stage);
            if underfunded {
                accounts[source] = holding(
                    accounts[0].account_id,
                    accounts[token_slot].account_id,
                    amount - 1,
                );
            } else {
                accounts[destination] = holding(
                    accounts[2].account_id,
                    accounts[token_slot].account_id,
                    u128::MAX,
                );
            }
            assert!(
                std::panic::catch_unwind(|| process_instruction(
                    accounts,
                    Instruction::WithdrawFactoryProceeds,
                    FACTORY
                ))
                .is_err(),
                "unsafe manual payout accepted"
            );
        }
    }
}

#[test]
#[should_panic(expected = "Adapter must initialize its custody PDA before ATA preparation")]
fn adapter_must_claim_its_custody_before_factory_prepares_atas() {
    let mut accounts = settlement(SettlementStage::Burned { collateral_due: 30 });
    accounts[2].account = Account::default();
    let _ = process_instruction(
        accounts,
        Instruction::ExecuteGraduationHook {
            remaining_accounts: 0,
        },
        FACTORY,
    );
}
