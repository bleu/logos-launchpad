//! Signed public budget evidence for a test-only complete setup graph.
use std::{collections::HashMap, path::PathBuf};

#[path = "support/fixed_price_runtime.rs"]
mod fixed_price_runtime;

use factory_core::{CreatorAllocationRelease, FixedPrice, FixedPriceInputs, SaleStart};
use lee::{PrivateKey, PublicKey, V03State, error::LeeError, program::Program};
use lee_core::account::{Account, AccountId, Data};

fn guest(name: &str) -> Program {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../methods/target/riscv-guest/launchpad_methods/launchpad_programs/riscv32im-risc0-zkvm-elf/release");
    launchpad_client::load_program(&root.join(format!("{name}.bin"))).unwrap()
}
fn ata(owner: AccountId, definition: AccountId) -> AccountId {
    associated_token_account_core::get_associated_token_account_id(
        &programs::ata().id(),
        &associated_token_account_core::compute_ata_seed(owner, definition),
    )
}
struct Fixture {
    programs: Vec<Program>,
    accounts: HashMap<AccountId, Account>,
    ids: Vec<AccountId>,
    key: PrivateKey,
    inputs: FixedPriceInputs,
}
impl Fixture {
    fn new(allocation: u128, release: CreatorAllocationRelease) -> Self {
        let probe = guest("fixed_price_setup_probe");
        let curve = guest("curve");
        let lbp = guest("lbp");
        let key = PrivateKey::try_new([99; 32]).unwrap();
        let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
        let namespace = AccountId::new([97; 32]);
        let salt = [8; 32];
        let collateral = AccountId::new([5; 32]);
        let factory = factory_core::compute_factory_pda(namespace, probe.id(), salt);
        let definition = factory_core::compute_definition_pda(namespace, probe.id(), salt);
        let pool = lbp_core::compute_pool_pda(namespace, lbp.id(), definition, collateral, factory);
        let config = curve_core::compute_config_pda(namespace, curve.id());
        let clock = clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID;
        let accounts = HashMap::from([
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
                config,
                Account {
                    program_owner: curve.id(),
                    data: Data::from(&curve_core::Config::new(
                        namespace,
                        500,
                        AccountId::new([6; 32]),
                    )),
                    ..Account::default()
                },
            ),
            (
                clock,
                Account {
                    program_owner: [88; 8],
                    data: Data::try_from(
                        clock_core::ClockAccountData {
                            block_id: 1,
                            timestamp: 1,
                        }
                        .to_bytes(),
                    )
                    .unwrap(),
                    ..Account::default()
                },
            ),
        ]);
        let ids = vec![
            factory,
            definition,
            factory_core::compute_mint_pda(namespace, probe.id(), salt),
            factory_core::compute_metadata_pda(namespace, probe.id(), salt),
            factory_core::compute_escrow_pda(namespace, probe.id(), salt),
            creator,
            ata(creator, definition),
            collateral,
            ata(factory, definition),
            pool,
            ata(pool, definition),
            ata(pool, collateral),
            clock,
            config,
        ];
        let inputs = FixedPriceInputs {
            namespace,
            launch_salt: salt,
            name: "Sale".into(),
            uri: "ipfs://sale".into(),
            sale_inventory: 800,
            creator_allocation: allocation,
            price: FixedPrice {
                collateral_units: 1,
                token_units: 2,
            },
            start: SaleStart::Now,
            end_timestamp: 42,
            allocation_release: release,
            namespace_program_id: curve.id(),
            lbp_program_id: lbp.id(),
        };
        Self {
            programs: vec![probe, curve, lbp, programs::token(), programs::ata()],
            accounts,
            ids,
            key,
            inputs,
        }
    }
    fn transaction(&self) -> lee::PublicTransaction {
        let message = lee::public_transaction::Message::try_new(
            self.programs[0].id(),
            self.ids.clone(),
            vec![self.accounts[&self.ids[5]].nonce],
            factory_core::Instruction::CreateFixedPriceLbp {
                inputs: self.inputs.clone(),
            },
        )
        .unwrap();
        let witnesses = lee::public_transaction::WitnessSet::for_message(&message, &[&self.key]);
        lee::PublicTransaction::new(message, witnesses)
    }
    fn public_error(&self) -> LeeError {
        let mut state = V03State::new()
            .with_public_accounts(self.accounts.clone())
            .with_programs(self.programs.clone());
        let error = state
            .transition_from_public_transaction(&self.transaction(), 1, 1)
            .unwrap_err();
        for id in self.accounts.keys().chain(&self.ids) {
            assert_eq!(
                state.get_account_by_id(*id),
                self.accounts.get(id).cloned().unwrap_or_default(),
                "failed transaction changed {id}"
            );
        }
        println!(
            "PUBLIC allocation={} release={:?}: {error:?}",
            self.inputs.creator_allocation, self.inputs.allocation_release
        );
        error
    }
}

// The public adapter returns no cycle counts; this diagnostic checks the full graph.
fn diagnostic(fixture: &Fixture) -> (HashMap<AccountId, Account>, Vec<u64>) {
    let diagnostic = fixed_price_runtime::diagnostic(
        &fixture.programs,
        &fixture.accounts,
        fixture.transaction(),
        1,
        32,
    );
    assert!(diagnostic.failure.is_none(), "{diagnostic:?}");
    let mut cycles = Vec::new();
    let mut emitted = 0;
    for (index, invocation) in diagnostic.invocations.iter().enumerate() {
        let label = if invocation.program == fixture.programs[0].id() {
            "probe"
        } else if invocation.program == fixture.inputs.lbp_program_id {
            "lbp"
        } else if invocation.program == programs::ata().id() {
            "ata"
        } else {
            "token"
        };
        println!(
            "DIAGNOSTIC invocation={index} program={label} cycles={} emitted={}",
            invocation.cycles, invocation.emitted
        );
        cycles.push(invocation.cycles);
        emitted += invocation.emitted;
    }
    println!(
        "DIAGNOSTIC allocation={} release={:?} total_executions={} chained_executions={emitted} cycles={cycles:?} aggregate_cycles={}",
        fixture.inputs.creator_allocation,
        fixture.inputs.allocation_release,
        cycles.len(),
        cycles.iter().sum::<u64>()
    );
    (diagnostic.accounts, cycles)
}

fn assert_complete(fixture: &Fixture, accounts: &HashMap<AccountId, Account>) {
    let balance =
        |index| match token_core::TokenHolding::try_from(&accounts[&fixture.ids[index]].data)
            .unwrap()
        {
            token_core::TokenHolding::Fungible { balance, .. } => balance,
            _ => panic!("fungible holding required"),
        };
    let state = lbp_core::PoolAccount::try_from(&accounts[&fixture.ids[9]].data).unwrap();
    assert_eq!(
        accounts[&fixture.ids[9]].program_owner,
        fixture.inputs.lbp_program_id
    );
    assert_eq!(state.namespace, fixture.inputs.namespace);
    assert_eq!(
        state.namespace_program_id,
        fixture.inputs.namespace_program_id
    );
    assert_eq!(state.factory_owner, fixture.ids[0]);
    assert_eq!(state.factory_program_id, fixture.programs[0].id());
    assert_eq!(state.token_definition_id, fixture.ids[1]);
    assert_eq!(state.collateral_definition_id, fixture.ids[7]);
    assert_eq!(
        state.price,
        FixedPrice {
            collateral_units: 1,
            token_units: 2
        }
    );
    assert_eq!(state.creator_allocation, fixture.inputs.creator_allocation);
    assert_eq!(
        state.creator_allocation_release,
        fixture.inputs.allocation_release
    );
    let launch =
        factory_core::FixedPriceLaunchState::try_from(&accounts[&fixture.ids[0]].data).unwrap();
    assert_eq!(
        launch.creation_stage,
        factory_core::FixedPriceCreationStage::Complete
    );
    launch.assert_creator(&lee_core::account::AccountWithMetadata::new(
        accounts[&fixture.ids[5]].clone(),
        true,
        fixture.ids[5],
    ));
    assert_eq!(launch.total_supply, 800 + fixture.inputs.creator_allocation);
    assert!(state.funded);
    assert_eq!(state.sale_inventory, 800);
    assert_eq!(state.start_timestamp, 1);
    assert_eq!(state.end_timestamp, 42);
    assert_eq!(balance(10), 800);
    assert_eq!(balance(11), 0);
    assert_eq!(balance(8), 0);
    assert_eq!(balance(2), 0);
    let allocation = fixture.inputs.creator_allocation;
    if allocation > 0 {
        let immediate = fixture.inputs.allocation_release == CreatorAllocationRelease::AfterSetup;
        assert_eq!(balance(4), if immediate { 0 } else { allocation });
        if immediate {
            assert_eq!(balance(6), allocation);
        } else {
            assert!(
                !accounts.contains_key(&fixture.ids[6])
                    || accounts[&fixture.ids[6]] == Account::default()
            );
        }
    }
    let definition =
        token_core::TokenDefinition::try_from(&accounts[&fixture.ids[1]].data).unwrap();
    assert_eq!(
        definition,
        token_core::TokenDefinition::Fungible {
            name: "Sale".into(),
            total_supply: 800 + allocation,
            metadata_id: Some(fixture.ids[3]),
        }
    );
    let metadata = token_core::TokenMetadata::try_from(&accounts[&fixture.ids[3]].data).unwrap();
    assert_eq!(metadata.definition_id, fixture.ids[1]);
    assert_eq!(metadata.uri, "ipfs://sale");
}

#[test]
fn zero_allocation_complete_graph_reaches_public_chained_call_limit() {
    for release in [
        CreatorAllocationRelease::AfterSetup,
        CreatorAllocationRelease::AfterEnd,
    ] {
        let fixture = Fixture::new(0, release);
        assert!(matches!(
            fixture.public_error(),
            LeeError::MaxChainedCallsDepthExceeded
        ));
        let (accounts, cycles) = diagnostic(&fixture);
        assert_complete(&fixture, &accounts);
        assert!(cycles.iter().all(|c| *c < 32 * 1024 * 1024));
    }
}

#[test]
fn delayed_allocation_complete_graph_reaches_public_chained_call_limit() {
    let fixture = Fixture::new(100, CreatorAllocationRelease::AfterEnd);
    assert!(matches!(
        fixture.public_error(),
        LeeError::MaxChainedCallsDepthExceeded
    ));
    let (accounts, cycles) = diagnostic(&fixture);
    assert_complete(&fixture, &accounts);
    assert!(cycles.iter().all(|c| *c < 32 * 1024 * 1024));
}

#[test]
fn immediate_allocation_complete_graph_reaches_public_chained_call_limit() {
    let fixture = Fixture::new(100, CreatorAllocationRelease::AfterSetup);
    assert!(matches!(
        fixture.public_error(),
        LeeError::MaxChainedCallsDepthExceeded
    ));
    let (accounts, cycles) = diagnostic(&fixture);
    assert_complete(&fixture, &accounts);
    assert!(cycles.iter().all(|c| *c < 32 * 1024 * 1024));
}
