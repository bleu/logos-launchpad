//! Actual guest execution, including LEZ's call limit and chained-state validation.
use lee::{
    privacy_preserving_transaction::circuit::{ProgramWithDependencies, execute_and_prove},
    program::Program,
};
use lee_core::{
    InputAccountIdentity,
    account::{Account, AccountId, AccountWithMetadata, Data},
};
use std::{collections::HashMap, path::PathBuf};
use token_core::TokenHolding;

fn guest(name: &str) -> Program {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../methods/target/riscv-guest/launchpad_methods/launchpad_programs/riscv32im-risc0-zkvm-elf/release");
    launchpad_client::load_program(&root.join(format!("{name}.bin"))).unwrap()
}
fn id(n: u8) -> AccountId {
    if n == 3 || n == 20 || n >= 99 {
        AccountId::from(&lee::PublicKey::new_from_private_key(
            &lee::PrivateKey::try_new([n; 32]).unwrap(),
        ))
    } else {
        AccountId::new([n; 32])
    }
}
fn ata(owner: AccountId, definition: AccountId) -> AccountId {
    associated_token_account_core::get_associated_token_account_id(
        &programs::ata().id(),
        &associated_token_account_core::compute_ata_seed(owner, definition),
    )
}
fn holding(definition: AccountId, balance: u128) -> Account {
    Account {
        program_owner: programs::token().id(),
        data: Data::from(&TokenHolding::Fungible {
            definition_id: definition,
            balance,
        }),
        ..Account::default()
    }
}
struct Chain {
    public_mode: bool,
    curve: Program,
    factory: Program,
    additional_programs: Vec<Program>,
    accounts: HashMap<AccountId, Account>,
}
impl Chain {
    fn new() -> Self {
        let mut accounts = HashMap::new();
        accounts.insert(
            clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
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
        );
        Self {
            public_mode: false,
            curve: guest("curve"),
            factory: guest("factory"),
            additional_programs: Vec::new(),
            accounts,
        }
    }
    fn run<I: launchpad_client::PrivateInstruction>(
        &mut self,
        call: launchpad_client::PublicInvocation<I>,
    ) {
        if self.public_mode {
            self.run_public(call);
            return;
        }
        let program = if call.program_id == self.curve.id() {
            self.curve.clone()
        } else {
            self.factory.clone()
        };
        let pre: Vec<_> = call
            .account_ids
            .iter()
            .map(|id| {
                AccountWithMetadata::new(
                    self.accounts.get(id).cloned().unwrap_or_default(),
                    call.signer_accounts.contains(id),
                    *id,
                )
            })
            .collect();
        let identities = pre.iter().map(|_| InputAccountIdentity::Public).collect();
        let graph = ProgramWithDependencies::new(
            program,
            HashMap::from([
                (self.curve.id(), self.curve.clone()),
                (self.factory.id(), self.factory.clone()),
                (programs::token().id(), programs::token()),
                (programs::ata().id(), programs::ata()),
            ]),
        );
        let (out, _) = execute_and_prove(
            pre,
            Program::serialize_instruction(call.instruction.for_private_execution()).unwrap(),
            identities,
            &graph,
        )
        .expect("guest graph must execute");
        for (pre, post) in out
            .public_pre_states
            .into_iter()
            .zip(out.public_post_states)
        {
            self.accounts.insert(pre.account_id, post);
        }
    }
    fn run_public<I: serde::Serialize>(&mut self, call: launchpad_client::PublicInvocation<I>) {
        self.try_public(call)
            .expect("public execution must validate sibling authorizations");
    }
    fn try_public<I: serde::Serialize>(
        &mut self,
        call: launchpad_client::PublicInvocation<I>,
    ) -> Result<(), lee::error::LeeError> {
        let mut state = lee::V03State::new()
            .with_public_accounts(self.accounts.clone())
            .with_programs(
                [
                    self.curve.clone(),
                    self.factory.clone(),
                    programs::token(),
                    programs::ata(),
                ]
                .into_iter()
                .chain(self.additional_programs.iter().cloned()),
            );
        let keys: Vec<_> = call
            .signer_accounts
            .iter()
            .map(|signer| {
                let n = (1..=255)
                    .find(|n| id(*n) == *signer)
                    .expect("fixture signing key");
                lee::PrivateKey::try_new([n; 32]).unwrap()
            })
            .collect();
        let nonces = call
            .signer_accounts
            .iter()
            .map(|id| state.get_account_by_id(*id).nonce)
            .collect();
        let ids = call.account_ids.clone();
        let message = lee::public_transaction::Message::try_new(
            call.program_id,
            call.account_ids,
            nonces,
            call.instruction,
        )
        .unwrap();
        let witnesses = lee::public_transaction::WitnessSet::for_message(
            &message,
            &keys.iter().collect::<Vec<_>>(),
        );
        if let Err(error) = state.transition_from_public_transaction(
            &lee::PublicTransaction::new(message, witnesses),
            1,
            1,
        ) {
            for id in self.accounts.keys().chain(ids.iter()) {
                assert_eq!(
                    state.get_account_by_id(*id),
                    self.accounts.get(id).cloned().unwrap_or_default(),
                    "failed transaction changed runtime account {id}"
                );
            }
            return Err(error);
        }
        for id in ids {
            self.accounts.insert(id, state.get_account_by_id(id));
        }
        Ok(())
    }
    fn balance(&self, account: AccountId) -> u128 {
        if self
            .accounts
            .get(&account)
            .is_none_or(|a| *a == Account::default())
        {
            return 0;
        }
        match TokenHolding::try_from(&self.accounts[&account].data).unwrap() {
            TokenHolding::Fungible { balance, .. } => balance,
            _ => panic!("fungible holding"),
        }
    }
}
#[test]
fn fee_accrual_collection_and_withdrawal_preserve_vault_liabilities() {
    for public_mode in [false, true] {
        for sell in [false, true] {
            let mut chain = Chain::new();
            chain.public_mode = public_mode;
            let (namespace, owner, trader, token0, token1, treasury) =
                (id(1), id(2), id(3), id(4), id(5), id(6));
            let pool_id =
                curve_core::compute_pool_pda(namespace, chain.curve.id(), token0, token1, owner);
            let pool = curve_core::PoolAccount {
                funded: true,
                namespace,
                owner,
                owner_program: None,
                allowed_input_token: None,
                token0_definition_id: token0,
                token1_definition_id: token1,
                pool: pool::Pool::create(pool::PoolCreateParams {
                    token0_amount: 800,
                    token1_amount: 100,
                    virtual_reserve0: 1000,
                    virtual_reserve1: 100,
                    close_timestamp: None,
                    close_on_depletion: None,
                })
                .unwrap(),
            };
            chain.accounts.insert(
                pool_id,
                Account {
                    program_owner: chain.curve.id(),
                    data: Data::from(&pool),
                    ..Account::default()
                },
            );
            let config_id = curve_core::compute_config_pda(namespace, chain.curve.id());
            chain.accounts.insert(
                config_id,
                Account {
                    program_owner: chain.curve.id(),
                    data: Data::from(&curve_core::Config {
                        admin: namespace,
                        treasury,
                        protocol_fee_bps: if sell { 500 } else { 1000 },
                    }),
                    ..Account::default()
                },
            );
            chain.accounts.insert(
                trader,
                Account {
                    program_owner: [77; 8],
                    ..Account::default()
                },
            );
            for (owner, definition, balance) in [
                (pool_id, token0, 800),
                (pool_id, token1, 100),
                (trader, token0, 1000),
                (trader, token1, 1000),
                (treasury, token1, 0),
            ] {
                chain
                    .accounts
                    .insert(ata(owner, definition), holding(definition, balance));
            }
            let (input, output) = if sell {
                (token0, token1)
            } else {
                (token1, token0)
            };
            chain.run(launchpad_client::PublicInvocation {
                program_id: chain.curve.id(),
                account_ids: vec![
                    pool_id,
                    config_id,
                    trader,
                    ata(trader, input),
                    ata(pool_id, input),
                    ata(pool_id, output),
                    ata(trader, output),
                    clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
                ],
                signer_accounts: vec![trader],
                instruction: if sell {
                    curve_core::Instruction::SwapExactInput {
                        token_in: token0,
                        amount_in: 250,
                        min_amount_out: 19,
                    }
                } else {
                    curve_core::Instruction::SwapExactOutput {
                        token_in: token1,
                        amount_out: 200,
                        max_amount_in: 28,
                    }
                },
            });
            if sell {
                assert_eq!(chain.balance(ata(trader, token0)), 750);
                assert_eq!(chain.balance(ata(trader, token1)), 1019);
                assert_eq!(chain.balance(ata(treasury, token1)), 0);
                assert_eq!(chain.balance(ata(pool_id, token1)), 81);
            } else {
                assert_eq!(chain.balance(ata(trader, token0)), 1200);
                assert_eq!(chain.balance(ata(trader, token1)), 972);
                assert_eq!(chain.balance(ata(treasury, token1)), 0);
                assert_eq!(chain.balance(ata(pool_id, token1)), 128);
            }
            let fee = if sell { 1 } else { 3 };
            let after_swap =
                curve_core::PoolAccount::try_from(&chain.accounts[&pool_id].data).unwrap();
            assert_eq!(after_swap.pool.fees_accrued, fee);
            // Rotate the namespace treasury after accrual, then collect without a signer.
            let next_treasury = if sell { id(7) } else { id(20) };
            let mut config =
                curve_core::Config::try_from(&chain.accounts[&config_id].data).unwrap();
            config.treasury = next_treasury;
            chain.accounts.get_mut(&config_id).unwrap().data = Data::from(&config);
            chain
                .accounts
                .insert(ata(next_treasury, token1), holding(token1, 0));
            if sell {
                for _ in 0..2 {
                    chain.run(launchpad_client::build_collect_fees_invocation(
                        namespace,
                        chain.curve.id(),
                        pool_id,
                        token1,
                        next_treasury,
                    ));
                }
                assert_eq!(chain.balance(ata(next_treasury, token1)), fee);
            }
            // Close the fixture and withdraw: buy fees are collected here, sell fees
            // were collected already. Both routes must pay exactly the principal.
            let mut state =
                curve_core::PoolAccount::try_from(&chain.accounts[&pool_id].data).unwrap();
            state.pool.close_pool(1).unwrap();
            let principal = state.pool.real_reserve1;
            // Instead of changing the pool's address/owner, provide its NFT master.
            let holder = id(20);
            chain.accounts.insert(
                holder,
                Account {
                    program_owner: programs::token().id(),
                    data: Data::from(&TokenHolding::NftMaster {
                        definition_id: owner,
                        print_balance: 1,
                    }),
                    ..Account::default()
                },
            );
            chain.accounts.get_mut(&pool_id).unwrap().data = Data::from(&state);
            for definition in [token0, token1] {
                chain
                    .accounts
                    .insert(ata(holder, definition), holding(definition, 0));
            }
            let mut withdrawal = launchpad_client::PublicInvocation {
                program_id: chain.curve.id(),
                account_ids: vec![
                    pool_id,
                    holder,
                    ata(holder, token0),
                    ata(holder, token1),
                    ata(pool_id, token0),
                    ata(pool_id, token1),
                    clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
                    config_id,
                    ata(next_treasury, token1),
                ],
                signer_accounts: vec![holder],
                instruction: curve_core::Instruction::WithdrawReserves,
            };
            if next_treasury == holder {
                withdrawal.account_ids.pop();
            }
            chain.run(withdrawal);
            assert_eq!(
                chain.balance(ata(holder, token1)),
                principal + if sell { 0 } else { fee }
            );
            assert_eq!(
                chain.balance(ata(next_treasury, token1)),
                fee + if sell { 0 } else { principal }
            );
            assert_eq!(chain.balance(ata(treasury, token1)), 0);
            assert_eq!(chain.balance(ata(pool_id, token1)), 0);
            let final_state =
                curve_core::PoolAccount::try_from(&chain.accounts[&pool_id].data).unwrap();
            assert_eq!(final_state.pool.fees_accrued, 0);
            assert_eq!(final_state.pool.fees_collected, fee);
            chain.run(launchpad_client::build_collect_fees_invocation(
                namespace,
                chain.curve.id(),
                pool_id,
                token1,
                next_treasury,
            ));
            assert_eq!(
                chain.balance(ata(next_treasury, token1)),
                fee + if sell { 0 } else { principal }
            );
        }
    }
}

#[test]
fn public_creation_saves_immutable_hook_configuration() {
    let mut chain = Chain::new();
    chain.public_mode = true;
    let namespace = id(1);
    let creator = id(99);
    let collateral = id(5);
    chain.private_source(id(2));
    chain.accounts.insert(
        collateral,
        Account {
            program_owner: programs::token().id(),
            data: Data::from(&token_core::TokenDefinition::Fungible {
                name: "Collateral".into(),
                total_supply: 10000,
                metadata_id: None,
            }),
            ..Account::default()
        },
    );
    chain.accounts.insert(
        curve_core::compute_config_pda(namespace, chain.curve.id()),
        Account {
            program_owner: chain.curve.id(),
            data: Data::from(&curve_core::Config {
                admin: namespace,
                treasury: id(6),
                protocol_fee_bps: 1000,
            }),
            ..Account::default()
        },
    );
    let hook = factory_core::GraduationHook {
        program_id: chain.curve.id(),
        configuration: vec![7, 11],
    };
    let request = launchpad_client::CreateSaleRequest {
        name: "Sale".into(),
        uri: "ipfs://sale".into(),
        sale_reserve: 800,
        dex_seed_reserve: 100,
        creator_allocation: 100,
        virtual_token_reserve: 2000,
        virtual_collateral_reserve: 100,
        trade_mode: factory_core::TradeMode::BuyOnly,
        graduation_hook: Some(hook.clone()),
        end_timestamp: Some(2),
    };
    let sale = launchpad_client::FactorySale {
        namespace,
        factory_program_id: chain.factory.id(),
        curve_program_id: chain.curve.id(),
        launch_salt: [8; 32],
        collateral_definition: collateral,
    };
    let factory =
        factory_core::compute_factory_pda(namespace, chain.factory.id(), sale.launch_salt);
    for _ in 0..4 {
        let state = chain
            .accounts
            .get(&factory)
            .map(|a| factory_core::FactoryState::try_from(&a.data).unwrap());
        chain.run(
            launchpad_client::next_creation_invocation(
                sale,
                creator,
                request.clone(),
                state.as_ref(),
            )
            .unwrap()
            .unwrap(),
        );
        let state = factory_core::FactoryState::try_from(&chain.accounts[&factory].data).unwrap();
        assert_eq!(state.graduation_hook, Some(hook.clone()));
    }
    let state = factory_core::FactoryState::try_from(&chain.accounts[&factory].data).unwrap();
    let mut changed = request.clone();
    changed.graduation_hook.as_mut().unwrap().configuration = vec![12];
    assert!(
        launchpad_client::next_creation_invocation(sale, creator, changed, Some(&state)).is_err()
    );
    let mut changed = request;
    changed.graduation_hook.as_mut().unwrap().program_id = programs::token().id();
    assert!(
        launchpad_client::next_creation_invocation(sale, creator, changed, Some(&state)).is_err()
    );
}

#[test]
fn permissionless_hook_transfers_only_reserved_tokens_and_recorded_collateral() {
    let mut fixture = HookFixture::new(100, 200);
    fixture.mark_ready();
    fixture.chain.accounts.insert(
        ata(fixture.factory, fixture.token),
        holding(fixture.token, 119),
    );
    fixture.chain.accounts.insert(
        ata(fixture.factory, fixture.collateral),
        holding(fixture.collateral, 23),
    );
    fixture.advance_to_hook_ready();
    fixture.chain.run(fixture.graduation());
    assert_eq!(
        fixture.state().settlement_stage,
        factory_core::SettlementStage::Graduated
    );
    let custody = factory_core::compute_graduation_pda(fixture.adapter.id(), fixture.factory);
    assert_eq!(fixture.chain.balance(ata(custody, fixture.token)), 100);
    assert_eq!(fixture.chain.balance(ata(custody, fixture.collateral)), 12);
    assert_eq!(fixture.chain.balance(ata(id(6), fixture.collateral)), 2);
    assert_eq!(
        fixture.chain.balance(ata(fixture.factory, fixture.token)),
        19
    );
    assert_eq!(
        fixture
            .chain
            .balance(ata(fixture.factory, fixture.collateral)),
        23
    );
    assert_eq!(
        fixture.chain.balance(factory_core::compute_escrow_pda(
            fixture.sale.namespace,
            fixture.sale.factory_program_id,
            fixture.sale.launch_salt,
        )),
        100
    );
    let receipt = fixture.chain.accounts[&custody].data.as_ref();
    assert_eq!(&receipt[..16], &100_u128.to_le_bytes());
    assert_eq!(&receipt[16..32], &12_u128.to_le_bytes());
    assert_eq!(&receipt[32..], &[7, 11]);
}

#[test]
fn failed_hook_rolls_back_final_payout_and_can_retry_after_adapter_readiness_changes() {
    let mut fixture = HookFixture::new(100, 200);
    fixture.mark_ready();
    fixture.set_readiness(false);
    fixture.advance_to_hook_ready();
    let saved = fixture.chain.accounts.clone();
    let error = fixture.chain.try_public(fixture.graduation()).unwrap_err();
    assert!(
        matches!(error, lee::error::LeeError::ProgramExecutionFailed(ref message) if message.contains("Adapter is not ready")),
        "{error:?}"
    );
    assert_eq!(fixture.chain.accounts, saved);
    assert_eq!(
        fixture.state().settlement_stage,
        factory_core::SettlementStage::HookReady { collateral_due: 12 }
    );
    let custody = factory_core::compute_graduation_pda(fixture.adapter.id(), fixture.factory);
    assert_eq!(fixture.chain.balance(ata(custody, fixture.token)), 0);
    assert_eq!(fixture.chain.balance(ata(custody, fixture.collateral)), 0);
    assert_eq!(
        fixture.chain.balance(ata(fixture.factory, fixture.token)),
        100
    );
    assert_eq!(
        fixture
            .chain
            .balance(ata(fixture.factory, fixture.collateral)),
        12
    );
    assert_eq!(fixture.chain.balance(ata(id(6), fixture.collateral)), 2);
    assert!(matches!(
        token_core::TokenDefinition::try_from(&fixture.chain.accounts[&fixture.token].data)
            .unwrap(),
        token_core::TokenDefinition::Fungible {
            total_supply: 400,
            ..
        }
    ));
    fixture.mark_ready();
    fixture.chain.run(fixture.graduation());
    assert_eq!(
        fixture.state().settlement_stage,
        factory_core::SettlementStage::Graduated
    );
    assert_eq!(fixture.chain.balance(ata(custody, fixture.token)), 100);
    assert_eq!(fixture.chain.balance(ata(custody, fixture.collateral)), 12);
    assert_eq!(fixture.chain.balance(ata(id(6), fixture.collateral)), 2);
}

#[test]
fn failed_hook_keeps_creator_manual_recovery_available_with_or_without_reserved_tokens() {
    for seed in [100, 0] {
        let mut fixture = HookFixture::new(seed, 200);
        fixture.set_readiness(false);
        fixture.advance_to_hook_ready();
        assert!(fixture.chain.try_public(fixture.graduation()).is_err());
        for _ in 0..2 {
            if fixture.state().settlement_stage == factory_core::SettlementStage::Complete {
                break;
            }
            fixture.chain.run(
                launchpad_client::build_withdraw_factory_proceeds_invocation(
                    fixture.sale,
                    id(99),
                    id(6),
                ),
            );
        }
        assert_eq!(
            fixture.state().settlement_stage,
            factory_core::SettlementStage::Complete
        );
        assert_eq!(fixture.chain.balance(ata(id(99), fixture.token)), seed);
        assert_eq!(fixture.chain.balance(ata(id(99), fixture.collateral)), 12);
        assert_eq!(fixture.chain.balance(ata(id(6), fixture.collateral)), 2);
        let custody = factory_core::compute_graduation_pda(fixture.adapter.id(), fixture.factory);
        assert_eq!(fixture.chain.balance(ata(custody, fixture.token)), 0);
        assert_eq!(fixture.chain.balance(ata(custody, fixture.collateral)), 0);
        let saved = fixture.chain.accounts.clone();
        fixture.mark_ready();
        assert!(fixture.chain.try_public(fixture.graduation()).is_err());
        assert_eq!(fixture.chain.balance(ata(id(99), fixture.token)), seed);
        assert_eq!(fixture.chain.balance(ata(id(99), fixture.collateral)), 12);
        assert_eq!(
            fixture.chain.accounts[&fixture.factory],
            saved[&fixture.factory]
        );
    }
}

#[test]
fn hook_runs_once_when_either_or_both_payout_amounts_are_zero() {
    for (seed, bought, collateral_due) in [(0, 200, 12), (100, 0, 0), (0, 0, 0)] {
        let mut fixture = HookFixture::new(seed, bought);
        fixture.mark_ready();
        fixture.advance_to_hook_ready();
        fixture.chain.run(fixture.graduation());
        assert_eq!(
            fixture.state().settlement_stage,
            factory_core::SettlementStage::Graduated
        );
        let custody = factory_core::compute_graduation_pda(fixture.adapter.id(), fixture.factory);
        assert_eq!(fixture.chain.balance(ata(custody, fixture.token)), seed);
        assert_eq!(
            fixture.chain.balance(ata(custody, fixture.collateral)),
            collateral_due
        );
        let receipt = fixture.chain.accounts[&custody].data.as_ref();
        assert_eq!(&receipt[..16], &seed.to_le_bytes());
        assert_eq!(&receipt[16..32], &collateral_due.to_le_bytes());
        assert_eq!(&receipt[32..], &[7, 11]);
        let saved = fixture.chain.accounts.clone();
        assert!(fixture.chain.try_public(fixture.graduation()).is_err());
        assert!(
            fixture
                .chain
                .try_public(
                    launchpad_client::build_withdraw_factory_proceeds_invocation(
                        fixture.sale,
                        id(99),
                        id(6)
                    )
                )
                .is_err()
        );
        assert_eq!(fixture.chain.accounts, saved);
    }
}

#[test]
fn no_payout_manual_bookkeeping_and_creator_claim_do_not_block_a_zero_amount_hook() {
    let mut fixture = HookFixture::new(0, 0);
    fixture
        .chain
        .run(launchpad_client::build_claim_creator_allocation_invocation(
            fixture.sale,
            id(99),
        ));
    assert_eq!(fixture.chain.balance(ata(id(99), fixture.token)), 100);
    for _ in 0..4 {
        fixture.chain.run(
            launchpad_client::build_withdraw_factory_proceeds_invocation(
                fixture.sale,
                id(99),
                id(6),
            ),
        );
    }
    assert_eq!(
        fixture.state().settlement_stage,
        factory_core::SettlementStage::NoPayout
    );
    fixture.mark_ready();
    fixture.chain.run(fixture.graduation());
    fixture.chain.run(fixture.graduation());
    assert_eq!(
        fixture.state().settlement_stage,
        factory_core::SettlementStage::Graduated
    );
    let custody = factory_core::compute_graduation_pda(fixture.adapter.id(), fixture.factory);
    assert_eq!(fixture.chain.balance(ata(custody, fixture.token)), 0);
    assert_eq!(fixture.chain.balance(ata(custody, fixture.collateral)), 0);
    let receipt = fixture.chain.accounts[&custody].data.as_ref();
    assert_eq!(&receipt[..32], &[0; 32]);
    assert_eq!(&receipt[32..], &[7, 11]);
    assert_eq!(fixture.chain.balance(ata(id(99), fixture.token)), 100);
    let saved = fixture.chain.accounts.clone();
    assert!(fixture.chain.try_public(fixture.graduation()).is_err());
    assert_eq!(fixture.chain.accounts, saved);
}

#[test]
fn hook_preserves_recorded_payouts_when_current_treasury_aliases_settlement_custody() {
    for destination in 0..3 {
        let mut fixture = HookFixture::new(100, 200);
        fixture.mark_ready();
        let custody = factory_core::compute_graduation_pda(fixture.adapter.id(), fixture.factory);
        let pool = curve_core::compute_pool_pda(
            fixture.sale.namespace,
            fixture.sale.curve_program_id,
            fixture.token,
            fixture.collateral,
            fixture.factory,
        );
        let treasury = [fixture.factory, custody, pool][destination];
        let config_id =
            curve_core::compute_config_pda(fixture.sale.namespace, fixture.sale.curve_program_id);
        let mut config =
            curve_core::Config::try_from(&fixture.chain.accounts[&config_id].data).unwrap();
        config.treasury = treasury;
        fixture.chain.accounts.get_mut(&config_id).unwrap().data = Data::from(&config);
        if treasury == custody {
            fixture.chain.accounts.insert(
                ata(custody, fixture.collateral),
                holding(fixture.collateral, 0),
            );
        }
        for _ in 0..5 {
            let invocation = launchpad_client::build_graduation_invocation(
                fixture.sale,
                &fixture.state(),
                treasury,
                vec![fixture.readiness],
            )
            .unwrap();
            assert_eq!(invocation.account_ids.len(), 14);
            fixture
                .chain
                .try_public(invocation)
                .unwrap_or_else(|error| {
                    panic!(
                        "treasury case {destination}, stage {:?}: {error:?}",
                        fixture.state().settlement_stage
                    )
                });
        }
        assert_eq!(
            fixture.state().settlement_stage,
            factory_core::SettlementStage::Graduated
        );
        assert_eq!(fixture.chain.balance(ata(custody, fixture.token)), 100);
        assert_eq!(
            fixture.chain.balance(ata(custody, fixture.collateral)),
            if treasury == custody { 14 } else { 12 }
        );
        assert_eq!(
            fixture
                .chain
                .balance(ata(fixture.factory, fixture.collateral)),
            if treasury == fixture.factory { 2 } else { 0 }
        );
        assert_eq!(
            fixture.chain.balance(ata(pool, fixture.collateral)),
            if treasury == pool { 2 } else { 0 }
        );
        assert_eq!(fixture.chain.balance(ata(id(6), fixture.collateral)), 0);
        let receipt = fixture.chain.accounts[&custody].data.as_ref();
        assert_eq!(&receipt[16..32], &12_u128.to_le_bytes());
    }
}

struct HookFixture {
    chain: Chain,
    sale: launchpad_client::FactorySale,
    factory: AccountId,
    token: AccountId,
    collateral: AccountId,
    adapter: Program,
    readiness: AccountId,
}

impl HookFixture {
    fn new(seed: u128, bought: u128) -> Self {
        let mut chain = Chain::new();
        chain.public_mode = true;
        let adapter = guest("graduation_adapter");
        chain.additional_programs.push(adapter.clone());
        let namespace = id(1);
        let collateral = id(5);
        chain.private_source(id(2));
        chain.accounts.insert(
            collateral,
            Account {
                program_owner: programs::token().id(),
                data: Data::from(&token_core::TokenDefinition::Fungible {
                    name: "Collateral".into(),
                    total_supply: 10000,
                    metadata_id: None,
                }),
                ..Account::default()
            },
        );
        chain.accounts.insert(
            curve_core::compute_config_pda(namespace, chain.curve.id()),
            Account {
                program_owner: chain.curve.id(),
                data: Data::from(&curve_core::Config {
                    admin: namespace,
                    treasury: id(6),
                    protocol_fee_bps: 1000,
                }),
                ..Account::default()
            },
        );
        let sale = launchpad_client::FactorySale {
            namespace,
            factory_program_id: chain.factory.id(),
            curve_program_id: chain.curve.id(),
            launch_salt: [9; 32],
            collateral_definition: collateral,
        };
        let factory =
            factory_core::compute_factory_pda(namespace, chain.factory.id(), sale.launch_salt);
        let token =
            factory_core::compute_definition_pda(namespace, chain.factory.id(), sale.launch_salt);
        let request = launchpad_client::CreateSaleRequest {
            trade_mode: factory_core::TradeMode::BuyOnly,
            graduation_hook: Some(factory_core::GraduationHook {
                program_id: adapter.id(),
                configuration: vec![7, 11],
            }),
            name: "Sale".into(),
            uri: "ipfs://sale".into(),
            sale_reserve: 800,
            dex_seed_reserve: seed,
            creator_allocation: 100,
            virtual_token_reserve: 2000,
            virtual_collateral_reserve: 100,
            end_timestamp: Some(2),
        };
        for _ in 0..4 {
            let state = chain
                .accounts
                .get(&factory)
                .map(|a| factory_core::FactoryState::try_from(&a.data).unwrap());
            chain.run(
                launchpad_client::next_creation_invocation(
                    sale,
                    id(99),
                    request.clone(),
                    state.as_ref(),
                )
                .unwrap()
                .unwrap(),
            );
        }
        let trader = id(20);
        chain.accounts.insert(
            trader,
            Account {
                program_owner: [77; 8],
                ..Account::default()
            },
        );
        chain.accounts.insert(ata(trader, token), holding(token, 0));
        chain
            .accounts
            .insert(ata(trader, collateral), holding(collateral, 1000));
        chain
            .accounts
            .insert(ata(id(6), collateral), holding(collateral, 0));
        if bought != 0 {
            chain.run(launchpad_client::build_buy_invocation(
                sale,
                trader,
                launchpad_client::ExactOutputTradeTerms {
                    amount_out: bought,
                    max_amount_in: 100,
                },
            ));
        }
        chain
            .accounts
            .get_mut(&clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID)
            .unwrap()
            .data = Data::try_from(
            clock_core::ClockAccountData {
                block_id: 2,
                timestamp: 2,
            }
            .to_bytes(),
        )
        .unwrap();
        let readiness = readiness_id(adapter.id(), factory);
        Self {
            chain,
            sale,
            factory,
            token,
            collateral,
            adapter,
            readiness,
        }
    }
    fn state(&self) -> factory_core::FactoryState {
        factory_core::FactoryState::try_from(&self.chain.accounts[&self.factory].data).unwrap()
    }
    fn graduation(&self) -> launchpad_client::PublicInvocation<factory_core::Instruction> {
        launchpad_client::build_graduation_invocation(
            self.sale,
            &self.state(),
            id(6),
            vec![self.readiness],
        )
        .unwrap()
    }
    fn advance_to_hook_ready(&mut self) {
        for _ in 0..4 {
            self.chain.run(self.graduation());
        }
        assert!(matches!(
            self.state().settlement_stage,
            factory_core::SettlementStage::HookReady { .. }
        ));
    }
    fn mark_ready(&mut self) {
        self.set_readiness(true);
    }
    fn set_readiness(&mut self, ready: bool) {
        self.chain.run_public(launchpad_client::PublicInvocation {
            program_id: self.adapter.id(),
            account_ids: vec![
                self.readiness,
                factory_core::compute_graduation_pda(self.adapter.id(), self.factory),
            ],
            signer_accounts: vec![],
            instruction: factory_core::GraduationRequest {
                factory_program_id: self.sale.factory_program_id,
                namespace: self.sale.namespace,
                launch_salt: self.sale.launch_salt,
                token_definition_id: self.token,
                collateral_definition_id: self.collateral,
                dex_seed_reserve: 0,
                collateral_amount: 0,
                configuration: if ready {
                    b"ready".to_vec()
                } else {
                    b"not-ready".to_vec()
                },
            },
        });
    }
}

fn readiness_id(program: lee_core::program::ProgramId, factory: AccountId) -> AccountId {
    AccountId::for_public_pda(
        &program,
        &lee_core::program::PdaSeed::new(factory.to_bytes()),
    )
}

impl Chain {
    fn private_source(&mut self, nft: AccountId) -> AccountId {
        let nsk = [9; 32];
        let id = if self.public_mode {
            id(99)
        } else {
            AccountId::for_regular_private_account(&lee_core::NullifierPublicKey::from(&nsk), 0)
        };
        self.accounts.insert(
            id,
            Account {
                program_owner: programs::token().id(),
                data: Data::from(&TokenHolding::NftMaster {
                    definition_id: nft,
                    print_balance: 1,
                }),
                nonce: lee_core::account::Nonce::private_account_nonce_init(&id),
                ..Account::default()
            },
        );
        id
    }
    fn try_private<I: launchpad_client::PrivateInstruction>(
        &mut self,
        source: AccountId,
        call: launchpad_client::PublicInvocation<I>,
    ) -> Result<(), lee::error::LeeError> {
        let holder = call.signer_accounts[0];
        let instruction =
            launchpad_client::build_private_authority_instruction(&call, holder).unwrap();
        let mut pre = vec![AccountWithMetadata::new(
            self.accounts[&source].clone(),
            true,
            source,
        )];
        pre.extend(call.account_ids.iter().map(|id| {
            AccountWithMetadata::new(
                self.accounts.get(id).cloned().unwrap_or_default(),
                *id == holder,
                *id,
            )
        }));
        let ssk = lee_core::SharedSecretKey([10; 32]);
        let mut identities = vec![InputAccountIdentity::PrivateAuthorizedUpdate {
            epk: lee_core::EphemeralPublicKey(vec![]),
            view_tag: Default::default(),
            ssk,
            nsk: [9; 32],
            membership_proof: (0, vec![]),
            identifier: 0,
        }];
        identities.extend(
            call.account_ids
                .iter()
                .map(|_| InputAccountIdentity::Public),
        );
        let graph = ProgramWithDependencies::new(
            guest("private_authority"),
            HashMap::from([
                (self.curve.id(), self.curve.clone()),
                (self.factory.id(), self.factory.clone()),
                (programs::token().id(), programs::token()),
                (programs::ata().id(), programs::ata()),
            ]),
        );
        let (out, _) = execute_and_prove(
            pre,
            Program::serialize_instruction(instruction).unwrap(),
            identities,
            &graph,
        )?;
        let (_, returned) = lee_core::EncryptionScheme::decrypt(
            &out.encrypted_private_post_states[0].ciphertext,
            &ssk,
            &out.new_commitments[0],
            0,
        )
        .unwrap();
        assert_eq!(
            returned.data, self.accounts[&source].data,
            "authority returns after each stage"
        );
        self.accounts.insert(source, returned);
        for (pre, post) in out
            .public_pre_states
            .into_iter()
            .zip(out.public_post_states)
        {
            self.accounts.insert(pre.account_id, post);
        }
        assert!(matches!(
            TokenHolding::try_from(&self.accounts[&holder].data).unwrap(),
            TokenHolding::NftMaster {
                print_balance: 0,
                ..
            }
        ));
        Ok(())
    }
    fn run_private<I: launchpad_client::PrivateInstruction>(
        &mut self,
        source: AccountId,
        call: launchpad_client::PublicInvocation<I>,
    ) {
        if self.public_mode {
            let holder = call.signer_accounts[0];
            self.run_public(launchpad_client::build_transfer_authority_invocation(
                source, holder,
            ));
            self.run_public(call);
            self.run_public(launchpad_client::build_transfer_authority_invocation(
                holder, source,
            ));
        } else {
            self.try_private(source, call)
                .expect("private stage fits LEZ limit and validates all chained states");
        }
    }
}
#[test]
fn private_sale_resumes_creation_and_settlement_with_fresh_payout_holders() {
    for public_mode in [false, true] {
        for (allocation, seed, bought, deadline, collateral_due, remaining_supply) in [
            (100, 100, 200, Some(2), 12, 400),
            (0, 0, 800, None, 67, 800),
            (100, 100, 0, Some(2), 0, 200),
        ] {
            let mut chain = Chain::new();
            chain.public_mode = public_mode;
            let namespace = id(1);
            let nft = id(2);
            let collateral = id(5);
            let salt = [7; 32];
            let source = chain.private_source(nft);
            chain.accounts.insert(
                collateral,
                Account {
                    program_owner: programs::token().id(),
                    data: Data::from(&token_core::TokenDefinition::Fungible {
                        name: "Collateral".into(),
                        total_supply: 10000,
                        metadata_id: None,
                    }),
                    ..Account::default()
                },
            );
            chain.accounts.insert(
                curve_core::compute_config_pda(namespace, chain.curve.id()),
                Account {
                    program_owner: chain.curve.id(),
                    data: Data::from(&curve_core::Config {
                        admin: namespace,
                        treasury: id(6),
                        protocol_fee_bps: 0,
                    }),
                    ..Account::default()
                },
            );
            let request = launchpad_client::CreateSaleRequest {
                name: "Sale".into(),
                uri: "ipfs://sale".into(),
                sale_reserve: 800,
                dex_seed_reserve: seed,
                creator_allocation: allocation,
                virtual_token_reserve: 2000,
                virtual_collateral_reserve: 100,
                trade_mode: factory_core::TradeMode::BuyAndSell,
                graduation_hook: None,
                end_timestamp: deadline,
            };
            let sale = launchpad_client::FactorySale {
                namespace,
                factory_program_id: chain.factory.id(),
                curve_program_id: chain.curve.id(),
                launch_salt: salt,
                collateral_definition: collateral,
            };
            let factory_id = factory_core::compute_factory_pda(namespace, chain.factory.id(), salt);
            let token = factory_core::compute_definition_pda(namespace, chain.factory.id(), salt);
            let pool_id = curve_core::compute_pool_pda(
                namespace,
                chain.curve.id(),
                token,
                collateral,
                factory_id,
            );
            for step in 0..4 {
                let existing = chain
                    .accounts
                    .get(&factory_id)
                    .map(|a| factory_core::FactoryState::try_from(&a.data).unwrap());
                let call = launchpad_client::next_creation_invocation(
                    sale,
                    id(100 + step),
                    request.clone(),
                    existing.as_ref(),
                )
                .unwrap()
                .expect("next creation stage");
                chain.run_private(source, call);
                if step == 2 {
                    assert!(
                        !curve_core::PoolAccount::try_from(&chain.accounts[&pool_id].data)
                            .unwrap()
                            .funded
                    );
                }
            }
            let state =
                factory_core::FactoryState::try_from(&chain.accounts[&factory_id].data).unwrap();
            assert_eq!(state.creation_stage, factory_core::CreationStage::Active);
            assert!(
                launchpad_client::next_creation_invocation(
                    sale,
                    id(104),
                    request.clone(),
                    Some(&state)
                )
                .unwrap()
                .is_none()
            );
            let mut changed = request.clone();
            changed.sale_reserve = 700;
            assert!(
                launchpad_client::next_creation_invocation(sale, id(104), changed, Some(&state))
                    .is_err()
            );
            assert!(
                curve_core::PoolAccount::try_from(&chain.accounts[&pool_id].data)
                    .unwrap()
                    .funded
            );
            assert_eq!(chain.balance(ata(pool_id, token)), 800);
            assert_eq!(chain.balance(ata(factory_id, token)), seed);
            assert_eq!(
                chain.balance(factory_core::compute_escrow_pda(
                    namespace,
                    chain.factory.id(),
                    salt
                )),
                allocation
            );
            assert_eq!(
                chain.balance(factory_core::compute_mint_pda(
                    namespace,
                    chain.factory.id(),
                    salt
                )),
                0
            );
            if !public_mode {
                let saved = chain.accounts.clone();
                assert!(
                    chain
                        .try_private(
                            source,
                            launchpad_client::PublicInvocation {
                                program_id: chain.curve.id(),
                                account_ids: vec![
                                    pool_id,
                                    id(104),
                                    clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
                                ],
                                signer_accounts: vec![id(104)],
                                instruction: curve_core::Instruction::ClosePool,
                            },
                        )
                        .is_err()
                );
                assert_eq!(
                    chain.accounts, saved,
                    "factory-owned pool rejects direct closure"
                );
            }
            let trader = id(20);
            chain.accounts.insert(
                trader,
                Account {
                    program_owner: [77; 8],
                    ..Account::default()
                },
            );
            chain.accounts.insert(ata(trader, token), holding(token, 0));
            chain
                .accounts
                .insert(ata(trader, collateral), holding(collateral, 1000));
            chain
                .accounts
                .insert(ata(id(6), collateral), holding(collateral, 0));
            if bought != 0 {
                chain.run(launchpad_client::build_buy_invocation(
                    sale,
                    trader,
                    launchpad_client::ExactOutputTradeTerms {
                        amount_out: bought,
                        max_amount_in: 100,
                    },
                ));
            }
            assert_eq!(chain.balance(ata(pool_id, collateral)), collateral_due);
            if deadline.is_some() {
                chain
                    .accounts
                    .get_mut(&clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID)
                    .unwrap()
                    .data = Data::try_from(
                    clock_core::ClockAccountData {
                        block_id: 2,
                        timestamp: 3,
                    }
                    .to_bytes(),
                )
                .unwrap();
            }
            chain.run_private(
                source,
                launchpad_client::build_claim_creator_allocation_invocation(sale, id(105)),
            );
            assert_eq!(chain.balance(ata(id(105), token)), allocation);
            for step in 0..5 {
                let state = factory_core::FactoryState::try_from(&chain.accounts[&factory_id].data)
                    .unwrap();
                if state.settlement_stage.is_manual_settlement_complete() {
                    break;
                }
                chain.run_private(
                    source,
                    launchpad_client::build_withdraw_factory_proceeds_invocation(
                        sale,
                        id(106 + step),
                        id(6),
                    ),
                );
            }
            assert_eq!(
                factory_core::FactoryState::try_from(&chain.accounts[&factory_id].data)
                    .unwrap()
                    .settlement_stage,
                factory_core::SettlementStage::Complete
            );
            assert_eq!(chain.balance(ata(id(109), token)), seed);
            let collateral_holder = if seed == 0 { id(109) } else { id(110) };
            assert_eq!(
                chain.balance(ata(collateral_holder, collateral)),
                collateral_due
            );
            assert_eq!(chain.balance(ata(factory_id, token)), 0);
            assert_eq!(chain.balance(ata(factory_id, collateral)), 0);
            let token_core::TokenDefinition::Fungible { total_supply, .. } =
                token_core::TokenDefinition::try_from(&chain.accounts[&token].data).unwrap()
            else {
                panic!("fungible definition")
            };
            assert_eq!(
                total_supply, remaining_supply,
                "burn only the unsold allocation"
            );
            if !public_mode {
                let saved = chain.accounts.clone();
                assert!(
                    chain
                        .try_private(
                            source,
                            launchpad_client::build_withdraw_factory_proceeds_invocation(
                                sale,
                                id(111),
                                id(6),
                            )
                        )
                        .is_err()
                );
                assert_eq!(
                    chain.accounts, saved,
                    "replay leaves all balances unchanged"
                );
            }
        }
    }
}
