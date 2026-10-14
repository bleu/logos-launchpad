//! Signed local fixed-price setup with real token custody.

use std::{collections::HashMap, path::PathBuf};

#[path = "support/fixed_price_runtime.rs"]
mod fixed_price_runtime;
use fixed_price_runtime::Diagnostic;

use factory_core::{
    CreatorAllocationRelease, FixedPrice, FixedPriceCreationStage, FixedPriceInputs,
    FixedPriceLaunchState, Instruction, SaleStart,
};
use lee::{PrivateKey, PublicKey, program::Program};
use lee_core::{
    account::{Account, AccountId, Data},
    program::ProgramId,
};
use token_core::TokenHolding;

fn guest(name: &str) -> Program {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../methods/target/riscv-guest/launchpad_methods/launchpad_programs/riscv32im-risc0-zkvm-elf/release");
    launchpad_client::load_program(&root.join(format!("{name}.bin"))).unwrap()
}

fn signer(n: u8) -> AccountId {
    AccountId::from(&PublicKey::new_from_private_key(
        &PrivateKey::try_new([n; 32]).unwrap(),
    ))
}

fn ata(owner: AccountId, definition: AccountId) -> AccountId {
    associated_token_account_core::get_associated_token_account_id(
        &programs::ata().id(),
        &associated_token_account_core::compute_ata_seed(owner, definition),
    )
}

fn assert_program_error(error: lee::error::LeeError, expected: &str) {
    assert!(
        matches!(&error, lee::error::LeeError::ProgramExecutionFailed(message) if message.contains(expected)),
        "{error:?}"
    );
    println!("EXPECTED failure: {error:?}");
}

struct Sale {
    factory: Program,
    curve: Program,
    lbp: Program,
    accounts: HashMap<AccountId, Account>,
    ids: Vec<AccountId>,
    inputs: FixedPriceInputs,
    timestamp: u64,
}

impl Sale {
    fn new() -> Self {
        let factory = guest("factory");
        let curve = guest("curve");
        let lbp = guest("lbp");
        let namespace = signer(3);
        let creator = signer(20);
        let salt = [31; 32];
        let owner = factory_core::compute_factory_pda(namespace, factory.id(), salt);
        let definition = factory_core::compute_definition_pda(namespace, factory.id(), salt);
        let collateral = AccountId::new([41; 32]);
        let pool = curve_core::compute_pool_pda(namespace, lbp.id(), definition, collateral, owner);
        let ids = vec![
            owner,
            definition,
            factory_core::compute_mint_pda(namespace, factory.id(), salt),
            factory_core::compute_metadata_pda(namespace, factory.id(), salt),
            factory_core::compute_escrow_pda(namespace, factory.id(), salt),
            creator,
            ata(creator, definition),
            collateral,
            ata(owner, definition),
            pool,
            ata(pool, definition),
            ata(pool, collateral),
            clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
            curve_core::compute_config_pda(namespace, curve.id()),
        ];
        let mut accounts = HashMap::from([
            (
                namespace,
                Account {
                    program_owner: [77; 8],
                    ..Account::default()
                },
            ),
            (
                creator,
                Account {
                    program_owner: [77; 8],
                    ..Account::default()
                },
            ),
            (
                collateral,
                Account {
                    program_owner: programs::token().id(),
                    data: Data::from(&token_core::TokenDefinition::Fungible {
                        name: "Collateral".into(),
                        total_supply: 10_000,
                        metadata_id: None,
                    }),
                    ..Account::default()
                },
            ),
            (
                clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
                Account {
                    program_owner: [88; 8],
                    data: clock_core::ClockAccountData {
                        block_id: 1,
                        timestamp: 100,
                    }
                    .to_bytes()
                    .try_into()
                    .unwrap(),
                    ..Account::default()
                },
            ),
        ]);
        accounts.insert(ids[13], Account::default());
        let inputs = FixedPriceInputs {
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
            namespace_program_id: curve.id(),
            lbp_program_id: lbp.id(),
        };
        let mut sale = Self {
            factory,
            curve,
            lbp,
            accounts,
            ids,
            inputs,
            timestamp: 100,
        };
        sale.execute(
            sale.curve.id(),
            vec![sale.ids[13], namespace],
            &[3],
            curve_core::Instruction::InitializeNamespace {
                namespace,
                protocol_fee_bps: 100,
                treasury: AccountId::new([43; 32]),
            },
        )
        .unwrap();
        sale
    }

    fn execute<I: serde::Serialize>(
        &mut self,
        program: ProgramId,
        ids: Vec<AccountId>,
        keys: &[u8],
        instruction: I,
    ) -> Result<(), lee::error::LeeError> {
        let mut state = lee::V03State::new()
            .with_public_accounts(self.accounts.clone())
            .with_programs([
                self.factory.clone(),
                self.curve.clone(),
                self.lbp.clone(),
                programs::token(),
                programs::ata(),
            ]);
        let signing_keys: Vec<_> = keys
            .iter()
            .map(|n| PrivateKey::try_new([*n; 32]).unwrap())
            .collect();
        let nonces = keys
            .iter()
            .map(|n| state.get_account_by_id(signer(*n)).nonce)
            .collect();
        let message =
            lee::public_transaction::Message::try_new(program, ids.clone(), nonces, instruction)
                .unwrap();
        let witnesses = lee::public_transaction::WitnessSet::for_message(
            &message,
            &signing_keys.iter().collect::<Vec<_>>(),
        );
        if let Err(error) = state.transition_from_public_transaction(
            &lee::PublicTransaction::new(message, witnesses),
            1,
            self.timestamp,
        ) {
            for id in self.accounts.keys().chain(ids.iter()) {
                assert_eq!(
                    state.get_account_by_id(*id),
                    self.accounts.get(id).cloned().unwrap_or_default(),
                    "failed transaction changed account {id}"
                );
            }
            return Err(error);
        }
        for id in ids {
            self.accounts.insert(id, state.get_account_by_id(id));
        }
        Ok(())
    }

    fn create(&mut self) -> Result<(), lee::error::LeeError> {
        self.execute(
            self.factory.id(),
            self.ids.clone(),
            &[20],
            Instruction::CreateFixedPriceLbp {
                inputs: self.inputs.clone(),
            },
        )
    }

    fn continue_creation(
        &mut self,
        expected_stage: FixedPriceCreationStage,
    ) -> Result<(), lee::error::LeeError> {
        self.execute(
            self.factory.id(),
            self.ids.clone(),
            &[20],
            Instruction::ContinueFixedPriceCreation { expected_stage },
        )
    }

    fn launch(&self) -> FixedPriceLaunchState {
        FixedPriceLaunchState::try_from(&self.accounts[&self.ids[0]].data).unwrap()
    }

    fn set_time(&mut self, timestamp: u64) {
        self.timestamp = timestamp;
        self.accounts
            .get_mut(&clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID)
            .unwrap()
            .data = clock_core::ClockAccountData {
            block_id: 1,
            timestamp,
        }
        .to_bytes()
        .try_into()
        .unwrap();
    }

    fn pool(&self) -> lbp_core::PoolAccount {
        lbp_core::PoolAccount::try_from(&self.accounts[&self.ids[9]].data).unwrap()
    }

    fn balance(&self, index: usize) -> u128 {
        match TokenHolding::try_from(&self.accounts[&self.ids[index]].data).unwrap() {
            TokenHolding::Fungible { balance, .. } => balance,
            _ => panic!("expected fungible holding"),
        }
    }
}

fn stage_diagnostic(sale: &Sale, instruction: Instruction) -> Diagnostic {
    let key = PrivateKey::try_new([20; 32]).unwrap();
    let message = lee::public_transaction::Message::try_new(
        sale.factory.id(),
        sale.ids.clone(),
        vec![sale.accounts[&signer(20)].nonce],
        instruction,
    )
    .unwrap();
    let witnesses = lee::public_transaction::WitnessSet::for_message(&message, &[&key]);
    fixed_price_runtime::diagnostic(
        &[
            sale.factory.clone(),
            sale.curve.clone(),
            sale.lbp.clone(),
            programs::token(),
            programs::ata(),
        ],
        &sale.accounts,
        lee::PublicTransaction::new(message, witnesses),
        sale.timestamp,
        11,
    )
}

#[test]
fn signed_stages_fit_public_limits_with_measured_real_children() {
    for allocation in [0, 200] {
        for release in [
            CreatorAllocationRelease::AfterSetup,
            CreatorAllocationRelease::AfterEnd,
        ] {
            let mut sale = Sale::new();
            sale.inputs.creator_allocation = allocation;
            sale.inputs.allocation_release = release;
            println!(
                "STAGE_IMAGES factory={:?} lbp={:?}",
                sale.factory.id(),
                sale.lbp.id()
            );
            for (label, instruction, expected_executions) in [
                (
                    "mint",
                    Instruction::CreateFixedPriceLbp {
                        inputs: sale.inputs.clone(),
                    },
                    2,
                ),
                (
                    "allocate",
                    Instruction::ContinueFixedPriceCreation {
                        expected_stage: FixedPriceCreationStage::Minted,
                    },
                    if allocation == 0 { 4 } else { 5 },
                ),
                (
                    "prepare",
                    Instruction::ContinueFixedPriceCreation {
                        expected_stage: FixedPriceCreationStage::Allocated,
                    },
                    if allocation != 0 && release == CreatorAllocationRelease::AfterSetup {
                        8
                    } else {
                        6
                    },
                ),
                (
                    "activate",
                    Instruction::ContinueFixedPriceCreation {
                        expected_stage: FixedPriceCreationStage::PoolPrepared,
                    },
                    if allocation != 0 && release == CreatorAllocationRelease::AfterSetup {
                        5
                    } else {
                        4
                    },
                ),
            ] {
                let diagnostic = stage_diagnostic(&sale, instruction.clone());
                assert!(diagnostic.failure.is_none(), "{diagnostic:?}");
                assert_eq!(diagnostic.invocations.len(), expected_executions);
                assert!(
                    diagnostic
                        .invocations
                        .iter()
                        .all(|call| call.cycles < 32 * 1024 * 1024)
                );
                let cycles: Vec<_> = diagnostic
                    .invocations
                    .iter()
                    .map(|call| call.cycles)
                    .collect();
                let root_emitted = diagnostic.invocations[0].emitted;
                let nested: usize = diagnostic
                    .invocations
                    .iter()
                    .skip(1)
                    .map(|call| call.emitted)
                    .sum();
                println!(
                    "STAGE_DIAGNOSTIC stage={label} allocation={allocation} release={release:?} root_emitted={root_emitted} nested={nested} executions={} chained={} cycles={cycles:?} max_cycles={} aggregate_cycles={}",
                    cycles.len(),
                    cycles.len() - 1,
                    cycles.iter().max().unwrap(),
                    cycles.iter().sum::<u64>()
                );
                sale.execute(sale.factory.id(), sale.ids.clone(), &[20], instruction)
                    .unwrap();
                for (id, expected) in diagnostic.accounts {
                    let mut committed = sale.accounts[&id].clone();
                    if id == signer(20) {
                        assert_eq!(committed.nonce.0, expected.nonce.0 + 1);
                        committed.nonce = expected.nonce;
                    }
                    assert_eq!(
                        committed, expected,
                        "signed commit differs from diagnostic account {id}"
                    );
                }
                println!(
                    "STAGE_PUBLIC stage={label} allocation={allocation} release={release:?} committed=true"
                );
            }
            assert_eq!(
                sale.launch().creation_stage,
                FixedPriceCreationStage::Complete
            );
            assert!(sale.pool().funded);
            assert_eq!(sale.balance(10), 800);
            assert_eq!(sale.balance(11), 0);
            assert_eq!(sale.balance(8), 0);
            assert_eq!(sale.pool().recorded_collateral, 0);
        }
    }
}

#[test]
fn signed_creation_mints_the_exact_split_and_claims_creator_authority() {
    let mut sale = Sale::new();
    sale.create().unwrap();
    assert_eq!(sale.balance(2), 1_000);
    assert_eq!(sale.accounts[&sale.ids[0]].program_owner, sale.factory.id());
    assert_eq!(
        token_core::TokenDefinition::try_from(&sale.accounts[&sale.ids[1]].data).unwrap(),
        token_core::TokenDefinition::Fungible {
            name: sale.inputs.name.clone(),
            total_supply: 1_000,
            metadata_id: Some(sale.ids[3]),
        }
    );
    let metadata = token_core::TokenMetadata::try_from(&sale.accounts[&sale.ids[3]].data).unwrap();
    assert_eq!(metadata.definition_id, sale.ids[1]);
    assert_eq!(metadata.uri, sale.inputs.uri);
    let launch = sale.launch();
    assert_eq!(launch.creation_stage, FixedPriceCreationStage::Minted);
    assert_eq!((launch.start_timestamp, launch.end_timestamp), (100, 200));
    assert_eq!(launch.total_supply, 1_000);
    assert_eq!(launch.sale_inventory, 800);
    assert_eq!(launch.creator_allocation, 200);
    launch.assert_creator(&lee_core::account::AccountWithMetadata::new(
        sale.accounts[&sale.ids[5]].clone(),
        true,
        sale.ids[5],
    ));
}

#[test]
fn signed_allocation_splits_minted_supply_without_funding_the_pool() {
    let mut sale = Sale::new();
    sale.create().unwrap();
    sale.continue_creation(FixedPriceCreationStage::Minted)
        .unwrap();
    assert_eq!(
        sale.launch().creation_stage,
        FixedPriceCreationStage::Allocated
    );
    assert_eq!(sale.balance(2), 0);
    assert_eq!(sale.balance(8), 800);
    assert_eq!(sale.balance(4), 200);
    for index in [6, 9, 10, 11] {
        assert_eq!(sale.accounts[&sale.ids[index]], Account::default());
    }
    assert!(matches!(
        token_core::TokenDefinition::try_from(&sale.accounts[&sale.ids[1]].data).unwrap(),
        token_core::TokenDefinition::Fungible {
            total_supply: 1_000,
            ..
        }
    ));
}

#[test]
fn signed_prepare_claims_empty_vaults_and_keeps_inventory_in_factory_custody() {
    let mut sale = Sale::new();
    sale.create().unwrap();
    sale.continue_creation(FixedPriceCreationStage::Minted)
        .unwrap();
    sale.continue_creation(FixedPriceCreationStage::Allocated)
        .unwrap();
    assert_eq!(
        sale.launch().creation_stage,
        FixedPriceCreationStage::PoolPrepared
    );
    assert_eq!(sale.balance(8), 800);
    assert_eq!(sale.balance(4), 200);
    assert_eq!(sale.balance(10), 0);
    assert_eq!(sale.balance(11), 0);
    let pool = sale.pool();
    assert!(!pool.funded);
    assert_eq!(pool.recorded_collateral, 0);
    assert_eq!(pool.namespace, signer(3));
    assert_eq!(pool.namespace_program_id, sale.curve.id());
    assert_eq!(pool.factory_owner, sale.ids[0]);
    assert_eq!(pool.factory_program_id, sale.factory.id());
    assert_eq!(pool.token_definition_id, sale.ids[1]);
    assert_eq!(pool.collateral_definition_id, sale.ids[7]);
    assert_eq!(
        pool.price,
        FixedPrice {
            collateral_units: 3,
            token_units: 2
        }
    );
    assert_eq!((pool.start_timestamp, pool.end_timestamp), (100, 200));
    assert_eq!(
        pool.creator_allocation_release,
        CreatorAllocationRelease::AfterEnd
    );
    assert_eq!(sale.accounts[&sale.ids[9]].program_owner, sale.lbp.id());
    assert_eq!(sale.accounts[&sale.ids[6]], Account::default());
}

#[test]
fn signed_activation_funds_only_inventory_and_keeps_delayed_allocation_locked() {
    let mut sale = Sale::new();
    sale.create().unwrap();
    sale.continue_creation(FixedPriceCreationStage::Minted)
        .unwrap();
    sale.continue_creation(FixedPriceCreationStage::Allocated)
        .unwrap();
    let namespace = sale.accounts[&sale.ids[13]].clone();
    sale.continue_creation(FixedPriceCreationStage::PoolPrepared)
        .unwrap();
    assert_eq!(
        sale.launch().creation_stage,
        FixedPriceCreationStage::Complete
    );
    assert!(!sale.launch().creator_allocation_released);
    assert!(sale.pool().funded);
    assert_eq!(sale.pool().recorded_collateral, 0);
    assert_eq!(sale.balance(10), 800);
    assert_eq!(sale.balance(11), 0);
    assert_eq!(sale.balance(8), 0);
    assert_eq!(sale.balance(4), 200);
    assert_eq!(sale.balance(2), 0);
    assert_eq!(sale.accounts[&sale.ids[6]], Account::default());
    assert_eq!(sale.accounts[&sale.ids[13]], namespace);
}

#[test]
fn signed_final_setup_pays_immediate_allocation_before_scheduled_start() {
    let mut sale = Sale::new();
    sale.inputs.start = SaleStart::At { timestamp: 175 };
    sale.inputs.allocation_release = CreatorAllocationRelease::AfterSetup;
    sale.create().unwrap();
    sale.continue_creation(FixedPriceCreationStage::Minted)
        .unwrap();
    sale.continue_creation(FixedPriceCreationStage::Allocated)
        .unwrap();
    assert_eq!(sale.balance(6), 0);
    assert_eq!(sale.balance(4), 200);
    assert!(!sale.launch().creator_allocation_released);
    sale.continue_creation(FixedPriceCreationStage::PoolPrepared)
        .unwrap();
    assert_eq!(sale.timestamp, 100);
    assert_eq!(
        (sale.pool().start_timestamp, sale.pool().end_timestamp),
        (175, 200)
    );
    assert_eq!(
        (sale.launch().start_timestamp, sale.launch().end_timestamp),
        (175, 200)
    );
    assert!(sale.pool().funded);
    assert!(sale.launch().creator_allocation_released);
    assert_eq!(sale.balance(6), 200);
    assert_eq!(sale.balance(4), 0);
    assert_eq!(sale.balance(10), 800);
    assert_eq!(sale.balance(11), 0);
    assert_eq!(sale.balance(8), 0);
}

#[test]
fn signed_late_setup_keeps_the_original_start_and_end() {
    for (start, expected_start) in [
        (SaleStart::Now, 100),
        (SaleStart::At { timestamp: 125 }, 125),
    ] {
        let mut sale = Sale::new();
        sale.inputs.start = start;
        sale.create().unwrap();
        sale.set_time(151);
        sale.continue_creation(FixedPriceCreationStage::Minted)
            .unwrap();
        sale.set_time(175);
        sale.continue_creation(FixedPriceCreationStage::Allocated)
            .unwrap();
        sale.set_time(199);
        sale.continue_creation(FixedPriceCreationStage::PoolPrepared)
            .unwrap();
        assert_eq!(
            (sale.launch().start_timestamp, sale.launch().end_timestamp),
            (expected_start, 200)
        );
        assert_eq!(
            (sale.pool().start_timestamp, sale.pool().end_timestamp),
            (expected_start, 200)
        );
        assert!(sale.pool().funded);
        assert_eq!(sale.balance(10), 800);
        assert_eq!(sale.balance(11), 0);
    }
}

#[test]
fn signed_zero_allocation_setup_needs_no_creator_escrow_or_recipient_holding() {
    for release in [
        CreatorAllocationRelease::AfterSetup,
        CreatorAllocationRelease::AfterEnd,
    ] {
        let mut sale = Sale::new();
        sale.inputs.creator_allocation = 0;
        sale.inputs.allocation_release = release;
        sale.create().unwrap();
        assert_eq!(sale.balance(2), 800);
        for stage in [
            FixedPriceCreationStage::Minted,
            FixedPriceCreationStage::Allocated,
            FixedPriceCreationStage::PoolPrepared,
        ] {
            sale.continue_creation(stage).unwrap();
            assert_eq!(sale.accounts[&sale.ids[4]], Account::default());
            assert_eq!(sale.accounts[&sale.ids[6]], Account::default());
        }
        assert_eq!(
            sale.launch().creation_stage,
            FixedPriceCreationStage::Complete
        );
        assert!(sale.pool().funded);
        assert_eq!(sale.balance(10), 800);
        assert_eq!(sale.balance(11), 0);
        assert_eq!(sale.balance(8), 0);
        assert_eq!(sale.balance(2), 0);
        assert!(matches!(
            token_core::TokenDefinition::try_from(&sale.accounts[&sale.ids[1]].data).unwrap(),
            token_core::TokenDefinition::Fungible {
                total_supply: 800,
                ..
            }
        ));
    }
}

#[test]
fn signed_creation_requires_the_selected_creator_signature() {
    let mut sale = Sale::new();
    sale.accounts.insert(
        signer(21),
        Account {
            program_owner: [77; 8],
            ..Account::default()
        },
    );
    for keys in [vec![], vec![3], vec![21]] {
        let saved = sale.accounts.clone();
        let error = sale
            .execute(
                sale.factory.id(),
                sale.ids.clone(),
                &keys,
                Instruction::CreateFixedPriceLbp {
                    inputs: sale.inputs.clone(),
                },
            )
            .unwrap_err();
        assert_program_error(error, "Sale creator authorization failed");
        assert_eq!(sale.accounts, saved);
        assert!(!sale.accounts.contains_key(&sale.ids[0]));
    }
    sale.create().unwrap();
    assert_eq!(sale.balance(2), 1_000);
}

#[test]
fn signed_continuations_require_creator_role_and_reject_namespace_admin() {
    let mut sale = Sale::new();
    sale.accounts.insert(
        signer(21),
        Account {
            program_owner: [77; 8],
            ..Account::default()
        },
    );
    sale.create().unwrap();
    for stage in [
        FixedPriceCreationStage::Minted,
        FixedPriceCreationStage::Allocated,
        FixedPriceCreationStage::PoolPrepared,
    ] {
        for authority in [None, Some(21), Some(3)] {
            let saved = sale.accounts.clone();
            let mut ids = sale.ids.clone();
            let keys = if let Some(n) = authority {
                ids[5] = signer(n);
                ids[6] = ata(signer(n), ids[1]);
                vec![n]
            } else {
                vec![]
            };
            let error = sale
                .execute(
                    sale.factory.id(),
                    ids,
                    &keys,
                    Instruction::ContinueFixedPriceCreation {
                        expected_stage: stage,
                    },
                )
                .unwrap_err();
            assert_program_error(error, "Sale creator authorization failed");
            assert_eq!(sale.accounts, saved);
            assert_eq!(sale.accounts[&signer(20)].nonce, saved[&signer(20)].nonce);
            assert_eq!(sale.accounts[&signer(3)].nonce, saved[&signer(3)].nonce);
            assert_eq!(sale.launch().creation_stage, stage);
        }
        sale.continue_creation(stage).unwrap();
    }
    assert!(sale.pool().funded);
    assert_eq!(sale.balance(10), 800);
}

#[test]
fn signed_unsolicited_collateral_does_not_block_activation_or_enter_sale_accounting() {
    for release in [
        CreatorAllocationRelease::AfterSetup,
        CreatorAllocationRelease::AfterEnd,
    ] {
        let mut sale = Sale::new();
        sale.inputs.allocation_release = release;
        sale.create().unwrap();
        sale.continue_creation(FixedPriceCreationStage::Minted)
            .unwrap();
        sale.continue_creation(FixedPriceCreationStage::Allocated)
            .unwrap();
        assert_eq!(sale.pool().recorded_collateral, 0);
        let donor = signer(21);
        sale.accounts.insert(
            donor,
            Account {
                program_owner: programs::token().id(),
                data: Data::from(&TokenHolding::Fungible {
                    definition_id: sale.ids[7],
                    balance: 1,
                }),
                ..Account::default()
            },
        );
        let creator = sale.accounts[&signer(20)].clone();
        sale.execute(
            programs::token().id(),
            vec![donor, sale.ids[11]],
            &[21],
            token_core::Instruction::Transfer {
                amount_to_transfer: 1,
            },
        )
        .unwrap();
        assert_eq!(sale.balance(11), 1);
        assert_eq!(sale.pool().recorded_collateral, 0);
        assert_eq!(sale.accounts[&signer(20)], creator);
        assert!(matches!(
            TokenHolding::try_from(&sale.accounts[&donor].data).unwrap(),
            TokenHolding::Fungible { balance: 0, .. }
        ));
        assert_eq!(sale.accounts[&donor].nonce.0, 1);
        let collateral_vault = sale.accounts[&sale.ids[11]].clone();
        let donor_account = sale.accounts[&donor].clone();
        let mut expected_pool = sale.pool();
        expected_pool.funded = true;
        sale.continue_creation(FixedPriceCreationStage::PoolPrepared)
            .unwrap();
        assert_eq!(
            sale.launch().creation_stage,
            FixedPriceCreationStage::Complete
        );
        assert_eq!(sale.pool(), expected_pool);
        assert!(sale.pool().funded);
        assert_eq!(sale.pool().recorded_collateral, 0);
        assert_eq!(sale.accounts[&sale.ids[11]], collateral_vault);
        assert_eq!(sale.accounts[&donor], donor_account);
        assert_eq!(sale.balance(2), 0);
        assert_eq!(sale.balance(8), 0);
        assert_eq!(sale.balance(10), 800);
        assert_eq!(sale.balance(11), 1);
        match release {
            CreatorAllocationRelease::AfterSetup => {
                assert!(sale.launch().creator_allocation_released);
                assert_eq!(sale.balance(4), 0);
                assert_eq!(sale.balance(6), 200);
            }
            CreatorAllocationRelease::AfterEnd => {
                assert!(!sale.launch().creator_allocation_released);
                assert_eq!(sale.balance(4), 200);
                assert_eq!(sale.accounts[&sale.ids[6]], Account::default());
            }
        }
        let saved = sale.accounts.clone();
        for replay in [
            FixedPriceCreationStage::PoolPrepared,
            FixedPriceCreationStage::Complete,
        ] {
            sale.continue_creation(replay).unwrap_err();
            assert_eq!(sale.accounts, saved);
        }
    }
}

#[test]
fn signed_prepare_child_failure_rolls_back_stage_and_allows_corrected_retry() {
    let mut sale = Sale::new();
    sale.create().unwrap();
    sale.continue_creation(FixedPriceCreationStage::Minted)
        .unwrap();
    sale.accounts.insert(
        sale.ids[10],
        Account {
            program_owner: programs::token().id(),
            data: Data::from(&TokenHolding::Fungible {
                definition_id: sale.ids[7],
                balance: 0,
            }),
            ..Account::default()
        },
    );
    let saved = sale.accounts.clone();
    let diagnostic = stage_diagnostic(
        &sale,
        Instruction::ContinueFixedPriceCreation {
            expected_stage: FixedPriceCreationStage::Allocated,
        },
    );
    let (failed_program, message) = diagnostic.failure.as_ref().unwrap();
    assert_eq!(*failed_program, sale.lbp.id());
    assert!(
        message.contains("Token holding has the wrong definition"),
        "{message}"
    );
    assert_eq!(diagnostic.invocations.len(), 1);
    assert_eq!(
        FixedPriceLaunchState::try_from(&diagnostic.accounts[&sale.ids[0]].data)
            .unwrap()
            .creation_stage,
        FixedPriceCreationStage::PoolPrepared
    );
    assert_eq!(sale.accounts, saved);
    assert_program_error(
        sale.continue_creation(FixedPriceCreationStage::Allocated)
            .unwrap_err(),
        "Token holding has the wrong definition",
    );
    assert_eq!(sale.accounts, saved);
    assert_eq!(
        sale.launch().creation_stage,
        FixedPriceCreationStage::Allocated
    );
    assert_eq!(sale.balance(8), 800);
    assert_eq!(sale.balance(4), 200);
    for index in [6, 9, 11] {
        assert_eq!(sale.accounts[&sale.ids[index]], Account::default());
    }
    sale.accounts.insert(sale.ids[10], Account::default());
    sale.continue_creation(FixedPriceCreationStage::Allocated)
        .unwrap();
    assert_eq!(
        sale.launch().creation_stage,
        FixedPriceCreationStage::PoolPrepared
    );
    assert!(!sale.pool().funded);
    assert_eq!(sale.balance(10), 0);
    assert_eq!(sale.balance(11), 0);
}

#[test]
fn signed_payout_child_failure_rolls_back_funding_and_allows_corrected_retry() {
    let mut sale = Sale::new();
    sale.inputs.allocation_release = CreatorAllocationRelease::AfterSetup;
    sale.create().unwrap();
    sale.continue_creation(FixedPriceCreationStage::Minted)
        .unwrap();
    sale.continue_creation(FixedPriceCreationStage::Allocated)
        .unwrap();
    sale.accounts.get_mut(&sale.ids[6]).unwrap().data = Data::from(&TokenHolding::Fungible {
        definition_id: sale.ids[1],
        balance: u128::MAX,
    });
    let saved = sale.accounts.clone();
    let diagnostic = stage_diagnostic(
        &sale,
        Instruction::ContinueFixedPriceCreation {
            expected_stage: FixedPriceCreationStage::PoolPrepared,
        },
    );
    let (failed_program, message) = diagnostic.failure.as_ref().unwrap();
    assert_eq!(*failed_program, programs::token().id());
    assert!(message.contains("Recipient balance overflow"), "{message}");
    assert_eq!(
        diagnostic
            .invocations
            .iter()
            .map(|call| call.program)
            .collect::<Vec<_>>(),
        vec![
            sale.factory.id(),
            sale.lbp.id(),
            programs::ata().id(),
            programs::token().id()
        ]
    );
    assert!(
        lbp_core::PoolAccount::try_from(&diagnostic.accounts[&sale.ids[9]].data)
            .unwrap()
            .funded
    );
    for (index, expected) in [(8, 0), (10, 800), (4, 200), (6, u128::MAX), (11, 0)] {
        assert!(
            matches!(TokenHolding::try_from(&diagnostic.accounts[&sale.ids[index]].data).unwrap(),
            TokenHolding::Fungible { balance, .. } if balance == expected)
        );
    }
    assert_eq!(sale.accounts, saved);
    assert_program_error(
        sale.continue_creation(FixedPriceCreationStage::PoolPrepared)
            .unwrap_err(),
        "Recipient balance overflow",
    );
    assert_eq!(sale.accounts, saved);
    assert_eq!(
        sale.launch().creation_stage,
        FixedPriceCreationStage::PoolPrepared
    );
    assert!(!sale.launch().creator_allocation_released);
    assert!(!sale.pool().funded);
    assert_eq!(sale.balance(8), 800);
    assert_eq!(sale.balance(4), 200);
    assert_eq!(sale.balance(6), u128::MAX);
    assert_eq!(sale.balance(10), 0);
    assert_eq!(sale.balance(11), 0);
    sale.accounts.get_mut(&sale.ids[6]).unwrap().data = Data::from(&TokenHolding::Fungible {
        definition_id: sale.ids[1],
        balance: 0,
    });
    sale.continue_creation(FixedPriceCreationStage::PoolPrepared)
        .unwrap();
    assert!(sale.pool().funded);
    assert!(sale.launch().creator_allocation_released);
    assert_eq!(sale.balance(8), 0);
    assert_eq!(sale.balance(4), 0);
    assert_eq!(sale.balance(6), 200);
    assert_eq!(sale.balance(10), 800);
    assert_eq!(sale.balance(11), 0);
}

#[test]
fn signed_continuations_reject_expiry_at_end_and_after_end() {
    let mut sale = Sale::new();
    sale.create().unwrap();
    for stage in [
        FixedPriceCreationStage::Minted,
        FixedPriceCreationStage::Allocated,
        FixedPriceCreationStage::PoolPrepared,
    ] {
        for timestamp in [200, 201] {
            sale.set_time(timestamp);
            let saved = sale.accounts.clone();
            assert_program_error(
                sale.continue_creation(stage).unwrap_err(),
                "Sale setup has expired",
            );
            assert_eq!(sale.accounts, saved);
            assert_eq!(sale.launch().creation_stage, stage);
            assert_eq!(
                (sale.launch().start_timestamp, sale.launch().end_timestamp),
                (100, 200)
            );
        }
        sale.set_time(199);
        sale.continue_creation(stage).unwrap();
    }
    assert!(sale.pool().funded);
    assert_eq!(
        (sale.pool().start_timestamp, sale.pool().end_timestamp),
        (100, 200)
    );
}

#[test]
fn signed_creation_and_continuations_reject_a_fake_clock() {
    let mut sale = Sale::new();
    let fake = AccountId::new([94; 32]);
    sale.accounts
        .insert(fake, sale.accounts[&sale.ids[12]].clone());
    let mut ids = sale.ids.clone();
    ids[12] = fake;
    let saved = sale.accounts.clone();
    let error = sale
        .execute(
            sale.factory.id(),
            ids.clone(),
            &[20],
            Instruction::CreateFixedPriceLbp {
                inputs: sale.inputs.clone(),
            },
        )
        .unwrap_err();
    assert_program_error(error, "Clock account is not the trusted LEZ clock");
    assert_eq!(sale.accounts, saved);
    sale.create().unwrap();
    for stage in [
        FixedPriceCreationStage::Minted,
        FixedPriceCreationStage::Allocated,
        FixedPriceCreationStage::PoolPrepared,
    ] {
        let saved = sale.accounts.clone();
        let error = sale
            .execute(
                sale.factory.id(),
                ids.clone(),
                &[20],
                Instruction::ContinueFixedPriceCreation {
                    expected_stage: stage,
                },
            )
            .unwrap_err();
        assert_program_error(error, "Clock account is not the trusted LEZ clock");
        assert_eq!(sale.accounts, saved);
        assert_eq!(sale.launch().creation_stage, stage);
        sale.continue_creation(stage).unwrap();
    }
    assert!(sale.pool().funded);
}

#[test]
fn signed_replay_and_stale_stages_cannot_mint_fund_or_pay_twice() {
    let mut sale = Sale::new();
    sale.inputs.allocation_release = CreatorAllocationRelease::AfterSetup;
    sale.create().unwrap();
    let stages = [
        FixedPriceCreationStage::Minted,
        FixedPriceCreationStage::Allocated,
        FixedPriceCreationStage::PoolPrepared,
        FixedPriceCreationStage::Complete,
    ];
    for stage in stages {
        let saved = sale.accounts.clone();
        assert_program_error(sale.create().unwrap_err(), "already exists");
        assert_eq!(sale.accounts, saved);
        for stale in stages.into_iter().filter(|candidate| *candidate != stage) {
            assert_program_error(
                sale.continue_creation(stale).unwrap_err(),
                "Stale creation stage",
            );
            assert_eq!(sale.accounts, saved);
            assert_eq!(sale.launch().creation_stage, stage);
        }
        if stage != FixedPriceCreationStage::Complete {
            sale.continue_creation(stage).unwrap();
        } else {
            sale.continue_creation(FixedPriceCreationStage::Complete)
                .unwrap_err();
            assert_eq!(sale.accounts, saved);
        }
    }
    assert_eq!(sale.balance(10), 800);
    assert_eq!(sale.balance(6), 200);
    assert_eq!(sale.balance(4), 0);
    assert_eq!(sale.balance(8), 0);
    assert!(sale.pool().funded);
}

#[test]
fn signed_continuations_reject_substituted_sale_and_token_custody() {
    let mut sale = Sale::new();
    sale.create().unwrap();
    for stage in [
        FixedPriceCreationStage::Minted,
        FixedPriceCreationStage::Allocated,
        FixedPriceCreationStage::PoolPrepared,
    ] {
        for (index, message) in [
            (0, "Wrong factory PDA"),
            (2, "Wrong mint PDA"),
            (4, "Wrong creator escrow"),
            (8, "ATA account ID does not match expected derivation"),
            (9, "Wrong LBP pool"),
            (10, "ATA account ID does not match expected derivation"),
            (11, "ATA account ID does not match expected derivation"),
        ] {
            let substitute = AccountId::new([90 + index as u8; 32]);
            sale.accounts
                .insert(substitute, sale.accounts[&sale.ids[index]].clone());
            let saved = sale.accounts.clone();
            let mut ids = sale.ids.clone();
            ids[index] = substitute;
            let error = sale
                .execute(
                    sale.factory.id(),
                    ids,
                    &[20],
                    Instruction::ContinueFixedPriceCreation {
                        expected_stage: stage,
                    },
                )
                .unwrap_err();
            assert_program_error(error, message);
            assert_eq!(sale.accounts, saved);
            assert_eq!(sale.launch().creation_stage, stage);
            sale.accounts.remove(&substitute);
        }
        sale.continue_creation(stage).unwrap();
    }
    assert!(sale.pool().funded);
    assert_eq!(sale.balance(10), 800);
}
