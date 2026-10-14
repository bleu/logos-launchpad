use factory_core::{
    CreatorAllocationRelease, FixedPrice, FixedPriceInputs, FixedPriceLaunchState, Instruction,
    SaleStart, compute_definition_pda, compute_escrow_pda, compute_factory_pda,
    compute_metadata_pda, compute_mint_pda, process_instruction,
};
use lee_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::ProgramId,
};

const FACTORY: ProgramId = [7; 8];
const NAMESPACE_PROGRAM: ProgramId = [6; 8];
const LBP: ProgramId = [8; 8];

fn account(id: AccountId) -> AccountWithMetadata {
    AccountWithMetadata {
        account_id: id,
        account: Account::default(),
        is_authorized: false,
    }
}

fn ata(owner: AccountId, definition: AccountId) -> AccountId {
    associated_token_account_core::get_associated_token_account_id(
        &curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
        &associated_token_account_core::compute_ata_seed(owner, definition),
    )
}

struct Creation {
    inputs: FixedPriceInputs,
    accounts: Vec<AccountWithMetadata>,
}

impl Creation {
    fn prepared() -> Self {
        Self::prepared_with_release(CreatorAllocationRelease::AfterEnd)
    }

    fn prepared_with_release(release: CreatorAllocationRelease) -> Self {
        let mut creation = Self::allocated_with_release(release);
        let (posts, calls) = process_instruction(
            creation.accounts.clone(),
            Instruction::ContinueFixedPriceCreation {
                expected_stage: factory_core::FixedPriceCreationStage::Allocated,
            },
            FACTORY,
        );
        creation.accounts[0].account = posts[0].account().clone();
        let mut pool_pre: Vec<_> = [9, 0, 1, 7, 8, 10, 11, 12, 13]
            .into_iter()
            .map(|index| creation.accounts[index].clone())
            .collect();
        pool_pre[1].is_authorized = true;
        let instruction = risc0_zkvm::serde::from_slice(&calls[0].instruction_data).unwrap();
        let (pool_posts, _) = lbp_core::process_instruction(pool_pre, instruction, LBP);
        creation.accounts[9].account = pool_posts[0].account().clone();
        creation.accounts[9].account.program_owner = LBP;
        for (index, definition) in [(10, 1), (11, 7)] {
            creation.accounts[index].account.program_owner = curve_core::TOKEN_PROGRAM_ID;
            creation.accounts[index].account.data =
                Data::from(&token_core::TokenHolding::Fungible {
                    definition_id: creation.accounts[definition].account_id,
                    balance: 0,
                });
        }
        if release == CreatorAllocationRelease::AfterSetup {
            creation.accounts[6].account.program_owner = curve_core::TOKEN_PROGRAM_ID;
            creation.accounts[6].account.data = Data::from(&token_core::TokenHolding::Fungible {
                definition_id: creation.accounts[1].account_id,
                balance: 0,
            });
        }
        creation
    }

    fn allocated() -> Self {
        Self::allocated_with_release(CreatorAllocationRelease::AfterEnd)
    }

    fn allocated_with_release(release: CreatorAllocationRelease) -> Self {
        let mut creation = Self::minted_with_release(release);
        let (posts, _) = process_instruction(
            creation.accounts.clone(),
            Instruction::ContinueFixedPriceCreation {
                expected_stage: factory_core::FixedPriceCreationStage::Minted,
            },
            FACTORY,
        );
        creation.accounts[0].account = posts[0].account().clone();
        for (index, balance) in [(2, 0), (4, 200), (8, 800)] {
            creation.accounts[index].account.program_owner = curve_core::TOKEN_PROGRAM_ID;
            creation.accounts[index].account.data =
                Data::from(&token_core::TokenHolding::Fungible {
                    definition_id: creation.accounts[1].account_id,
                    balance,
                });
        }
        creation
    }

    fn minted() -> Self {
        Self::minted_with_release(CreatorAllocationRelease::AfterEnd)
    }

    fn minted_with_release(release: CreatorAllocationRelease) -> Self {
        let mut creation = Self::new();
        creation.inputs.allocation_release = release;
        let (posts, _) = process_instruction(
            creation.accounts.clone(),
            Instruction::CreateFixedPriceLbp {
                inputs: creation.inputs.clone(),
            },
            FACTORY,
        );
        creation.accounts[0].account = posts[0].account().clone();
        creation.accounts[0].account.program_owner = FACTORY;
        creation.accounts[1].account.program_owner = curve_core::TOKEN_PROGRAM_ID;
        creation.accounts[1].account.data = Data::from(&token_core::TokenDefinition::Fungible {
            name: "Launch".into(),
            total_supply: 1_000,
            metadata_id: Some(creation.accounts[3].account_id),
        });
        creation.accounts[2].account.program_owner = curve_core::TOKEN_PROGRAM_ID;
        creation.accounts[2].account.data = Data::from(&token_core::TokenHolding::Fungible {
            definition_id: creation.accounts[1].account_id,
            balance: 1_000,
        });
        creation.accounts[3].account.program_owner = curve_core::TOKEN_PROGRAM_ID;
        creation
    }

    fn new() -> Self {
        let namespace = AccountId::new([40; 32]);
        let salt = [3; 32];
        let factory = compute_factory_pda(namespace, FACTORY, salt);
        let definition = compute_definition_pda(namespace, FACTORY, salt);
        let collateral = AccountId::new([41; 32]);
        let creator = AccountId::new([42; 32]);
        let pool = curve_core::compute_pool_pda(namespace, LBP, definition, collateral, factory);
        let mut accounts = vec![
            account(factory),
            account(definition),
            account(compute_mint_pda(namespace, FACTORY, salt)),
            account(compute_metadata_pda(namespace, FACTORY, salt)),
            account(compute_escrow_pda(namespace, FACTORY, salt)),
            account(creator),
            account(ata(creator, definition)),
            account(collateral),
            account(ata(factory, definition)),
            account(pool),
            account(ata(pool, definition)),
            account(ata(pool, collateral)),
            account(clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID),
            account(curve_core::compute_config_pda(namespace, NAMESPACE_PROGRAM)),
        ];
        accounts[5].is_authorized = true;
        accounts[7].account.program_owner = curve_core::TOKEN_PROGRAM_ID;
        accounts[7].account.data = Data::from(&token_core::TokenDefinition::Fungible {
            name: "Collateral".into(),
            total_supply: 10_000,
            metadata_id: None,
        });
        accounts[12].account.data = clock_core::ClockAccountData {
            block_id: 1,
            timestamp: 100,
        }
        .to_bytes()
        .try_into()
        .unwrap();
        accounts[13].account.program_owner = NAMESPACE_PROGRAM;
        accounts[13].account.data = Data::from(&curve_core::Config::new(
            namespace,
            100,
            AccountId::new([43; 32]),
        ));
        Self {
            inputs: FixedPriceInputs {
                namespace,
                launch_salt: salt,
                name: "Launch".into(),
                uri: "https://example.test/launch.json".into(),
                sale_inventory: 800,
                creator_allocation: 200,
                price: FixedPrice {
                    collateral_units: 3,
                    token_units: 2,
                },
                start: SaleStart::Now,
                end_timestamp: 200,
                allocation_release: CreatorAllocationRelease::AfterEnd,
                namespace_program_id: NAMESPACE_PROGRAM,
                lbp_program_id: LBP,
            },
            accounts,
        }
    }
}

#[test]
fn creation_rejects_a_factory_account_outside_the_namespace_and_salt() {
    let mut creation = Creation::new();
    creation.accounts[0].account_id = AccountId::new([99; 32]);
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            creation.accounts,
            Instruction::CreateFixedPriceLbp {
                inputs: creation.inputs
            },
            FACTORY,
        ))
        .is_err()
    );
}

#[test]
fn creation_cannot_reuse_an_existing_launch_account() {
    let mut creation = Creation::new();
    creation.accounts[0].account.program_owner = FACTORY;
    creation.accounts[0].account.data = vec![1].try_into().unwrap();
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            creation.accounts,
            Instruction::CreateFixedPriceLbp {
                inputs: creation.inputs
            },
            FACTORY,
        ))
        .is_err()
    );
}

#[test]
fn creation_rejects_a_config_from_another_namespace() {
    let mut creation = Creation::new();
    creation.accounts[13].account_id =
        curve_core::compute_config_pda(AccountId::new([55; 32]), NAMESPACE_PROGRAM);
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            creation.accounts,
            Instruction::CreateFixedPriceLbp {
                inputs: creation.inputs
            },
            FACTORY,
        ))
        .is_err()
    );
}

#[test]
fn creation_requires_positive_sale_inventory() {
    let mut creation = Creation::new();
    creation.inputs.sale_inventory = 0;
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            creation.accounts,
            Instruction::CreateFixedPriceLbp {
                inputs: creation.inputs
            },
            FACTORY,
        ))
        .is_err()
    );
}

#[test]
fn creation_requires_a_positive_exact_price_ratio() {
    for price in [
        FixedPrice {
            collateral_units: 0,
            token_units: 2,
        },
        FixedPrice {
            collateral_units: 3,
            token_units: 0,
        },
    ] {
        let mut creation = Creation::new();
        creation.inputs.price = price;
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                creation.accounts,
                Instruction::CreateFixedPriceLbp {
                    inputs: creation.inputs
                },
                FACTORY,
            ))
            .is_err(),
            "accepted {price:?}"
        );
    }
}

#[test]
fn initial_creation_rejects_a_scheduled_start_in_the_past() {
    let mut creation = Creation::new();
    creation.inputs.start = SaleStart::At { timestamp: 99 };
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            creation.accounts,
            Instruction::CreateFixedPriceLbp {
                inputs: creation.inputs
            },
            FACTORY,
        ))
        .is_err()
    );
}

#[test]
fn creation_requires_end_strictly_after_the_resolved_start() {
    for (start, end) in [
        (SaleStart::Now, 100),
        (SaleStart::Now, 99),
        (SaleStart::At { timestamp: 150 }, 150),
        (SaleStart::At { timestamp: 150 }, 140),
    ] {
        let mut creation = Creation::new();
        creation.inputs.start = start;
        creation.inputs.end_timestamp = end;
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                creation.accounts,
                Instruction::CreateFixedPriceLbp {
                    inputs: creation.inputs
                },
                FACTORY,
            ))
            .is_err()
        );
    }
}

#[test]
fn creation_rejects_substituted_setup_custody_accounts() {
    for index in [1, 2, 3, 4, 6, 8, 9, 10, 11] {
        let mut creation = Creation::new();
        creation.accounts[index].account_id = AccountId::new([99; 32]);
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                creation.accounts,
                Instruction::CreateFixedPriceLbp {
                    inputs: creation.inputs
                },
                FACTORY,
            ))
            .is_err(),
            "accepted substituted account {index}"
        );
    }
}

#[test]
fn creation_cannot_import_existing_token_or_sale_custody() {
    for index in [1, 2, 3, 4, 8, 9, 10, 11] {
        let mut creation = Creation::new();
        creation.accounts[index].account.data = vec![1].try_into().unwrap();
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                creation.accounts,
                Instruction::CreateFixedPriceLbp {
                    inputs: creation.inputs
                },
                FACTORY,
            ))
            .is_err(),
            "accepted existing account {index}"
        );
    }
}

#[test]
fn creation_requires_a_real_fungible_collateral_definition() {
    for invalid_owner in [false, true] {
        let mut creation = Creation::new();
        if invalid_owner {
            creation.accounts[7].account.program_owner = [99; 8];
        } else {
            creation.accounts[7].account.data = Data::default();
        }
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                creation.accounts,
                Instruction::CreateFixedPriceLbp {
                    inputs: creation.inputs
                },
                FACTORY,
            ))
            .is_err()
        );
    }
}

#[test]
fn creation_requires_token_name_and_metadata_uri() {
    for empty_name in [false, true] {
        let mut creation = Creation::new();
        if empty_name {
            creation.inputs.name.clear();
        } else {
            creation.inputs.uri.clear();
        }
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                creation.accounts,
                Instruction::CreateFixedPriceLbp {
                    inputs: creation.inputs
                },
                FACTORY,
            ))
            .is_err()
        );
    }
}

#[test]
fn creation_rejects_an_empty_namespace_identity() {
    let mut creation = Creation::new();
    creation.inputs.namespace = AccountId::default();
    let ns = creation.inputs.namespace;
    creation.accounts[0].account_id = compute_factory_pda(ns, FACTORY, creation.inputs.launch_salt);
    creation.accounts[1].account_id =
        compute_definition_pda(ns, FACTORY, creation.inputs.launch_salt);
    creation.accounts[2].account_id = compute_mint_pda(ns, FACTORY, creation.inputs.launch_salt);
    creation.accounts[3].account_id =
        compute_metadata_pda(ns, FACTORY, creation.inputs.launch_salt);
    creation.accounts[4].account_id = compute_escrow_pda(ns, FACTORY, creation.inputs.launch_salt);
    let definition = creation.accounts[1].account_id;
    let factory = creation.accounts[0].account_id;
    creation.accounts[6].account_id = ata(creation.accounts[5].account_id, definition);
    creation.accounts[8].account_id = ata(factory, definition);
    let pool = curve_core::compute_pool_pda(
        ns,
        LBP,
        definition,
        creation.accounts[7].account_id,
        factory,
    );
    creation.accounts[9].account_id = pool;
    creation.accounts[10].account_id = ata(pool, definition);
    creation.accounts[11].account_id = ata(pool, creation.accounts[7].account_id);
    creation.accounts[13].account_id = curve_core::compute_config_pda(ns, NAMESPACE_PROGRAM);
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            creation.accounts,
            Instruction::CreateFixedPriceLbp {
                inputs: creation.inputs
            },
            FACTORY,
        ))
        .is_err()
    );
}

#[test]
fn creation_records_fixed_price_mode_and_emits_only_inventory_plus_allocation() {
    let creation = Creation::new();
    let (posts, calls) = process_instruction(
        creation.accounts,
        Instruction::CreateFixedPriceLbp {
            inputs: creation.inputs,
        },
        FACTORY,
    );
    let state = FixedPriceLaunchState::try_from(&posts[0].account().data).unwrap();
    assert_eq!(state.mode, lbp_core::PoolMode::FixedPrice);
    let mint: token_core::Instruction =
        risc0_zkvm::serde::from_slice(&calls[0].instruction_data).unwrap();
    let token_core::Instruction::NewDefinitionWithMetadata {
        new_definition: token_core::NewTokenDefinition::Fungible { name, total_supply },
        ..
    } = mint
    else {
        panic!("expected new fungible token definition")
    };
    assert_eq!(name, "Launch");
    assert_eq!(total_supply, 1_000);
}

#[test]
fn creation_cannot_overflow_total_supply() {
    let mut creation = Creation::new();
    creation.inputs.sale_inventory = u128::MAX;
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            creation.accounts,
            Instruction::CreateFixedPriceLbp {
                inputs: creation.inputs
            },
            FACTORY,
        ))
        .is_err()
    );
}

#[test]
fn creation_requires_the_creator_signature() {
    let mut creation = Creation::new();
    creation.accounts[5].is_authorized = false;
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            creation.accounts,
            Instruction::CreateFixedPriceLbp {
                inputs: creation.inputs
            },
            FACTORY,
        ))
        .is_err()
    );
}

#[test]
fn scheduled_creation_requires_the_canonical_clock() {
    let mut creation = Creation::new();
    creation.inputs.start = SaleStart::At { timestamp: 150 };
    creation.accounts[12].account_id = AccountId::new([99; 32]);
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            creation.accounts,
            Instruction::CreateFixedPriceLbp {
                inputs: creation.inputs
            },
            FACTORY,
        ))
        .is_err()
    );
}

#[test]
fn creator_can_split_minted_inventory_and_locked_allocation() {
    let creation = Creation::minted();
    let (posts, calls) = process_instruction(
        creation.accounts,
        Instruction::ContinueFixedPriceCreation {
            expected_stage: factory_core::FixedPriceCreationStage::Minted,
        },
        FACTORY,
    );
    let state = FixedPriceLaunchState::try_from(&posts[0].account().data).unwrap();
    assert_eq!(
        state.creation_stage,
        factory_core::FixedPriceCreationStage::Allocated
    );
    assert!(!state.creator_allocation_released);
    let amounts: Vec<u128> = calls
        .iter()
        .filter_map(|call| {
            if call.program_id != curve_core::TOKEN_PROGRAM_ID {
                return None;
            }
            let instruction: token_core::Instruction =
                risc0_zkvm::serde::from_slice(&call.instruction_data).unwrap();
            match instruction {
                token_core::Instruction::Transfer { amount_to_transfer } => {
                    Some(amount_to_transfer)
                }
                _ => None,
            }
        })
        .collect();
    assert_eq!(amounts, [800, 200]);
}

#[test]
fn continuation_rejects_substituted_setup_custody_accounts() {
    for index in [1, 2, 3, 4, 6, 7, 8, 9, 10, 11, 13] {
        let mut creation = Creation::minted();
        creation.accounts[index].account_id = AccountId::new([99; 32]);
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                creation.accounts,
                Instruction::ContinueFixedPriceCreation {
                    expected_stage: factory_core::FixedPriceCreationStage::Minted,
                },
                FACTORY,
            ))
            .is_err(),
            "accepted substituted account {index}"
        );
    }
}

#[test]
fn allocation_stage_requires_intact_mint_and_empty_destinations() {
    for case in 0..5 {
        let mut creation = Creation::minted();
        match case {
            0 => creation.accounts[2].account.program_owner = [99; 8],
            1 => {
                creation.accounts[2].account.data =
                    Data::from(&token_core::TokenHolding::Fungible {
                        definition_id: AccountId::new([99; 32]),
                        balance: 1_000,
                    })
            }
            2 => {
                creation.accounts[2].account.data =
                    Data::from(&token_core::TokenHolding::Fungible {
                        definition_id: creation.accounts[1].account_id,
                        balance: 999,
                    })
            }
            3 => creation.accounts[4].account.data = vec![1].try_into().unwrap(),
            4 => {
                creation.accounts[8].account.program_owner = curve_core::TOKEN_PROGRAM_ID;
                creation.accounts[8].account.data =
                    Data::from(&token_core::TokenHolding::Fungible {
                        definition_id: creation.accounts[1].account_id,
                        balance: 1,
                    });
            }
            _ => unreachable!(),
        }
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                creation.accounts,
                Instruction::ContinueFixedPriceCreation {
                    expected_stage: factory_core::FixedPriceCreationStage::Minted,
                },
                FACTORY,
            ))
            .is_err(),
            "accepted invalid mint or destination case {case}"
        );
    }
}

#[test]
fn late_pool_preparation_uses_the_saved_price_and_schedule() {
    let mut creation = Creation::allocated();
    creation.accounts[12].account.data = clock_core::ClockAccountData {
        block_id: 2,
        timestamp: 120,
    }
    .to_bytes()
    .try_into()
    .unwrap();
    let (posts, calls) = process_instruction(
        creation.accounts,
        Instruction::ContinueFixedPriceCreation {
            expected_stage: factory_core::FixedPriceCreationStage::Allocated,
        },
        FACTORY,
    );
    let state = FixedPriceLaunchState::try_from(&posts[0].account().data).unwrap();
    assert_eq!(
        state.creation_stage,
        factory_core::FixedPriceCreationStage::PoolPrepared
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].program_id, LBP);
    let instruction: lbp_core::Instruction =
        risc0_zkvm::serde::from_slice(&calls[0].instruction_data).unwrap();
    let lbp_core::Instruction::PrepareFixedPrice { inputs } = instruction else {
        panic!("expected pool preparation")
    };
    assert_eq!((inputs.start_timestamp, inputs.end_timestamp), (100, 200));
    assert_eq!(
        inputs.price,
        FixedPrice {
            collateral_units: 3,
            token_units: 2
        }
    );
    assert_eq!(
        (inputs.sale_inventory, inputs.creator_allocation),
        (800, 200)
    );
}

#[test]
fn immediate_allocation_prepares_the_creator_ata_without_paying() {
    let creation = Creation::allocated_with_release(CreatorAllocationRelease::AfterSetup);
    let (posts, calls) = process_instruction(
        creation.accounts,
        Instruction::ContinueFixedPriceCreation {
            expected_stage: factory_core::FixedPriceCreationStage::Allocated,
        },
        FACTORY,
    );
    let state = FixedPriceLaunchState::try_from(&posts[0].account().data).unwrap();
    assert!(!state.creator_allocation_released);
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[1].program_id,
        curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID
    );
    let instruction: associated_token_account_core::Instruction =
        risc0_zkvm::serde::from_slice(&calls[1].instruction_data).unwrap();
    assert!(matches!(
        instruction,
        associated_token_account_core::Instruction::Create { .. }
    ));
}

#[test]
fn final_setup_funds_the_pool_and_keeps_delayed_allocation_locked() {
    let creation = Creation::prepared();
    let (posts, calls) = process_instruction(
        creation.accounts,
        Instruction::ContinueFixedPriceCreation {
            expected_stage: factory_core::FixedPriceCreationStage::PoolPrepared,
        },
        FACTORY,
    );
    let state = FixedPriceLaunchState::try_from(&posts[0].account().data).unwrap();
    assert_eq!(
        state.creation_stage,
        factory_core::FixedPriceCreationStage::Complete
    );
    assert!(!state.creator_allocation_released);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].program_id, LBP);
    let instruction: lbp_core::Instruction =
        risc0_zkvm::serde::from_slice(&calls[0].instruction_data).unwrap();
    assert!(matches!(
        instruction,
        lbp_core::Instruction::ActivateFixedPrice
    ));
}

#[test]
fn final_setup_pays_immediate_allocation_with_inventory_funding() {
    let creation = Creation::prepared_with_release(CreatorAllocationRelease::AfterSetup);
    let (posts, calls) = process_instruction(
        creation.accounts,
        Instruction::ContinueFixedPriceCreation {
            expected_stage: factory_core::FixedPriceCreationStage::PoolPrepared,
        },
        FACTORY,
    );
    let state = FixedPriceLaunchState::try_from(&posts[0].account().data).unwrap();
    assert_eq!(
        state.creation_stage,
        factory_core::FixedPriceCreationStage::Complete
    );
    assert!(state.creator_allocation_released);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].program_id, LBP);
    assert_eq!(calls[1].program_id, curve_core::TOKEN_PROGRAM_ID);
    let instruction: token_core::Instruction =
        risc0_zkvm::serde::from_slice(&calls[1].instruction_data).unwrap();
    assert!(matches!(
        instruction,
        token_core::Instruction::Transfer {
            amount_to_transfer: 200
        }
    ));
}

#[test]
fn continuation_requires_trusted_time_before_the_original_end() {
    for stage in [
        factory_core::FixedPriceCreationStage::Minted,
        factory_core::FixedPriceCreationStage::Allocated,
        factory_core::FixedPriceCreationStage::PoolPrepared,
    ] {
        for timestamp in [200, 201, 120] {
            let mut creation = match stage {
                factory_core::FixedPriceCreationStage::Minted => Creation::minted(),
                factory_core::FixedPriceCreationStage::Allocated => Creation::allocated(),
                _ => Creation::prepared(),
            };
            creation.accounts[12].account.data = clock_core::ClockAccountData {
                block_id: 2,
                timestamp,
            }
            .to_bytes()
            .try_into()
            .unwrap();
            if timestamp == 120 {
                creation.accounts[12].account_id = AccountId::new([99; 32]);
            }
            assert!(
                std::panic::catch_unwind(|| process_instruction(
                    creation.accounts,
                    Instruction::ContinueFixedPriceCreation {
                        expected_stage: stage
                    },
                    FACTORY,
                ))
                .is_err(),
                "accepted time {timestamp} at {stage:?}"
            );
        }
    }
}

#[test]
fn continuation_requires_the_original_fixed_supply_definition() {
    for stage in [
        factory_core::FixedPriceCreationStage::Minted,
        factory_core::FixedPriceCreationStage::Allocated,
        factory_core::FixedPriceCreationStage::PoolPrepared,
    ] {
        for case in 0..4 {
            let mut creation = match stage {
                factory_core::FixedPriceCreationStage::Minted => Creation::minted(),
                factory_core::FixedPriceCreationStage::Allocated => Creation::allocated(),
                _ => Creation::prepared(),
            };
            match case {
                0 => creation.accounts[1].account.program_owner = [99; 8],
                1 => creation.accounts[1].account.data = Data::default(),
                2 | 3 => {
                    creation.accounts[1].account.data =
                        Data::from(&token_core::TokenDefinition::Fungible {
                            name: "Launch".into(),
                            total_supply: if case == 2 { 999 } else { 1_000 },
                            metadata_id: if case == 3 {
                                None
                            } else {
                                Some(creation.accounts[3].account_id)
                            },
                        })
                }
                _ => unreachable!(),
            }
            assert!(
                std::panic::catch_unwind(|| process_instruction(
                    creation.accounts,
                    Instruction::ContinueFixedPriceCreation {
                        expected_stage: stage
                    },
                    FACTORY,
                ))
                .is_err(),
                "accepted definition case {case} at {stage:?}"
            );
        }
    }
}

#[test]
fn later_setup_requires_intact_inventory_and_locked_allocation() {
    for stage in [
        factory_core::FixedPriceCreationStage::Allocated,
        factory_core::FixedPriceCreationStage::PoolPrepared,
    ] {
        for (index, balance) in [(2, 1), (4, 199), (8, 799)] {
            for wrong_owner in [false, true] {
                let mut creation = if stage == factory_core::FixedPriceCreationStage::Allocated {
                    Creation::allocated()
                } else {
                    Creation::prepared()
                };
                if wrong_owner {
                    creation.accounts[index].account.program_owner = [99; 8];
                } else {
                    creation.accounts[index].account.data =
                        Data::from(&token_core::TokenHolding::Fungible {
                            definition_id: creation.accounts[1].account_id,
                            balance,
                        });
                }
                assert!(
                    std::panic::catch_unwind(|| process_instruction(
                        creation.accounts,
                        Instruction::ContinueFixedPriceCreation {
                            expected_stage: stage
                        },
                        FACTORY,
                    ))
                    .is_err(),
                    "accepted custody {index} at {stage:?}"
                );
            }
        }
    }
}

#[test]
fn continuation_requires_the_saved_fungible_collateral_definition() {
    for stage in [
        factory_core::FixedPriceCreationStage::Minted,
        factory_core::FixedPriceCreationStage::Allocated,
        factory_core::FixedPriceCreationStage::PoolPrepared,
    ] {
        for wrong_owner in [false, true] {
            let mut creation = match stage {
                factory_core::FixedPriceCreationStage::Minted => Creation::minted(),
                factory_core::FixedPriceCreationStage::Allocated => Creation::allocated(),
                _ => Creation::prepared(),
            };
            if wrong_owner {
                creation.accounts[7].account.program_owner = [99; 8];
            } else {
                creation.accounts[7].account.data = Data::default();
            }
            assert!(
                std::panic::catch_unwind(|| process_instruction(
                    creation.accounts,
                    Instruction::ContinueFixedPriceCreation {
                        expected_stage: stage
                    },
                    FACTORY,
                ))
                .is_err(),
                "accepted collateral at {stage:?}"
            );
        }
    }
}

#[test]
fn every_continuation_requires_the_sale_creator_role() {
    for stage in [
        factory_core::FixedPriceCreationStage::Minted,
        factory_core::FixedPriceCreationStage::Allocated,
        factory_core::FixedPriceCreationStage::PoolPrepared,
    ] {
        for unsigned in [false, true] {
            let mut creation = match stage {
                factory_core::FixedPriceCreationStage::Minted => Creation::minted(),
                factory_core::FixedPriceCreationStage::Allocated => Creation::allocated(),
                _ => Creation::prepared(),
            };
            if unsigned {
                creation.accounts[5].is_authorized = false;
            } else {
                creation.accounts[5].account_id = creation.inputs.namespace;
            }
            let error = std::panic::catch_unwind(|| {
                process_instruction(
                    creation.accounts,
                    Instruction::ContinueFixedPriceCreation {
                        expected_stage: stage,
                    },
                    FACTORY,
                )
            })
            .unwrap_err();
            let message = error.downcast_ref::<String>().unwrap();
            assert!(message.contains("Sale creator authorization failed"));
        }
    }
}

#[test]
fn fixed_price_setup_rejects_private_wrappers() {
    for instruction in [
        Instruction::CreateFixedPriceLbp {
            inputs: Creation::new().inputs,
        },
        Instruction::ContinueFixedPriceCreation {
            expected_stage: factory_core::FixedPriceCreationStage::Minted,
        },
    ] {
        let error = std::panic::catch_unwind(|| {
            process_instruction(
                vec![],
                Instruction::Private {
                    instruction: Box::new(instruction),
                },
                FACTORY,
            )
        })
        .unwrap_err();
        let message = error.downcast_ref::<&str>().unwrap();
        assert!(message.contains("Fixed-price creation requires public execution"));
    }
}

#[test]
fn creator_can_create_a_namespace_bound_fixed_price_sale() {
    let creation = Creation::new();
    let creator = creation.accounts[5].clone();
    let (posts, _) = process_instruction(
        creation.accounts,
        Instruction::CreateFixedPriceLbp {
            inputs: creation.inputs,
        },
        FACTORY,
    );
    let state = FixedPriceLaunchState::try_from(&posts[0].account().data).unwrap();
    assert_eq!(state.namespace, AccountId::new([40; 32]));
    assert_eq!(state.total_supply, 1_000);
    assert_eq!(state.sale_inventory, 800);
    assert_eq!(state.creator_allocation, 200);
    assert_eq!(
        state.price,
        FixedPrice {
            collateral_units: 3,
            token_units: 2
        }
    );
    assert_eq!((state.start_timestamp, state.end_timestamp), (100, 200));
    assert_eq!(state.allocation_release, CreatorAllocationRelease::AfterEnd);
    state.assert_creator(&creator);
}
