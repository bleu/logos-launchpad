use lbp_core::{
    CreatorAllocationRelease, FixedPrice, FixedPricePoolInputs, Instruction, PoolAccount, PoolMode,
    compute_pool_pda, process_instruction,
};
use lee_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{PdaSeed, ProgramId},
};
use token_core::{TokenDefinition, TokenHolding};

const LBP: ProgramId = [8; 8];
const FACTORY: ProgramId = [7; 8];
const NAMESPACE_PROGRAM: ProgramId = [6; 8];

fn account(id: AccountId) -> AccountWithMetadata {
    AccountWithMetadata {
        account_id: id,
        account: Account::default(),
        is_authorized: false,
    }
}

fn ata(owner: AccountId, definition: AccountId) -> AccountId {
    AccountId::for_public_pda(
        &curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
        &associated_token_account_core::compute_ata_seed(owner, definition),
    )
}

fn holding(account: &mut AccountWithMetadata, definition_id: AccountId, balance: u128) {
    account.account.program_owner = curve_core::TOKEN_PROGRAM_ID;
    account.account.data = Data::from(&TokenHolding::Fungible {
        definition_id,
        balance,
    });
}

struct Setup {
    inputs: FixedPricePoolInputs,
    accounts: Vec<AccountWithMetadata>,
}

impl Setup {
    fn rejects(self, message: &str) {
        let instruction = if self.accounts[0].account == Account::default() {
            Instruction::PrepareFixedPrice {
                inputs: self.inputs.clone(),
            }
        } else {
            Instruction::ActivateFixedPrice
        };
        self.rejects_instruction(instruction, message);
    }

    fn rejects_instruction(self, instruction: Instruction, message: &str) {
        let error =
            match std::panic::catch_unwind(|| process_instruction(self.accounts, instruction, LBP))
            {
                Ok(_) => panic!("Invalid setup was accepted"),
                Err(error) => error,
            };
        let actual = error
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| error.downcast_ref::<&str>().copied())
            .unwrap_or("Unknown panic");
        assert!(
            actual.contains(message),
            "Expected {message:?}, got {actual:?}"
        );
    }

    fn prepared() -> Self {
        let mut setup = Self::new();
        let (posts, _) = process_instruction(
            setup.accounts.clone(),
            Instruction::PrepareFixedPrice {
                inputs: setup.inputs.clone(),
            },
            LBP,
        );
        setup.accounts[0].account = posts[0].account().clone();
        setup.accounts[0].account.program_owner = LBP;
        let token = setup.accounts[2].account_id;
        let collateral = setup.accounts[3].account_id;
        holding(&mut setup.accounts[5], token, 0);
        holding(&mut setup.accounts[6], collateral, 0);
        setup
    }

    fn new() -> Self {
        let namespace = AccountId::new([40; 32]);
        let token = AccountId::new([41; 32]);
        let collateral = AccountId::new([42; 32]);
        let factory_seed = [3; 32];
        let factory = AccountId::for_public_pda(&FACTORY, &PdaSeed::new(factory_seed));
        let pool = compute_pool_pda(namespace, LBP, token, collateral, factory);
        let mut accounts = vec![
            account(pool),
            account(factory),
            account(token),
            account(collateral),
            account(ata(factory, token)),
            account(ata(pool, token)),
            account(ata(pool, collateral)),
            account(clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID),
            account(curve_core::compute_config_pda(namespace, NAMESPACE_PROGRAM)),
        ];
        accounts[1].account.program_owner = FACTORY;
        accounts[1].account.data = vec![1].try_into().unwrap();
        accounts[1].is_authorized = true;
        for index in [2, 3] {
            accounts[index].account.program_owner = curve_core::TOKEN_PROGRAM_ID;
            accounts[index].account.data = Data::from(&TokenDefinition::Fungible {
                name: "Token".into(),
                total_supply: 1_000,
                metadata_id: None,
            });
        }
        holding(&mut accounts[4], token, 800);
        accounts[7].account.data = clock_core::ClockAccountData {
            block_id: 1,
            timestamp: 100,
        }
        .to_bytes()
        .try_into()
        .unwrap();
        accounts[8].account.program_owner = NAMESPACE_PROGRAM;
        accounts[8].account.data = Data::from(&curve_core::Config::new(
            namespace,
            100,
            AccountId::new([43; 32]),
        ));
        Self {
            inputs: FixedPricePoolInputs {
                namespace,
                namespace_program_id: NAMESPACE_PROGRAM,
                factory_program_id: FACTORY,
                factory_seed,
                sale_inventory: 800,
                creator_allocation: 200,
                price: FixedPrice {
                    collateral_units: 3,
                    token_units: 2,
                },
                start_timestamp: 100,
                end_timestamp: Some(200),
                creator_allocation_release: CreatorAllocationRelease::AfterSetup,
            },
            accounts,
        }
    }
}

fn active_sale() -> Setup {
    let mut setup = Setup::prepared();
    let (posts, _) =
        process_instruction(setup.accounts.clone(), Instruction::ActivateFixedPrice, LBP);
    setup.accounts[0].account = posts[0].account().clone();
    setup.accounts[0].account.program_owner = LBP;
    let token_definition = setup.accounts[2].account_id;
    holding(&mut setup.accounts[4], token_definition, 0);
    holding(
        &mut setup.accounts[5],
        token_definition,
        setup.inputs.sale_inventory,
    );
    setup
}

fn buyer_accounts(
    setup: &Setup,
    participant_id: AccountId,
    collateral_balance: u128,
) -> Vec<AccountWithMetadata> {
    let mut participant = account(participant_id);
    participant.is_authorized = true;
    let collateral_definition = setup.accounts[3].account_id;
    let token_definition = setup.accounts[2].account_id;
    let mut participant_collateral = account(ata(participant_id, collateral_definition));
    holding(
        &mut participant_collateral,
        collateral_definition,
        collateral_balance,
    );
    let mut participant_token = account(ata(participant_id, token_definition));
    holding(&mut participant_token, token_definition, 0);
    vec![
        setup.accounts[0].clone(),
        setup.accounts[8].clone(),
        participant,
        participant_collateral,
        setup.accounts[6].clone(),
        setup.accounts[5].clone(),
        participant_token,
        setup.accounts[7].clone(),
    ]
}

#[test]
fn setup_requires_distinct_and_saved_definition_ids() {
    let mut setup = Setup::new();
    setup.accounts[3] = setup.accounts[2].clone();
    setup.accounts[0].account_id = compute_pool_pda(
        setup.inputs.namespace,
        LBP,
        setup.accounts[2].account_id,
        setup.accounts[3].account_id,
        setup.accounts[1].account_id,
    );
    setup.accounts[5].account_id = ata(setup.accounts[0].account_id, setup.accounts[2].account_id);
    setup.accounts[6].account_id = setup.accounts[5].account_id;
    setup.rejects("Pool definitions must differ");
    for index in [2, 3] {
        let mut setup = Setup::prepared();
        setup.accounts[index].account_id = AccountId::new([99; 32]);
        setup.rejects("Definition ID does not match saved pool");
    }
}

#[test]
fn setup_rejects_prefunded_launch_vaults_and_initial_collateral() {
    for prepared in [false, true] {
        for index in [5, 6] {
            if prepared && index == 6 {
                continue;
            }
            for balance in [1, 800, u128::MAX] {
                let mut setup = if prepared {
                    Setup::prepared()
                } else {
                    Setup::new()
                };
                let definition = setup.accounts[if index == 6 { 3 } else { 2 }].account_id;
                holding(&mut setup.accounts[index], definition, balance);
                setup.rejects("Pool vault must be empty");
            }
        }
    }
}

#[test]
fn activation_leaves_unsolicited_collateral_outside_sale_accounting() {
    for balance in [1, 800, u128::MAX] {
        let mut setup = Setup::prepared();
        let definition = setup.accounts[3].account_id;
        holding(&mut setup.accounts[6], definition, balance);
        let collateral_vault = setup.accounts[6].account.clone();
        let (posts, calls) =
            process_instruction(setup.accounts, Instruction::ActivateFixedPrice, LBP);
        let state = PoolAccount::try_from(&posts[0].account().data).unwrap();
        assert!(state.funded);
        assert_eq!(state.recorded_collateral, 0);
        assert_eq!(state.sale_inventory, 800);
        assert_eq!(posts[6].account(), &collateral_vault);
        assert_eq!(calls.len(), 1);
        let instruction: associated_token_account_core::Instruction =
            risc0_zkvm::serde::from_slice(&calls[0].instruction_data).unwrap();
        assert!(matches!(
            instruction,
            associated_token_account_core::Instruction::Transfer {
                ata_program_id: curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                amount: 800,
            }
        ));
    }
}

#[test]
fn activation_rejects_recorded_collateral_in_a_pending_pool() {
    let mut setup = Setup::prepared();
    let mut state = PoolAccount::try_from(&setup.accounts[0].account.data).unwrap();
    state.recorded_collateral = 1;
    setup.accounts[0].account.data = Data::from(&state);
    let definition = setup.accounts[3].account_id;
    holding(&mut setup.accounts[6], definition, 1);
    setup.rejects("Pending pool must have zero recorded collateral");
}

#[test]
fn activation_requires_exact_inventory_in_factory_custody() {
    for balance in [0, 799, 801, 1_000, u128::MAX] {
        let mut setup = Setup::prepared();
        let definition = setup.accounts[2].account_id;
        holding(&mut setup.accounts[4], definition, balance);
        setup.rejects("Factory inventory must equal sale inventory");
    }
}

#[test]
fn setup_rejects_invalid_token_holdings_at_canonical_addresses() {
    for prepared in [false, true] {
        for index in [4, 5, 6] {
            for invalid in 0..5 {
                if !prepared && index != 4 && invalid == 4 {
                    continue;
                }
                let mut setup = if prepared {
                    Setup::prepared()
                } else {
                    Setup::new()
                };
                let definition = setup.accounts[if index == 6 { 3 } else { 2 }].account_id;
                holding(
                    &mut setup.accounts[index],
                    definition,
                    if index == 4 { 800 } else { 0 },
                );
                match invalid {
                    0 => setup.accounts[index].account.program_owner = LBP,
                    1 => {
                        setup.accounts[index].account.data = Data::from(&TokenHolding::NftMaster {
                            definition_id: definition,
                            print_balance: 1,
                        })
                    }
                    2 => holding(&mut setup.accounts[index], AccountId::new([99; 32]), 0),
                    3 => setup.accounts[index].account.data = vec![99].try_into().unwrap(),
                    _ => setup.accounts[index].account = Account::default(),
                }
                setup.rejects("Token holding");
            }
        }
    }
}

#[test]
fn setup_rejects_noncanonical_factory_or_pool_token_accounts() {
    for prepared in [false, true] {
        for index in [4, 5, 6] {
            let mut setup = if prepared {
                Setup::prepared()
            } else {
                Setup::new()
            };
            setup.accounts[index].account_id = AccountId::new([99; 32]);
            setup.rejects("ATA account ID");
        }
    }
}

#[test]
fn setup_requires_fungible_definitions_owned_by_the_pinned_token_program() {
    for prepared in [false, true] {
        for index in [2, 3] {
            for invalid in 0..3 {
                let mut setup = if prepared {
                    Setup::prepared()
                } else {
                    Setup::new()
                };
                match invalid {
                    0 => setup.accounts[index].account.program_owner = LBP,
                    1 => {
                        setup.accounts[index].account.data =
                            Data::from(&TokenDefinition::NonFungible {
                                name: "NFT".into(),
                                printable_supply: 1,
                                metadata_id: AccountId::new([50; 32]),
                            })
                    }
                    _ => setup.accounts[index].account.data = vec![99].try_into().unwrap(),
                }
                setup.rejects("Token definition");
            }
        }
    }
}

#[test]
fn activation_cannot_fund_the_same_pool_twice() {
    let mut setup = Setup::prepared();
    let (posts, _) =
        process_instruction(setup.accounts.clone(), Instruction::ActivateFixedPrice, LBP);
    setup.accounts[0].account = posts[0].account().clone();
    setup.rejects("Pool is already funded");
}

#[test]
fn preparation_cannot_replace_a_prepared_pool_or_its_terms() {
    let mut setup = Setup::prepared();
    setup.inputs.sale_inventory = 900;
    setup.inputs.price.collateral_units = 9;
    let instruction = Instruction::PrepareFixedPrice {
        inputs: setup.inputs.clone(),
    };
    setup.rejects_instruction(instruction, "Pool is already initialized");
}

#[test]
fn setup_requires_the_canonical_pool_identity_and_owner() {
    for prepared in [false, true] {
        let mut setup = if prepared {
            Setup::prepared()
        } else {
            Setup::new()
        };
        setup.accounts[0].account_id = AccountId::new([99; 32]);
        setup.rejects("Pool account ID");
    }
    let mut setup = Setup::prepared();
    setup.accounts[0].account.program_owner = NAMESPACE_PROGRAM;
    setup.rejects("Pool program owner");
}

#[test]
fn setup_requires_the_saved_namespace_config() {
    for prepared in [false, true] {
        for invalid in 0..3 {
            let mut setup = if prepared {
                Setup::prepared()
            } else {
                Setup::new()
            };
            let message = match invalid {
                0 => {
                    setup.accounts[8].account_id =
                        curve_core::compute_config_pda(AccountId::new([99; 32]), NAMESPACE_PROGRAM);
                    "Config account ID"
                }
                1 => {
                    setup.accounts[8].account.program_owner = LBP;
                    "Config owner"
                }
                _ => {
                    setup.accounts[8].account.data = vec![1].try_into().unwrap();
                    "Config account holds invalid data"
                }
            };
            setup.rejects(message);
        }
    }
}

#[test]
fn setup_keeps_the_original_schedule_and_stops_at_the_saved_end() {
    for prepared in [false, true] {
        for timestamp in [100, 150, 199, 200, 201] {
            let mut setup = if prepared {
                Setup::prepared()
            } else {
                Setup::new()
            };
            setup.accounts[7].account.data = clock_core::ClockAccountData {
                block_id: 2,
                timestamp,
            }
            .to_bytes()
            .try_into()
            .unwrap();
            if timestamp >= 200 {
                setup.rejects("Sale has ended");
            } else {
                let instruction = if prepared {
                    Instruction::ActivateFixedPrice
                } else {
                    Instruction::PrepareFixedPrice {
                        inputs: setup.inputs,
                    }
                };
                let (posts, _) = process_instruction(setup.accounts, instruction, LBP);
                let state = PoolAccount::try_from(&posts[0].account().data).unwrap();
                assert_eq!(
                    (state.start_timestamp, state.end_timestamp),
                    (100, Some(200))
                );
                assert_eq!(state.funded, prepared);
            }
        }
    }
}

#[test]
fn setup_rejects_a_substituted_clock() {
    for prepared in [false, true] {
        let mut setup = if prepared {
            Setup::prepared()
        } else {
            Setup::new()
        };
        setup.accounts[7].account_id = AccountId::new([99; 32]);
        setup.rejects("Trusted LEZ clock");
    }
}

#[test]
fn preparation_requires_end_after_start() {
    for end in [99, 100] {
        let mut setup = Setup::new();
        setup.inputs.end_timestamp = Some(end);
        setup.rejects("End must be after start");
    }
}

#[test]
fn preparation_requires_positive_inventory_and_price_units() {
    for invalid in 0..3 {
        let mut setup = Setup::new();
        match invalid {
            0 => setup.inputs.sale_inventory = 0,
            1 => setup.inputs.price.collateral_units = 0,
            _ => setup.inputs.price.token_units = 0,
        }
        setup.rejects("must be positive");
    }
}

#[test]
fn setup_accepts_only_authorized_factory_pda_custody() {
    for prepared in [false, true] {
        for invalid in 0..3 {
            let mut setup = if prepared {
                Setup::prepared()
            } else {
                Setup::new()
            };
            match invalid {
                0 => setup.accounts[1].is_authorized = false,
                1 => setup.accounts[1].account.program_owner = NAMESPACE_PROGRAM,
                _ => setup.accounts[1].account_id = setup.inputs.namespace,
            }
            setup.rejects("Factory authority");
        }
    }
}

#[test]
fn activation_funds_only_the_saved_sale_inventory_and_preserves_the_terms() {
    let setup = Setup::prepared();
    let mut expected = PoolAccount::try_from(&setup.accounts[0].account.data).unwrap();
    expected.funded = true;
    assert_eq!(expected.recorded_collateral, 0);
    let source = setup.accounts[4].clone();
    let vault = setup.accounts[5].clone();
    let (posts, calls) = process_instruction(setup.accounts, Instruction::ActivateFixedPrice, LBP);
    assert_eq!(
        PoolAccount::try_from(&posts[0].account().data).unwrap(),
        expected
    );
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(
        call.program_id,
        curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID
    );
    assert_eq!(call.pre_states[0].account_id, expected.factory_owner);
    assert!(call.pre_states[0].is_authorized);
    assert_eq!(call.pre_states[1], source);
    assert_eq!(call.pre_states[2], vault);
    let instruction: associated_token_account_core::Instruction =
        risc0_zkvm::serde::from_slice(&call.instruction_data).unwrap();
    assert!(matches!(
        instruction,
        associated_token_account_core::Instruction::Transfer {
            ata_program_id: curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
            amount: 800,
        }
    ));
    assert_eq!(
        TokenHolding::try_from(&posts[6].account().data).unwrap(),
        TokenHolding::Fungible {
            definition_id: AccountId::new([42; 32]),
            balance: 0,
        }
    );
}

#[test]
fn exact_input_buy_uses_full_payment_and_tracks_creator_proceeds_and_fees_separately() {
    let mut setup = Setup::prepared();
    let (posts, _) =
        process_instruction(setup.accounts.clone(), Instruction::ActivateFixedPrice, LBP);
    setup.accounts[0].account = posts[0].account().clone();
    setup.accounts[0].account.program_owner = LBP;
    let token_definition = setup.accounts[2].account_id;
    holding(&mut setup.accounts[4], token_definition, 0);
    holding(
        &mut setup.accounts[5],
        token_definition,
        setup.inputs.sale_inventory,
    );

    let participant_id = AccountId::new([44; 32]);
    let mut participant = account(participant_id);
    participant.is_authorized = true;
    let mut participant_collateral = account(ata(participant_id, setup.accounts[3].account_id));
    holding(
        &mut participant_collateral,
        setup.accounts[3].account_id,
        100,
    );
    let mut participant_token = account(ata(participant_id, setup.accounts[2].account_id));
    holding(&mut participant_token, setup.accounts[2].account_id, 0);

    let pre = vec![
        setup.accounts[0].clone(),
        setup.accounts[8].clone(),
        participant,
        participant_collateral,
        setup.accounts[6].clone(),
        setup.accounts[5].clone(),
        participant_token,
        setup.accounts[7].clone(),
    ];
    let (posts, calls) = process_instruction(
        pre,
        Instruction::BuyExactInput {
            amount_in: 10,
            min_amount_out: 6,
        },
        LBP,
    );

    let state = PoolAccount::try_from(&posts[0].account().data).unwrap();
    assert_eq!(state.recorded_collateral, 9);
    assert_eq!(state.accrued_fees, 1);
    assert_eq!(state.remaining_inventory, 794);
    assert_eq!(calls.len(), 2);
    let input_transfer: associated_token_account_core::Instruction =
        risc0_zkvm::serde::from_slice(&calls[0].instruction_data).unwrap();
    let output_transfer: associated_token_account_core::Instruction =
        risc0_zkvm::serde::from_slice(&calls[1].instruction_data).unwrap();
    assert!(matches!(
        input_transfer,
        associated_token_account_core::Instruction::Transfer { amount: 10, .. }
    ));
    assert!(matches!(
        output_transfer,
        associated_token_account_core::Instruction::Transfer { amount: 6, .. }
    ));
}

#[test]
fn exact_input_buy_allows_the_fee_to_consume_the_full_payment() {
    let mut setup = Setup::prepared();
    let (posts, _) =
        process_instruction(setup.accounts.clone(), Instruction::ActivateFixedPrice, LBP);
    setup.accounts[0].account = posts[0].account().clone();
    setup.accounts[0].account.program_owner = LBP;
    let namespace = setup.inputs.namespace;
    setup.accounts[8].account.data = Data::from(&curve_core::Config::new(
        namespace,
        9_999,
        AccountId::new([43; 32]),
    ));
    let token_definition = setup.accounts[2].account_id;
    holding(&mut setup.accounts[4], token_definition, 0);
    holding(&mut setup.accounts[5], token_definition, 800);

    let participant_id = AccountId::new([44; 32]);
    let mut participant = account(participant_id);
    participant.is_authorized = true;
    let collateral_definition = setup.accounts[3].account_id;
    let mut participant_collateral = account(ata(participant_id, collateral_definition));
    holding(&mut participant_collateral, collateral_definition, 100);
    let mut participant_token = account(ata(participant_id, token_definition));
    holding(&mut participant_token, token_definition, 0);
    let (posts, _) = process_instruction(
        vec![
            setup.accounts[0].clone(),
            setup.accounts[8].clone(),
            participant,
            participant_collateral,
            setup.accounts[6].clone(),
            setup.accounts[5].clone(),
            participant_token,
            setup.accounts[7].clone(),
        ],
        Instruction::BuyExactInput {
            amount_in: 3,
            min_amount_out: 2,
        },
        LBP,
    );

    let state = PoolAccount::try_from(&posts[0].account().data).unwrap();
    assert_eq!(state.recorded_collateral, 0);
    assert_eq!(state.accrued_fees, 3);
    assert_eq!(state.remaining_inventory, 798);
}

#[test]
fn exact_input_rejects_dust_and_checked_math_overflow() {
    let setup = active_sale();
    assert!(
        std::panic::catch_unwind(|| {
            process_instruction(
                buyer_accounts(&setup, AccountId::new([44; 32]), 100),
                Instruction::BuyExactInput {
                    amount_in: 1,
                    min_amount_out: 0,
                },
                LBP,
            )
        })
        .is_err()
    );

    let mut setup = active_sale();
    let mut state = PoolAccount::try_from(&setup.accounts[0].account.data).unwrap();
    state.price = FixedPrice {
        collateral_units: 1,
        token_units: u128::MAX,
    };
    setup.accounts[0].account.data = Data::from(&state);
    assert!(
        std::panic::catch_unwind(|| {
            process_instruction(
                buyer_accounts(&setup, AccountId::new([44; 32]), 100),
                Instruction::BuyExactInput {
                    amount_in: 2,
                    min_amount_out: 1,
                },
                LBP,
            )
        })
        .is_err()
    );
}

#[test]
fn exact_output_buy_rounds_payment_up_and_enforces_the_full_input_limit() {
    let mut setup = Setup::prepared();
    let (posts, _) =
        process_instruction(setup.accounts.clone(), Instruction::ActivateFixedPrice, LBP);
    setup.accounts[0].account = posts[0].account().clone();
    setup.accounts[0].account.program_owner = LBP;
    let token_definition = setup.accounts[2].account_id;
    holding(&mut setup.accounts[4], token_definition, 0);
    holding(
        &mut setup.accounts[5],
        token_definition,
        setup.inputs.sale_inventory,
    );

    let participant_id = AccountId::new([44; 32]);
    let mut participant = account(participant_id);
    participant.is_authorized = true;
    let mut participant_collateral = account(ata(participant_id, setup.accounts[3].account_id));
    holding(
        &mut participant_collateral,
        setup.accounts[3].account_id,
        100,
    );
    let mut participant_token = account(ata(participant_id, token_definition));
    holding(&mut participant_token, token_definition, 0);
    let pre = vec![
        setup.accounts[0].clone(),
        setup.accounts[8].clone(),
        participant,
        participant_collateral,
        setup.accounts[6].clone(),
        setup.accounts[5].clone(),
        participant_token,
        setup.accounts[7].clone(),
    ];
    let (posts, calls) = process_instruction(
        pre,
        Instruction::BuyExactOutput {
            amount_out: 7,
            max_amount_in: 11,
        },
        LBP,
    );

    let state = PoolAccount::try_from(&posts[0].account().data).unwrap();
    assert_eq!(state.recorded_collateral, 10);
    assert_eq!(state.accrued_fees, 1);
    assert_eq!(state.remaining_inventory, 793);
    assert_eq!(calls.len(), 2);
    let input_transfer: associated_token_account_core::Instruction =
        risc0_zkvm::serde::from_slice(&calls[0].instruction_data).unwrap();
    let output_transfer: associated_token_account_core::Instruction =
        risc0_zkvm::serde::from_slice(&calls[1].instruction_data).unwrap();
    assert!(matches!(
        input_transfer,
        associated_token_account_core::Instruction::Transfer { amount: 11, .. }
    ));
    assert!(matches!(
        output_transfer,
        associated_token_account_core::Instruction::Transfer { amount: 7, .. }
    ));
}

#[test]
fn fee_collection_pays_the_current_treasury_without_changing_creator_proceeds() {
    let mut setup = Setup::prepared();
    let mut state = PoolAccount::try_from(&setup.accounts[0].account.data).unwrap();
    state.funded = true;
    state.recorded_collateral = 9;
    state.accrued_fees = 1;
    setup.accounts[0].account.program_owner = LBP;
    setup.accounts[0].account.data = Data::from(&state);
    let collateral_definition = setup.accounts[3].account_id;
    holding(&mut setup.accounts[6], collateral_definition, 10);

    let treasury = AccountId::new([45; 32]);
    setup.accounts[8].account.data = Data::from(&curve_core::Config::new(
        setup.inputs.namespace,
        100,
        treasury,
    ));
    let mut treasury_collateral = account(ata(treasury, collateral_definition));
    holding(&mut treasury_collateral, collateral_definition, 0);
    let (posts, calls) = process_instruction(
        vec![
            setup.accounts[0].clone(),
            setup.accounts[8].clone(),
            setup.accounts[6].clone(),
            treasury_collateral,
        ],
        Instruction::CollectFees,
        LBP,
    );

    let state = PoolAccount::try_from(&posts[0].account().data).unwrap();
    assert_eq!(state.recorded_collateral, 9);
    assert_eq!(state.accrued_fees, 0);
    assert_eq!(calls.len(), 1);
    let transfer: associated_token_account_core::Instruction =
        risc0_zkvm::serde::from_slice(&calls[0].instruction_data).unwrap();
    assert!(matches!(
        transfer,
        associated_token_account_core::Instruction::Transfer { amount: 1, .. }
    ));
    let mut pool = setup.accounts[0].clone();
    pool.account = posts[0].account().clone();
    let (posts, calls) = process_instruction(
        vec![
            pool,
            setup.accounts[8].clone(),
            setup.accounts[6].clone(),
            account(ata(treasury, collateral_definition)),
        ],
        Instruction::CollectFees,
        LBP,
    );
    assert_eq!(calls.len(), 0);
    assert_eq!(
        PoolAccount::try_from(&posts[0].account().data)
            .unwrap()
            .accrued_fees,
        0
    );
}

#[test]
fn sellout_stops_later_buys_and_keeps_proceeds_collectible() {
    let setup = active_sale();
    let participant = AccountId::new([44; 32]);
    let (posts, calls) = process_instruction(
        buyer_accounts(&setup, participant, 1_200),
        Instruction::BuyExactInput {
            amount_in: 1_200,
            min_amount_out: 800,
        },
        LBP,
    );
    let state = PoolAccount::try_from(&posts[0].account().data).unwrap();
    assert_eq!(state.remaining_inventory, 0);
    assert_eq!(state.recorded_collateral, 1_188);
    assert_eq!(state.accrued_fees, 12);
    assert_eq!(calls.len(), 2);

    let mut sold_out = setup.accounts[0].clone();
    sold_out.account = posts[0].account().clone();
    let mut pre = buyer_accounts(&setup, participant, 1_200);
    pre[0] = sold_out;
    let error = std::panic::catch_unwind(|| {
        process_instruction(
            pre,
            Instruction::BuyExactInput {
                amount_in: 3,
                min_amount_out: 2,
            },
            LBP,
        )
    })
    .unwrap_err();
    let message = error
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| error.downcast_ref::<&str>().copied())
        .unwrap_or("Unknown panic");
    assert!(message.contains("Sale inventory is exhausted"));
}

#[test]
fn buys_stop_at_the_configured_end_time() {
    let mut setup = active_sale();
    setup.accounts[7].account.data = clock_core::ClockAccountData {
        block_id: 2,
        timestamp: 200,
    }
    .to_bytes()
    .try_into()
    .unwrap();
    let error = std::panic::catch_unwind(|| {
        process_instruction(
            buyer_accounts(&setup, AccountId::new([44; 32]), 100),
            Instruction::BuyExactInput {
                amount_in: 10,
                min_amount_out: 6,
            },
            LBP,
        )
    })
    .unwrap_err();
    let message = error
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| error.downcast_ref::<&str>().copied())
        .unwrap_or("Unknown panic");
    assert!(message.contains("Sale has ended"));
}

#[test]
fn setup_and_activation_have_no_deadline_when_end_time_is_absent() {
    let mut setup = Setup::new();
    setup.inputs.end_timestamp = None;
    setup.accounts[7].account.data = clock_core::ClockAccountData {
        block_id: 2,
        timestamp: 1_000,
    }
    .to_bytes()
    .try_into()
    .unwrap();
    let (posts, _) = process_instruction(
        setup.accounts.clone(),
        Instruction::PrepareFixedPrice {
            inputs: setup.inputs.clone(),
        },
        LBP,
    );
    setup.accounts[0].account = posts[0].account().clone();
    setup.accounts[0].account.program_owner = LBP;
    let token_definition = setup.accounts[2].account_id;
    let collateral_definition = setup.accounts[3].account_id;
    holding(&mut setup.accounts[5], token_definition, 0);
    holding(&mut setup.accounts[6], collateral_definition, 0);
    let (posts, _) = process_instruction(setup.accounts, Instruction::ActivateFixedPrice, LBP);

    let state = PoolAccount::try_from(&posts[0].account().data).unwrap();
    assert!(state.funded);
    assert_eq!(state.end_timestamp, None);
}

#[test]
fn buys_accept_an_open_sale_after_the_configured_end_boundary() {
    let mut setup = Setup::new();
    setup.inputs.end_timestamp = None;
    setup.accounts[7].account.data = clock_core::ClockAccountData {
        block_id: 2,
        timestamp: 1_000,
    }
    .to_bytes()
    .try_into()
    .unwrap();
    let (posts, _) = process_instruction(
        setup.accounts.clone(),
        Instruction::PrepareFixedPrice {
            inputs: setup.inputs.clone(),
        },
        LBP,
    );
    setup.accounts[0].account = posts[0].account().clone();
    setup.accounts[0].account.program_owner = LBP;
    let token_definition = setup.accounts[2].account_id;
    holding(&mut setup.accounts[5], token_definition, 0);
    let collateral_definition = setup.accounts[3].account_id;
    holding(&mut setup.accounts[6], collateral_definition, 0);
    let (posts, _) =
        process_instruction(setup.accounts.clone(), Instruction::ActivateFixedPrice, LBP);
    setup.accounts[0].account = posts[0].account().clone();
    setup.accounts[0].account.program_owner = LBP;
    holding(&mut setup.accounts[5], token_definition, 800);

    let (posts, calls) = process_instruction(
        buyer_accounts(&setup, AccountId::new([44; 32]), 100),
        Instruction::BuyExactInput {
            amount_in: 10,
            min_amount_out: 6,
        },
        LBP,
    );

    assert_eq!(calls.len(), 2);
    let state = PoolAccount::try_from(&posts[0].account().data).unwrap();
    assert_eq!(state.end_timestamp, None);
    assert_eq!(state.remaining_inventory, 794);
}

#[test]
fn buys_reject_noncanonical_token_accounts() {
    for instruction in [
        Instruction::BuyExactInput {
            amount_in: 10,
            min_amount_out: 6,
        },
        Instruction::BuyExactOutput {
            amount_out: 6,
            max_amount_in: 10,
        },
    ] {
        for index in 3..7 {
            let setup = active_sale();
            let mut pre = buyer_accounts(&setup, AccountId::new([44; 32]), 100);
            pre[index].account_id = AccountId::new([99; 32]);
            assert!(
                std::panic::catch_unwind(|| { process_instruction(pre, instruction.clone(), LBP) })
                    .is_err()
            );
        }
    }
}

#[test]
fn preparation_saves_a_pending_fixed_price_pool_and_prepares_empty_vaults() {
    for (creator_allocation, release) in [
        (200, CreatorAllocationRelease::AfterSetup),
        (0, CreatorAllocationRelease::AfterSetup),
        (200, CreatorAllocationRelease::AfterEnd),
    ] {
        let mut setup = Setup::new();
        setup.inputs.creator_allocation = creator_allocation;
        setup.inputs.creator_allocation_release = release;
        let factory_owner = setup.accounts[1].account_id;
        let (posts, calls) = process_instruction(
            setup.accounts,
            Instruction::PrepareFixedPrice {
                inputs: setup.inputs,
            },
            LBP,
        );
        let state = PoolAccount::try_from(&posts[0].account().data).unwrap();
        assert_eq!(state.mode, PoolMode::FixedPrice);
        assert!(!state.funded);
        assert_eq!(state.recorded_collateral, 0);
        assert_eq!(state.namespace, AccountId::new([40; 32]));
        assert_eq!(state.namespace_program_id, NAMESPACE_PROGRAM);
        assert_eq!(state.factory_owner, factory_owner);
        assert_eq!(state.factory_program_id, FACTORY);
        assert_eq!(state.factory_seed, [3; 32]);
        assert_eq!(state.token_definition_id, AccountId::new([41; 32]));
        assert_eq!(state.collateral_definition_id, AccountId::new([42; 32]));
        assert_eq!(
            (state.sale_inventory, state.creator_allocation),
            (800, creator_allocation)
        );
        assert_eq!(
            state.price,
            FixedPrice {
                collateral_units: 3,
                token_units: 2
            }
        );
        assert_eq!(
            (state.start_timestamp, state.end_timestamp),
            (100, Some(200))
        );
        assert_eq!(state.creator_allocation_release, release);
        assert_eq!(posts.len(), 9);
        assert_eq!(calls.len(), 2);
        for (call, definition_id) in calls
            .iter()
            .zip([state.token_definition_id, state.collateral_definition_id])
        {
            assert_eq!(
                call.program_id,
                curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID
            );
            assert_eq!(call.pre_states[0].account.program_owner, LBP);
            assert_eq!(
                PoolAccount::try_from(&call.pre_states[0].account.data).unwrap(),
                state
            );
            assert_eq!(call.pre_states[1].account_id, definition_id);
            assert_eq!(call.pre_states[2].account, Account::default());
            let instruction: associated_token_account_core::Instruction =
                risc0_zkvm::serde::from_slice(&call.instruction_data).unwrap();
            assert!(matches!(
                instruction,
                associated_token_account_core::Instruction::Create {
                    ata_program_id: curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                }
            ));
        }
    }
}
