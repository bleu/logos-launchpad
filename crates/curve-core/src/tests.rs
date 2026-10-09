//! AMM-style handler tests: hand-built `AccountWithMetadata` fixtures, direct handler
//! calls, `#[should_panic]` on the gates. Mirrors `lez/programs/amm/src/tests.rs`.

use associated_token_account_core::{
    Instruction as AtaInstruction, compute_ata_seed, get_associated_token_account_id,
};
use lee_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{Claim, ProgramId},
};

use pool::{FeeSide, Pool, PoolCreateParams};
use token_core::{TokenDefinition, TokenHolding};

use crate::dispatch::process_instruction;
use crate::{
    ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID, AdminCandidate, Config, Instruction, PoolAccount,
    compute_config_pda, compute_pool_pda,
    pool_create::create_pool,
    pool_lifecycle::{close_pool, withdraw_reserves},
    pool_swap::{swap_exact_input, swap_exact_output},
};

const NAMESPACE_CREATOR: AccountId = AccountId::new([0xAD; 32]);
const CURVE_PROGRAM_ID: ProgramId = [7; 8];
const ATA_PROGRAM_ID: ProgramId = ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID;
const TOKEN_PROGRAM_ID: ProgramId = crate::authority::TOKEN_PROGRAM_ID;

fn uninitialized_config() -> AccountWithMetadata {
    AccountWithMetadata {
        account: Account::default(),
        is_authorized: false,
        account_id: compute_config_pda(AccountId::new([0xAD; 32]), CURVE_PROGRAM_ID),
    }
}

fn signer(account_id: AccountId) -> AccountWithMetadata {
    AccountWithMetadata {
        account: Account {
            program_owner: crate::authority::TOKEN_PROGRAM_ID,
            data: Data::from(&TokenHolding::NftMaster {
                definition_id: account_id,
                print_balance: 1,
            }),
            ..Account::default()
        },
        is_authorized: true,
        account_id,
    }
}

fn admin_signer(account_id: AccountId) -> AccountWithMetadata {
    AccountWithMetadata {
        account: Account::default(),
        is_authorized: true,
        account_id,
    }
}

fn namespace_creator_signer() -> AccountWithMetadata {
    admin_signer(NAMESPACE_CREATOR)
}

fn initialized_config(admin: AccountId) -> AccountWithMetadata {
    let account = Account {
        program_owner: CURVE_PROGRAM_ID,
        data: Data::from(&Config::new(admin, 100, new_treasury())),
        ..Account::default()
    };
    AccountWithMetadata {
        account,
        is_authorized: false,
        account_id: compute_config_pda(AccountId::new([0xAD; 32]), CURVE_PROGRAM_ID),
    }
}

fn new_admin() -> AccountId {
    AccountId::new([1; 32])
}

fn new_treasury() -> AccountId {
    AccountId::new([2; 32])
}

fn intruder_signer() -> AccountWithMetadata {
    signer(AccountId::new([9; 32]))
}

fn token_definition(account_id: AccountId) -> AccountWithMetadata {
    AccountWithMetadata {
        account: Account {
            program_owner: TOKEN_PROGRAM_ID,
            data: Data::from(&TokenDefinition::Fungible {
                name: "Token".into(),
                total_supply: 10_000,
                metadata_id: None,
            }),
            ..Account::default()
        },
        is_authorized: false,
        account_id,
    }
}

fn ata(owner: AccountId, definition: AccountId, account: Account) -> AccountWithMetadata {
    let seed = compute_ata_seed(owner, definition);
    AccountWithMetadata {
        account,
        is_authorized: false,
        account_id: get_associated_token_account_id(&ATA_PROGRAM_ID, &seed),
    }
}

fn owner_source_ata(owner: AccountId, token_definition_id: AccountId) -> AccountWithMetadata {
    ata(
        owner,
        token_definition_id,
        Account {
            program_owner: TOKEN_PROGRAM_ID,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: token_definition_id,
                balance: 1_000,
            }),
            ..Account::default()
        },
    )
}

fn holding_ata(
    owner: AccountId,
    token_definition_id: AccountId,
    balance: u128,
) -> AccountWithMetadata {
    ata(
        owner,
        token_definition_id,
        Account {
            program_owner: TOKEN_PROGRAM_ID,
            data: Data::from(&TokenHolding::Fungible {
                definition_id: token_definition_id,
                balance,
            }),
            ..Account::default()
        },
    )
}

fn trusted_clock(timestamp: u64) -> AccountWithMetadata {
    AccountWithMetadata {
        account: Account {
            program_owner: [88; 8],
            data: Data::try_from(
                clock_core::ClockAccountData {
                    block_id: 7,
                    timestamp,
                }
                .to_bytes(),
            )
            .expect("clock data fits"),
            ..Account::default()
        },
        is_authorized: false,
        account_id: clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
    }
}

struct CreatePoolAccounts {
    pool: AccountWithMetadata,
    owner: AccountWithMetadata,
    token0_definition: AccountWithMetadata,
    token1_definition: AccountWithMetadata,
    owner_token0_ata: AccountWithMetadata,
    owner_token1_ata: AccountWithMetadata,
    pool_token0_ata: AccountWithMetadata,
    pool_token1_ata: AccountWithMetadata,
}

fn valid_create_pool_accounts() -> CreatePoolAccounts {
    let owner = signer(AccountId::new([3; 32]));
    let token0_definition_id = AccountId::new([4; 32]);
    let token1_definition_id = AccountId::new([5; 32]);
    let pool_id = compute_pool_pda(
        AccountId::new([0xAD; 32]),
        CURVE_PROGRAM_ID,
        token0_definition_id,
        token1_definition_id,
        owner.account_id,
    );
    CreatePoolAccounts {
        pool: AccountWithMetadata {
            account_id: pool_id,
            ..uninitialized_config()
        },
        owner_token0_ata: owner_source_ata(owner.account_id, token0_definition_id),
        owner_token1_ata: owner_source_ata(owner.account_id, token1_definition_id),
        owner,
        token0_definition: token_definition(token0_definition_id),
        token1_definition: token_definition(token1_definition_id),
        pool_token0_ata: ata(pool_id, token0_definition_id, Account::default()),
        pool_token1_ata: ata(pool_id, token1_definition_id, Account::default()),
    }
}

fn create_pool_with_accounts(
    accounts: CreatePoolAccounts,
    token0_amount: u128,
    token1_amount: u128,
    virtual_reserve0: u128,
    virtual_reserve1: u128,
) {
    let owner_id = accounts.owner.account_id;
    let _result = create_pool(
        AccountId::new([0xAD; 32]),
        accounts.pool,
        accounts.owner,
        accounts.token0_definition,
        accounts.token1_definition,
        accounts.owner_token0_ata,
        accounts.owner_token1_ata,
        accounts.pool_token0_ata,
        accounts.pool_token1_ata,
        trusted_clock(1),
        token0_amount,
        token1_amount,
        virtual_reserve0,
        virtual_reserve1,
        Some(42),
        None,
        owner_id,
        CURVE_PROGRAM_ID,
        None,
        None,
        false,
    );
}

#[should_panic(expected = "Owner authorization is missing")]
#[test]
fn create_pool_rejects_an_owner_without_authorization() {
    let mut accounts = valid_create_pool_accounts();
    accounts.owner.is_authorized = false;
    create_pool_with_accounts(accounts, 800, 25, 1_000, 100);
}

#[test]
fn trusted_ata_program_id_matches_the_pinned_lez_guest() {
    assert_eq!(ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID, programs::ata().id());
}

#[should_panic(expected = "ATA account ID does not match expected derivation")]
#[test]
fn create_pool_rejects_a_source_ata_not_owned_by_the_owner() {
    let intruder_id = AccountId::new([6; 32]);
    let mut accounts = valid_create_pool_accounts();
    accounts.owner_token0_ata = owner_source_ata(intruder_id, AccountId::new([4; 32]));
    create_pool_with_accounts(accounts, 800, 25, 1_000, 100);
}

#[should_panic(expected = "Pool account ID does not match PDA")]
#[test]
fn create_pool_rejects_a_forged_pool_account() {
    let mut accounts = valid_create_pool_accounts();
    accounts.pool.account_id = AccountId::new([6; 32]);
    create_pool_with_accounts(accounts, 800, 25, 1_000, 100);
}

#[should_panic(expected = "ATA account ID does not match expected derivation")]
#[test]
fn create_pool_rejects_a_forged_pool_token0_ata() {
    let mut accounts = valid_create_pool_accounts();
    accounts.pool_token0_ata.account_id = AccountId::new([6; 32]);
    create_pool_with_accounts(accounts, 800, 25, 1_000, 100);
}

#[should_panic(expected = "ATA account ID does not match expected derivation")]
#[test]
fn create_pool_rejects_a_forged_pool_token1_ata() {
    let mut accounts = valid_create_pool_accounts();
    accounts.pool_token1_ata.account_id = AccountId::new([6; 32]);
    create_pool_with_accounts(accounts, 800, 25, 1_000, 100);
}

#[should_panic(expected = "Pool is already initialized")]
#[test]
fn create_pool_rejects_reinitializing_an_existing_pool() {
    let mut accounts = valid_create_pool_accounts();
    accounts.pool.account.program_owner = CURVE_PROGRAM_ID;
    create_pool_with_accounts(accounts, 800, 25, 1_000, 100);
}

#[should_panic(expected = "Pool parameters are invalid")]
#[test]
fn create_pool_enforces_pool_parameter_validation() {
    create_pool_with_accounts(valid_create_pool_accounts(), 800, 25, 0, 100);
}

#[should_panic(expected = "Close timestamp must be in the future")]
#[test]
fn create_pool_rejects_an_end_timestamp_at_the_trusted_time_boundary() {
    let accounts = valid_create_pool_accounts();
    let owner_id = accounts.owner.account_id;
    let _ = create_pool(
        AccountId::new([0xAD; 32]),
        accounts.pool,
        accounts.owner,
        accounts.token0_definition,
        accounts.token1_definition,
        accounts.owner_token0_ata,
        accounts.owner_token1_ata,
        accounts.pool_token0_ata,
        accounts.pool_token1_ata,
        trusted_clock(42),
        800,
        25,
        1_000,
        100,
        Some(42),
        None,
        owner_id,
        CURVE_PROGRAM_ID,
        None,
        None,
        false,
    );
}

#[should_panic(expected = "Clock account is not the trusted LEZ clock")]
#[test]
fn create_pool_rejects_a_substituted_clock_even_without_an_end_timestamp() {
    let accounts = valid_create_pool_accounts();
    let owner_id = accounts.owner.account_id;
    let mut fake_clock = trusted_clock(1);
    fake_clock.account_id = AccountId::new([99; 32]);
    let _ = create_pool(
        AccountId::new([0xAD; 32]),
        accounts.pool,
        accounts.owner,
        accounts.token0_definition,
        accounts.token1_definition,
        accounts.owner_token0_ata,
        accounts.owner_token1_ata,
        accounts.pool_token0_ata,
        accounts.pool_token1_ata,
        fake_clock,
        800,
        25,
        1_000,
        100,
        None,
        None,
        owner_id,
        CURVE_PROGRAM_ID,
        None,
        None,
        false,
    );
}

#[test]
fn owner_can_open_a_pool_and_atomically_fund_both_ata_reserves() {
    let accounts = valid_create_pool_accounts();
    let owner_id = accounts.owner.account_id;
    let token0_id = accounts.token0_definition.account_id;
    let token1_id = accounts.token1_definition.account_id;
    let pool_token0_ata = accounts.pool_token0_ata.clone();
    let pool_token1_ata = accounts.pool_token1_ata.clone();
    let (post_states, chained_calls) = create_pool(
        AccountId::new([0xAD; 32]),
        accounts.pool,
        accounts.owner,
        accounts.token0_definition,
        accounts.token1_definition,
        accounts.owner_token0_ata,
        accounts.owner_token1_ata,
        accounts.pool_token0_ata,
        accounts.pool_token1_ata,
        trusted_clock(1),
        800,
        25,
        1_000,
        100,
        Some(42),
        None,
        owner_id,
        CURVE_PROGRAM_ID,
        None,
        None,
        false,
    );

    let pool_account = PoolAccount::try_from(&post_states[0].account().data)
        .expect("pool post-state holds a valid pool");
    assert_eq!(pool_account.pool.k, 100_000);
    assert_eq!(
        (
            pool_account.pool.real_reserve0,
            pool_account.pool.real_reserve1
        ),
        (800, 25)
    );
    assert_eq!(pool_account.owner, owner_id);
    assert_eq!(
        post_states[0].required_claim(),
        Some(Claim::Pda(crate::compute_pool_pda_seed(
            AccountId::new([0xAD; 32]),
            token0_id,
            token1_id,
            owner_id,
        )))
    );

    assert_eq!(chained_calls.len(), 4);
    for call in &chained_calls[..2] {
        assert_eq!(call.program_id, ATA_PROGRAM_ID);
        let instruction: AtaInstruction =
            risc0_zkvm::serde::from_slice(&call.instruction_data).expect("ATA instruction parses");
        assert!(matches!(
            instruction,
            AtaInstruction::Create {
                ata_program_id: ATA_PROGRAM_ID
            }
        ));
    }
    for (transfer, amount, destination, definition_id) in [
        (
            &chained_calls[2],
            800,
            pool_token0_ata.account_id,
            token0_id,
        ),
        (&chained_calls[3], 25, pool_token1_ata.account_id, token1_id),
    ] {
        assert_eq!(transfer.program_id, ATA_PROGRAM_ID);
        let instruction: AtaInstruction = risc0_zkvm::serde::from_slice(&transfer.instruction_data)
            .expect("ATA instruction parses");
        assert!(
            matches!(instruction, AtaInstruction::Transfer { ata_program_id: ATA_PROGRAM_ID, amount: actual } if actual == amount)
        );
        assert_eq!(transfer.pre_states[2].account_id, destination);
        assert_eq!(
            TokenHolding::try_from(&transfer.pre_states[2].account.data)
                .expect("funding recipient is initialized"),
            TokenHolding::Fungible {
                definition_id,
                balance: 0
            }
        );
    }
}

#[test]
fn zero_initial_reserve_creates_its_ata_without_emitting_a_zero_transfer() {
    let accounts = valid_create_pool_accounts();
    let owner_id = accounts.owner.account_id;
    let (_, chained_calls) = create_pool(
        AccountId::new([0xAD; 32]),
        accounts.pool,
        accounts.owner,
        accounts.token0_definition,
        accounts.token1_definition,
        accounts.owner_token0_ata,
        accounts.owner_token1_ata,
        accounts.pool_token0_ata,
        accounts.pool_token1_ata,
        trusted_clock(1),
        800,
        0,
        1_000,
        100,
        None,
        None,
        owner_id,
        CURVE_PROGRAM_ID,
        None,
        None,
        false,
    );

    assert_eq!(
        chained_calls.len(),
        3,
        "two ATA creates and one funding transfer"
    );
    let instruction: AtaInstruction =
        risc0_zkvm::serde::from_slice(&chained_calls[2].instruction_data)
            .expect("ATA transfer parses");
    assert!(matches!(
        instruction,
        AtaInstruction::Transfer {
            ata_program_id: ATA_PROGRAM_ID,
            amount: 800,
        }
    ));
}

#[test]
fn namespace_fee_rate_is_below_one_hundred_percent_on_creation_and_update() {
    for (config, authority, initialize) in [
        (uninitialized_config(), namespace_creator_signer(), true),
        (
            namespace_config(NAMESPACE_CREATOR, 100, new_treasury()),
            namespace_creator_signer(),
            false,
        ),
    ] {
        let pre_states = vec![config, authority];
        let instruction = |protocol_fee_bps| {
            if initialize {
                Instruction::InitializeNamespace {
                    namespace: NAMESPACE_CREATOR,
                    protocol_fee_bps,
                    treasury: new_treasury(),
                }
            } else {
                Instruction::UpdateFee {
                    namespace: NAMESPACE_CREATOR,
                    protocol_fee_bps,
                }
            }
        };
        for protocol_fee_bps in [0, 9_999] {
            let (post_states, _) = process_instruction(
                pre_states.clone(),
                instruction(protocol_fee_bps),
                CURVE_PROGRAM_ID,
            );
            lee_core::program::validate_execution(&pre_states, &post_states, CURVE_PROGRAM_ID)
                .expect("configuration must pass runtime account validation");
            let config = Config::try_from(&post_states[0].account().data)
                .expect("post state holds a valid Config");
            assert_eq!(
                config.protocol_fee_bps,
                u16::try_from(protocol_fee_bps).expect("valid namespace fee fits in u16")
            );
        }
        for protocol_fee_bps in [10_000, 10_001, 65_536, u32::MAX] {
            let result = std::panic::catch_unwind(|| {
                process_instruction(
                    pre_states.clone(),
                    instruction(protocol_fee_bps),
                    CURVE_PROGRAM_ID,
                )
            });
            assert!(
                result.is_err(),
                "namespace fee {protocol_fee_bps} must be rejected"
            );
        }
    }
}

#[test]
fn namespace_fee_can_be_lowered_from_a_legacy_hundred_percent_rate() {
    let mut legacy_config = initialized_config(new_admin());
    legacy_config.account.data = Data::from(&Config::new(new_admin(), 10_000, new_treasury()));
    let pre_states = vec![legacy_config, admin_signer(new_admin())];
    let (post_states, _) = process_instruction(
        pre_states.clone(),
        Instruction::UpdateFee {
            namespace: NAMESPACE_CREATOR,
            protocol_fee_bps: 9_999,
        },
        CURVE_PROGRAM_ID,
    );
    lee_core::program::validate_execution(&pre_states, &post_states, CURVE_PROGRAM_ID)
        .expect("configuration must pass runtime account validation");
    let config =
        Config::try_from(&post_states[0].account().data).expect("post state holds a valid Config");
    assert_eq!(config.protocol_fee_bps, 9_999);
}

#[test]
fn pool_state_round_trips_with_ordered_tokens_owner_and_optional_expiry() {
    let pool_account = PoolAccount {
        allowed_input_token: None,
        funded: true,
        owner_program: None,
        namespace: AccountId::new([0xAD; 32]),
        token0_definition_id: AccountId::new([1; 32]),
        token1_definition_id: AccountId::new([2; 32]),
        owner: AccountId::new([3; 32]),

        pool: Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 25,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            close_timestamp: Some(42),
            close_on_depletion: None,
        })
        .expect("valid pool"),
    };
    let data = Data::from(&pool_account);
    assert_eq!(
        PoolAccount::try_from(&data).expect("data parses"),
        pool_account
    );
}

#[test]
fn stored_pool_owner_can_close_the_pool() {
    let owner = AccountId::new([3; 32]);
    let token0 = AccountId::new([1; 32]);
    let token1 = AccountId::new([2; 32]);
    let pool_account = PoolAccount {
        allowed_input_token: None,
        funded: true,
        owner_program: None,
        namespace: AccountId::new([0xAD; 32]),
        token0_definition_id: token0,
        token1_definition_id: token1,
        owner,
        pool: Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 25,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            close_timestamp: None,
            close_on_depletion: None,
        })
        .expect("valid pool"),
    };
    let pool = AccountWithMetadata {
        account: Account {
            program_owner: CURVE_PROGRAM_ID,
            data: Data::from(&pool_account),
            ..Account::default()
        },
        is_authorized: false,
        account_id: compute_pool_pda(
            AccountId::new([0xAD; 32]),
            CURVE_PROGRAM_ID,
            token0,
            token1,
            owner,
        ),
    };

    let mut pending = pool.clone();
    let mut pending_state = pool_account.clone();
    pending_state.funded = false;
    pending.account.data = Data::from(&pending_state);
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            vec![pending, signer(owner), trusted_clock(1)],
            Instruction::ClosePool,
            CURVE_PROGRAM_ID,
        ))
        .is_err(),
        "pending pools cannot close"
    );

    let [post, _, _]: [_; 3] = close_pool(pool, signer(owner), trusted_clock(1), CURVE_PROGRAM_ID)
        .try_into()
        .expect("one post state");
    let closed = PoolAccount::try_from(&post.account().data).expect("valid pool state");
    assert_eq!(closed.pool.lifecycle, pool::PoolLifecycle::Closed);
}

#[should_panic(expected = "Authority is not the pool owner")]
#[test]
fn an_unrelated_signer_cannot_close_the_pool() {
    let owner = AccountId::new([3; 32]);
    let token0 = AccountId::new([1; 32]);
    let token1 = AccountId::new([2; 32]);
    let pool_account = PoolAccount {
        allowed_input_token: None,
        funded: true,
        owner_program: None,
        namespace: AccountId::new([0xAD; 32]),
        token0_definition_id: token0,
        token1_definition_id: token1,
        owner,
        pool: Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 25,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            close_timestamp: None,
            close_on_depletion: None,
        })
        .expect("valid pool"),
    };
    let pool = AccountWithMetadata {
        account: Account {
            program_owner: CURVE_PROGRAM_ID,
            data: Data::from(&pool_account),
            ..Account::default()
        },
        is_authorized: false,
        account_id: compute_pool_pda(
            AccountId::new([0xAD; 32]),
            CURVE_PROGRAM_ID,
            token0,
            token1,
            owner,
        ),
    };
    let _ = close_pool(pool, intruder_signer(), trusted_clock(1), CURVE_PROGRAM_ID);
}

#[test]
fn expired_pool_owner_can_withdraw_both_reserves_without_closing_first() {
    let owner = AccountId::new([3; 32]);
    let token0 = AccountId::new([1; 32]);
    let token1 = AccountId::new([2; 32]);
    let pool_account = PoolAccount {
        allowed_input_token: None,
        funded: true,
        owner_program: None,
        namespace: AccountId::new([0xAD; 32]),
        token0_definition_id: token0,
        token1_definition_id: token1,
        owner,
        pool: Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 25,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            close_timestamp: Some(42),
            close_on_depletion: None,
        })
        .expect("valid pool"),
    };
    let pool = AccountWithMetadata {
        account: Account {
            program_owner: CURVE_PROGRAM_ID,
            data: Data::from(&pool_account),
            ..Account::default()
        },
        is_authorized: false,
        account_id: compute_pool_pda(
            AccountId::new([0xAD; 32]),
            CURVE_PROGRAM_ID,
            token0,
            token1,
            owner,
        ),
    };
    let clock = AccountWithMetadata {
        account: Account {
            data: Data::try_from(
                clock_core::ClockAccountData {
                    block_id: 7,
                    timestamp: 42,
                }
                .to_bytes(),
            )
            .expect("clock data fits"),
            ..Account::default()
        },
        is_authorized: false,
        account_id: clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
    };

    let (posts, withdrawn) = withdraw_reserves(pool, signer(owner), clock, CURVE_PROGRAM_ID);
    assert_eq!(
        (withdrawn.token0_amount, withdrawn.token1_amount),
        (800, 25)
    );
    let [post, _, _]: [_; 3] = posts.try_into().expect("one post state");
    let retired = PoolAccount::try_from(&post.account().data).expect("valid pool state");
    assert_eq!(retired.pool.lifecycle, pool::PoolLifecycle::Withdrawn);
}

#[test]
fn withdrawal_accepts_an_omitted_treasury_that_is_the_pool_collateral_vault() {
    let mut pool = namespace_pool(NAMESPACE_CREATOR);
    let mut state = PoolAccount::try_from(&pool.account.data).unwrap();
    state.pool.lifecycle = pool::PoolLifecycle::Closed;
    pool.account.data = Data::from(&state);
    let pre = vec![
        pool.clone(),
        signer(state.owner),
        holding_ata(state.owner, state.token0_definition_id, 0),
        holding_ata(state.owner, state.token1_definition_id, 0),
        holding_ata(pool.account_id, state.token0_definition_id, 800),
        holding_ata(pool.account_id, state.token1_definition_id, 100),
        trusted_clock(42),
        namespace_config(NAMESPACE_CREATOR, 100, pool.account_id),
    ];
    let (posts, calls) =
        process_instruction(pre.clone(), Instruction::WithdrawReserves, CURVE_PROGRAM_ID);
    lee_core::program::validate_execution(&pre, &posts, CURVE_PROGRAM_ID).unwrap();
    let withdrawn = PoolAccount::try_from(&posts[0].account().data).unwrap();
    assert_eq!(withdrawn.pool.lifecycle, pool::PoolLifecycle::Withdrawn);
    assert_eq!(posts.len(), 8);
    let amounts: Vec<u128> = calls
        .iter()
        .map(|call| {
            let instruction: AtaInstruction =
                risc0_zkvm::serde::from_slice(&call.instruction_data).unwrap();
            let AtaInstruction::Transfer { amount, .. } = instruction else {
                panic!("withdrawal must transfer reserves")
            };
            amount
        })
        .collect();
    assert_eq!(amounts, vec![800, 100]);
}

#[test]
fn withdrawal_leaves_collected_fees_in_the_pool_when_it_is_the_treasury() {
    let mut pool = namespace_pool(NAMESPACE_CREATOR);
    let mut state = PoolAccount::try_from(&pool.account.data).unwrap();
    state.pool.lifecycle = pool::PoolLifecycle::Closed;
    state.pool.fees_accrued = 7;
    pool.account.data = Data::from(&state);
    let pre = vec![
        pool.clone(),
        signer(state.owner),
        holding_ata(state.owner, state.token0_definition_id, 0),
        holding_ata(state.owner, state.token1_definition_id, 0),
        holding_ata(pool.account_id, state.token0_definition_id, 800),
        holding_ata(pool.account_id, state.token1_definition_id, 107),
        trusted_clock(42),
        namespace_config(NAMESPACE_CREATOR, 100, pool.account_id),
    ];
    let (posts, calls) =
        process_instruction(pre.clone(), Instruction::WithdrawReserves, CURVE_PROGRAM_ID);
    lee_core::program::validate_execution(&pre, &posts, CURVE_PROGRAM_ID).unwrap();
    let withdrawn = PoolAccount::try_from(&posts[0].account().data).unwrap();
    assert_eq!(withdrawn.pool.fees_accrued, 0);
    assert_eq!(withdrawn.pool.fees_collected, 7);
    assert_eq!(calls.len(), 2, "fees already at treasury need no transfer");
    let instruction: AtaInstruction =
        risc0_zkvm::serde::from_slice(&calls[1].instruction_data).unwrap();
    assert!(matches!(
        instruction,
        AtaInstruction::Transfer { amount: 100, .. }
    ));
    assert_eq!(
        TokenHolding::try_from(&calls[1].pre_states[1].account.data).unwrap(),
        TokenHolding::Fungible {
            definition_id: state.token1_definition_id,
            balance: 107
        }
    );
}

#[test]
fn withdraw_dispatch_transfers_both_real_reserves_and_retires_the_pool() {
    let owner = AccountId::new([3; 32]);
    let token0 = AccountId::new([1; 32]);
    let token1 = AccountId::new([2; 32]);
    let pool_id = compute_pool_pda(
        AccountId::new([0xAD; 32]),
        CURVE_PROGRAM_ID,
        token0,
        token1,
        owner,
    );
    let mut pool = AccountWithMetadata {
        account: Account {
            program_owner: CURVE_PROGRAM_ID,
            data: Data::from(&PoolAccount {
                allowed_input_token: None,
                funded: true,
                owner_program: None,
                namespace: AccountId::new([0xAD; 32]),
                token0_definition_id: token0,
                token1_definition_id: token1,
                owner,
                pool: Pool::create(PoolCreateParams {
                    token0_amount: 800,
                    token1_amount: 100,
                    virtual_reserve0: 1000,
                    virtual_reserve1: 100,
                    close_timestamp: Some(42),
                    close_on_depletion: None,
                })
                .expect("valid pool"),
            }),
            ..Account::default()
        },
        is_authorized: false,
        account_id: pool_id,
    };
    let mut state = PoolAccount::try_from(&pool.account.data).unwrap();
    state.pool.fees_accrued = 3;
    pool.account.data = Data::from(&state);
    let pool_token0 = holding_ata(pool_id, token0, 800);
    let pool_token1 = holding_ata(pool_id, token1, 103);
    let owner_token0 = holding_ata(owner, token0, 0);
    let owner_token1 = holding_ata(owner, token1, 0);

    let pre = vec![
        pool,
        signer(owner),
        owner_token0.clone(),
        owner_token1.clone(),
        pool_token0.clone(),
        pool_token1.clone(),
        trusted_clock(42),
        initialized_config(new_admin()),
        holding_ata(new_treasury(), token1, 0),
    ];
    let mut pending = pre.clone();
    let mut pending_state = PoolAccount::try_from(&pending[0].account.data).unwrap();
    pending_state.funded = false;
    pending[0].account.data = Data::from(&pending_state);
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            pending,
            Instruction::WithdrawReserves,
            CURVE_PROGRAM_ID
        ))
        .is_err(),
        "pending pools cannot settle"
    );
    let (posts, calls) =
        process_instruction(pre.clone(), Instruction::WithdrawReserves, CURVE_PROGRAM_ID);

    lee_core::program::validate_execution(&pre, &posts, CURVE_PROGRAM_ID)
        .expect("complete account outputs");
    for call in &calls {
        if call.pre_states[0].account_id == pre[0].account_id {
            assert_eq!(
                &call.pre_states[0].account,
                posts[0].account(),
                "chained pool authority uses updated state"
            );
        }
    }

    let [post, ..]: [_; 9] = posts
        .try_into()
        .expect("only the pool state changes directly");
    let retired = PoolAccount::try_from(&post.account().data).expect("valid pool state");
    assert_eq!(retired.pool.lifecycle, pool::PoolLifecycle::Withdrawn);
    assert_eq!(
        (retired.pool.real_reserve0, retired.pool.real_reserve1),
        (0, 0)
    );
    assert_eq!(retired.pool.fees_accrued, 0);
    assert_eq!(retired.pool.fees_collected, 3);
    assert_eq!(calls.len(), 3, "fee collection and both reserves");
    let TokenHolding::Fungible { balance, .. } =
        TokenHolding::try_from(&calls[2].pre_states[1].account.data).unwrap()
    else {
        panic!("fungible")
    };
    assert_eq!(balance, 100, "principal transfer sees the fee debit");
    let transfers: Vec<(AccountId, AccountId, AccountId, u128)> = calls
        .iter()
        .map(|call| {
            let instruction: AtaInstruction = risc0_zkvm::serde::from_slice(&call.instruction_data)
                .expect("ATA instruction parses");
            let AtaInstruction::Transfer { amount, .. } = instruction else {
                panic!("withdrawal contains only transfers");
            };
            (
                call.pre_states[0].account_id,
                call.pre_states[1].account_id,
                call.pre_states[2].account_id,
                amount,
            )
        })
        .collect();
    assert_eq!(
        transfers,
        vec![
            (pool_id, pool_token1.account_id, pre[8].account_id, 3),
            (
                pool_id,
                pool_token0.account_id,
                owner_token0.account_id,
                800
            ),
            (
                pool_id,
                pool_token1.account_id,
                owner_token1.account_id,
                100
            ),
        ]
    );
}

#[test]
fn exact_input_sell_charges_the_protocol_fee_in_collateral() {
    let owner = AccountId::new([3; 32]);
    let token0 = AccountId::new([1; 32]);
    let token1 = AccountId::new([2; 32]);
    let pool_account = PoolAccount {
        allowed_input_token: None,
        funded: true,
        owner_program: None,
        namespace: AccountId::new([0xAD; 32]),
        token0_definition_id: token0,
        token1_definition_id: token1,
        owner,
        pool: Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 100,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            close_timestamp: None,
            close_on_depletion: None,
        })
        .expect("valid pool"),
    };
    let pool = AccountWithMetadata {
        account: Account {
            program_owner: CURVE_PROGRAM_ID,
            data: Data::from(&pool_account),
            ..Account::default()
        },
        is_authorized: false,
        account_id: compute_pool_pda(
            AccountId::new([0xAD; 32]),
            CURVE_PROGRAM_ID,
            token0,
            token1,
            owner,
        ),
    };

    let (posts, settlement) = swap_exact_input(
        pool,
        initialized_config(new_admin()),
        None,
        250,
        19,
        token0,
        CURVE_PROGRAM_ID,
    );
    assert_eq!(settlement.token_in, token0);
    assert_eq!(settlement.token_out, token1);
    assert_eq!(settlement.effective_amount_in, 250);
    assert_eq!(settlement.raw_amount_out, 20);
    assert_eq!(settlement.protocol_fee, 1);
    assert_eq!(settlement.protocol_fee_side, FeeSide::Output);
    assert_eq!(settlement.treasury, new_treasury());
    let [post]: [_; 1] = posts.try_into().expect("one post state");
    let updated = PoolAccount::try_from(&post.account().data).expect("valid pool state");
    assert_eq!(
        (updated.pool.real_reserve0, updated.pool.real_reserve1),
        (1050, 80)
    );
}

#[test]
fn token0_to_token1_exact_input_accrues_fee_with_two_transfers() {
    let owner = AccountId::new([3; 32]);
    let participant = AccountId::new([4; 32]);
    let token0 = AccountId::new([1; 32]);
    let token1 = AccountId::new([2; 32]);
    let pool_id = compute_pool_pda(
        AccountId::new([0xAD; 32]),
        CURVE_PROGRAM_ID,
        token0,
        token1,
        owner,
    );
    let pool = AccountWithMetadata {
        account: Account {
            program_owner: CURVE_PROGRAM_ID,
            data: Data::from(&PoolAccount {
                allowed_input_token: None,
                funded: true,
                owner_program: None,
                namespace: AccountId::new([0xAD; 32]),
                token0_definition_id: token0,
                token1_definition_id: token1,
                owner,
                pool: Pool::create(PoolCreateParams {
                    token0_amount: 800,
                    token1_amount: 100,
                    virtual_reserve0: 1000,
                    virtual_reserve1: 100,
                    close_timestamp: Some(42),
                    close_on_depletion: None,
                })
                .expect("valid pool"),
            }),
            ..Account::default()
        },
        is_authorized: false,
        account_id: pool_id,
    };
    let participant_token0 = holding_ata(participant, token0, 1_000);
    let participant_token1 = holding_ata(participant, token1, 0);
    let pool_token0 = holding_ata(pool_id, token0, 800);
    let pool_token1 = holding_ata(pool_id, token1, 100);

    let pre = vec![
        pool,
        initialized_config(new_admin()),
        signer(participant),
        participant_token0.clone(),
        pool_token0.clone(),
        pool_token1.clone(),
        participant_token1.clone(),
        trusted_clock(41),
    ];
    let mut pending = pre.clone();
    let mut pending_state = PoolAccount::try_from(&pending[0].account.data).unwrap();
    pending_state.funded = false;
    pending[0].account.data = Data::from(&pending_state);
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            pending,
            Instruction::SwapExactInput {
                amount_in: 250,
                min_amount_out: 19,
                token_in: token0
            },
            CURVE_PROGRAM_ID
        ))
        .is_err(),
        "pending pools cannot settle"
    );
    let (posts, calls) = process_instruction(
        pre.clone(),
        Instruction::SwapExactInput {
            amount_in: 250,
            min_amount_out: 19,
            token_in: token0,
        },
        CURVE_PROGRAM_ID,
    );

    lee_core::program::validate_execution(&pre, &posts, CURVE_PROGRAM_ID)
        .expect("complete account outputs");
    for call in &calls {
        if call.pre_states[0].account_id == pre[0].account_id {
            assert_eq!(
                &call.pre_states[0].account,
                posts[0].account(),
                "chained pool authority uses updated state"
            );
        }
    }

    let [post, ..]: [_; 8] = posts
        .try_into()
        .expect("only the pool state changes directly");
    let updated = PoolAccount::try_from(&post.account().data).expect("valid pool state");
    assert_eq!(
        (updated.pool.real_reserve0, updated.pool.real_reserve1),
        (1050, 80)
    );
    assert_eq!(updated.pool.fees_accrued, 1);
    assert_eq!(calls.len(), 2, "input and net output transfers");
    for call in &calls {
        assert_eq!(call.program_id, ATA_PROGRAM_ID);
    }
    assert!(
        calls[1].pre_states[0].is_authorized,
        "the pool must authorize the output transfer"
    );
    assert_eq!(
        calls[1].pda_seeds,
        vec![crate::compute_pool_pda_seed(
            AccountId::new([0xAD; 32]),
            token0,
            token1,
            owner
        )],
        "the pool must prove its PDA authority for the output transfer"
    );
    let transfers: Vec<(AccountId, AccountId, AccountId, u128)> = calls
        .iter()
        .map(|call| {
            let instruction: AtaInstruction = risc0_zkvm::serde::from_slice(&call.instruction_data)
                .expect("ATA instruction parses");
            let AtaInstruction::Transfer { amount, .. } = instruction else {
                panic!("swap settlement contains only transfers");
            };
            (
                call.pre_states[0].account_id,
                call.pre_states[1].account_id,
                call.pre_states[2].account_id,
                amount,
            )
        })
        .collect();
    assert_eq!(
        transfers,
        vec![
            (
                participant,
                participant_token0.account_id,
                pool_token0.account_id,
                250
            ),
            (
                pool_id,
                pool_token1.account_id,
                participant_token1.account_id,
                19
            ),
        ]
    );
}

#[test]
fn exact_output_handler_caps_fee_inclusive_input_in_the_reverse_direction() {
    let owner = AccountId::new([3; 32]);
    let token0 = AccountId::new([1; 32]);
    let token1 = AccountId::new([2; 32]);
    let pool_account = PoolAccount {
        allowed_input_token: None,
        funded: true,
        owner_program: None,
        namespace: AccountId::new([0xAD; 32]),
        token0_definition_id: token0,
        token1_definition_id: token1,
        owner,
        pool: Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 100,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            close_timestamp: None,
            close_on_depletion: None,
        })
        .expect("valid pool"),
    };
    let pool = AccountWithMetadata {
        account: Account {
            program_owner: CURVE_PROGRAM_ID,
            data: Data::from(&pool_account),
            ..Account::default()
        },
        is_authorized: false,
        account_id: compute_pool_pda(
            AccountId::new([0xAD; 32]),
            CURVE_PROGRAM_ID,
            token0,
            token1,
            owner,
        ),
    };
    let config = AccountWithMetadata {
        account: Account {
            program_owner: CURVE_PROGRAM_ID,
            data: Data::from(&Config::new(new_admin(), 1_000, new_treasury())),
            ..Account::default()
        },
        is_authorized: false,
        account_id: compute_config_pda(AccountId::new([0xAD; 32]), CURVE_PROGRAM_ID),
    };

    let (_, settlement) = swap_exact_output(pool, config, None, 200, 28, token1, CURVE_PROGRAM_ID);
    assert_eq!(settlement.token_in, token1);
    assert_eq!(settlement.token_out, token0);
    assert_eq!(
        (
            settlement.amount_in,
            settlement.amount_out,
            settlement.protocol_fee,
            settlement.raw_amount_out,
        ),
        (28, 200, 3, 200)
    );
}

#[test]
fn exact_output_dispatch_settles_fee_inclusive_input_and_requested_output_atomically() {
    let owner = AccountId::new([3; 32]);
    let participant = AccountId::new([4; 32]);
    let token0 = AccountId::new([1; 32]);
    let token1 = AccountId::new([2; 32]);
    let pool_id = compute_pool_pda(
        AccountId::new([0xAD; 32]),
        CURVE_PROGRAM_ID,
        token0,
        token1,
        owner,
    );
    let pool = AccountWithMetadata {
        account: Account {
            program_owner: CURVE_PROGRAM_ID,
            data: Data::from(&PoolAccount {
                allowed_input_token: None,
                funded: true,
                owner_program: None,
                namespace: AccountId::new([0xAD; 32]),
                token0_definition_id: token0,
                token1_definition_id: token1,
                owner,
                pool: Pool::create(PoolCreateParams {
                    token0_amount: 800,
                    token1_amount: 100,
                    virtual_reserve0: 1000,
                    virtual_reserve1: 100,
                    close_timestamp: Some(42),
                    close_on_depletion: None,
                })
                .expect("valid pool"),
            }),
            ..Account::default()
        },
        is_authorized: false,
        account_id: pool_id,
    };
    let participant_token0 = holding_ata(participant, token0, 0);
    let participant_token1 = holding_ata(participant, token1, 1_000);
    let pool_token0 = holding_ata(pool_id, token0, 800);
    let pool_token1 = holding_ata(pool_id, token1, 100);
    let config = AccountWithMetadata {
        account: Account {
            program_owner: CURVE_PROGRAM_ID,
            data: Data::from(&Config::new(new_admin(), 1_000, new_treasury())),
            ..Account::default()
        },
        is_authorized: false,
        account_id: compute_config_pda(AccountId::new([0xAD; 32]), CURVE_PROGRAM_ID),
    };

    let pre = vec![
        pool,
        config,
        signer(participant),
        participant_token1.clone(),
        pool_token1.clone(),
        pool_token0.clone(),
        participant_token0.clone(),
        trusted_clock(41),
    ];
    let mut pending = pre.clone();
    let mut pending_state = PoolAccount::try_from(&pending[0].account.data).unwrap();
    pending_state.funded = false;
    pending[0].account.data = Data::from(&pending_state);
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            pending,
            Instruction::SwapExactOutput {
                amount_out: 200,
                max_amount_in: 28,
                token_in: token1
            },
            CURVE_PROGRAM_ID
        ))
        .is_err(),
        "pending pools cannot settle"
    );
    let (posts, calls) = process_instruction(
        pre.clone(),
        Instruction::SwapExactOutput {
            amount_out: 200,
            max_amount_in: 28,
            token_in: token1,
        },
        CURVE_PROGRAM_ID,
    );

    lee_core::program::validate_execution(&pre, &posts, CURVE_PROGRAM_ID)
        .expect("complete account outputs");
    for call in &calls {
        if call.pre_states[0].account_id == pre[0].account_id {
            assert_eq!(
                &call.pre_states[0].account,
                posts[0].account(),
                "chained pool authority uses updated state"
            );
        }
    }

    let [post, ..]: [_; 8] = posts
        .try_into()
        .expect("only the pool state changes directly");
    let updated = PoolAccount::try_from(&post.account().data).expect("valid pool state");
    assert_eq!(
        (updated.pool.real_reserve0, updated.pool.real_reserve1),
        (600, 125)
    );
    assert_eq!(
        calls.len(),
        2,
        "gross collateral and requested output transfers"
    );
    let amounts: Vec<u128> = calls
        .iter()
        .map(|call| {
            let instruction: AtaInstruction = risc0_zkvm::serde::from_slice(&call.instruction_data)
                .expect("ATA instruction parses");
            let AtaInstruction::Transfer { amount, .. } = instruction else {
                panic!("swap settlement contains only transfers");
            };
            amount
        })
        .collect();
    assert_eq!(updated.pool.fees_accrued, 3);
    assert_eq!(amounts, vec![28, 200]);
    assert_eq!(calls[0].pre_states[0].account_id, participant);
    assert_eq!(
        calls[0].pre_states[1].account_id,
        participant_token1.account_id
    );
    assert_eq!(calls[0].pre_states[2].account_id, pool_token1.account_id);
    assert_eq!(calls[1].pre_states[0].account_id, pool_id);
    assert_eq!(calls[1].pre_states[1].account_id, pool_token0.account_id);
    assert_eq!(
        calls[1].pre_states[2].account_id,
        participant_token0.account_id
    );
}

#[test]
fn instruction_wire_format_matches_the_idl_order() {
    // `read_lee_inputs::<Instruction>()` in the guest deserialises with risc0's serde.
    let instructions = [
        Instruction::RenounceAdmin {
            namespace: AccountId::new([0xAD; 32]),
        },
        Instruction::CreatePool {
            allowed_input_token: None,
            defer_funding: false,
            namespace: AccountId::new([0xAD; 32]),
            token0_amount: 800,
            token1_amount: 25,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            close_timestamp: Some(42),
            close_on_depletion: Some(0),
            owner: AccountId::new([3; 32]),
            owner_program_id: Some([8; 8]),
            owner_program_seed: Some([9; 32]),
            curve_program_id: [7; 8],
        },
        Instruction::SwapExactInput {
            amount_in: 25,
            min_amount_out: 190,
            token_in: AccountId::new([1; 32]),
        },
        Instruction::SwapExactOutput {
            amount_out: 250,
            max_amount_in: 18,
            token_in: AccountId::new([2; 32]),
        },
        Instruction::ClosePool,
        Instruction::ActivatePool,
        Instruction::WithdrawReserves,
        Instruction::CollectFees,
        Instruction::SetFrozen {
            namespace: AccountId::new([0xAD; 32]),
            frozen: true,
        },
        Instruction::TransferEmergencyAuthority {
            namespace: AccountId::new([0xAD; 32]),
            new_authority: new_admin(),
        },
        Instruction::InitializeNamespace {
            namespace: AccountId::new([0xAD; 32]),
            protocol_fee_bps: 100,
            treasury: new_treasury(),
        },
        Instruction::UpdateFee {
            namespace: AccountId::new([0xAD; 32]),
            protocol_fee_bps: 100,
        },
        Instruction::UpdateTreasury {
            namespace: AccountId::new([0xAD; 32]),
            treasury: new_treasury(),
        },
        Instruction::TransferAdmin {
            namespace: AccountId::new([0xAD; 32]),
            new_admin: new_admin(),
            candidate: AdminCandidate::Signer,
        },
        Instruction::Private {
            instruction: Box::new(Instruction::ClosePool),
        },
    ];
    for (tag, instruction) in (0_u32..=14).zip(instructions) {
        let words = risc0_zkvm::serde::to_vec(&instruction).expect("instruction serialises");
        assert_eq!(words.first().copied(), Some(tag));
        let back: Instruction = risc0_zkvm::serde::from_slice(&words).expect("wire words parse");
        assert_eq!(back, instruction);
    }
}

fn valid_create_pool_instruction(
    close_on_depletion: Option<u8>,
    owner_program_id: Option<ProgramId>,
    owner_program_seed: Option<[u8; 32]>,
) -> Instruction {
    Instruction::CreatePool {
        allowed_input_token: None,
        defer_funding: false,
        namespace: NAMESPACE_CREATOR,
        token0_amount: 800,
        token1_amount: 25,
        virtual_reserve0: 1_000,
        virtual_reserve1: 100,
        close_timestamp: Some(42),
        close_on_depletion,
        owner: AccountId::new([3; 32]),
        owner_program_id,
        owner_program_seed,
        curve_program_id: CURVE_PROGRAM_ID,
    }
}

#[test]
fn wire_values_reject_unrepresentable_fee_invalid_depletion_and_incomplete_owner_program() {
    let result = std::panic::catch_unwind(|| {
        process_instruction(
            vec![uninitialized_config(), namespace_creator_signer()],
            Instruction::InitializeNamespace {
                namespace: NAMESPACE_CREATOR,
                protocol_fee_bps: u32::from(u16::MAX) + 1,
                treasury: new_treasury(),
            },
            CURVE_PROGRAM_ID,
        )
    });
    assert!(result.is_err());

    let result = std::panic::catch_unwind(|| {
        process_instruction(
            vec![uninitialized_config(), namespace_creator_signer()],
            Instruction::InitializeNamespace {
                namespace: NAMESPACE_CREATOR,
                protocol_fee_bps: u32::from(crate::MAX_FEE_BPS) + 1,
                treasury: new_treasury(),
            },
            CURVE_PROGRAM_ID,
        )
    });
    assert!(result.is_err());

    for instruction in [
        valid_create_pool_instruction(Some(2), None, None),
        valid_create_pool_instruction(None, Some([8; 8]), None),
        valid_create_pool_instruction(None, None, Some([9; 32])),
    ] {
        let accounts = valid_create_pool_accounts();
        let pre_states = vec![
            accounts.pool,
            accounts.owner,
            accounts.token0_definition,
            accounts.token1_definition,
            accounts.owner_token0_ata,
            accounts.owner_token1_ata,
            accounts.pool_token0_ata,
            accounts.pool_token1_ata,
            trusted_clock(1),
            initialized_config(NAMESPACE_CREATOR),
        ];
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                pre_states,
                instruction,
                CURVE_PROGRAM_ID,
            ))
            .is_err()
        );
    }

    let owner_program_id = [8; 8];
    let owner_program_seed = [9; 32];
    let owner = AccountId::for_public_pda(
        &owner_program_id,
        &lee_core::program::PdaSeed::new(owner_program_seed),
    );
    let token0_definition = token_definition(AccountId::new([4; 32]));
    let token1_definition = token_definition(AccountId::new([5; 32]));
    let pool_id = compute_pool_pda(
        NAMESPACE_CREATOR,
        CURVE_PROGRAM_ID,
        token0_definition.account_id,
        token1_definition.account_id,
        owner,
    );
    let mut instruction =
        valid_create_pool_instruction(Some(1), Some(owner_program_id), Some(owner_program_seed));
    let Instruction::CreatePool {
        owner: wire_owner, ..
    } = &mut instruction
    else {
        unreachable!("fixture constructs a pool instruction");
    };
    *wire_owner = owner;
    let (posts, _) = process_instruction(
        vec![
            AccountWithMetadata {
                account_id: pool_id,
                ..uninitialized_config()
            },
            AccountWithMetadata {
                account: Account {
                    program_owner: owner_program_id,
                    ..Account::default()
                },
                account_id: owner,
                is_authorized: true,
            },
            token0_definition.clone(),
            token1_definition.clone(),
            owner_source_ata(owner, token0_definition.account_id),
            owner_source_ata(owner, token1_definition.account_id),
            ata(pool_id, token0_definition.account_id, Account::default()),
            ata(pool_id, token1_definition.account_id, Account::default()),
            trusted_clock(1),
            initialized_config(NAMESPACE_CREATOR),
        ],
        instruction,
        CURVE_PROGRAM_ID,
    );
    assert_eq!(
        PoolAccount::try_from(&posts[0].account().data)
            .expect("pool state parses")
            .owner_program,
        Some((owner_program_id, owner_program_seed))
    );
}

#[test]
fn pool_creation_validates_and_saves_the_neutral_input_restriction() {
    for wire_side in [None, Some(0), Some(1), Some(2), Some(u8::MAX)] {
        let accounts = valid_create_pool_accounts();
        let pre = vec![
            accounts.pool,
            accounts.owner,
            accounts.token0_definition,
            accounts.token1_definition,
            accounts.owner_token0_ata,
            accounts.owner_token1_ata,
            accounts.pool_token0_ata,
            accounts.pool_token1_ata,
            trusted_clock(1),
            initialized_config(NAMESPACE_CREATOR),
        ];
        let mut instruction = valid_create_pool_instruction(None, None, None);
        let Instruction::CreatePool {
            allowed_input_token,
            ..
        } = &mut instruction
        else {
            unreachable!()
        };
        *allowed_input_token = wire_side;
        let result =
            std::panic::catch_unwind(|| process_instruction(pre, instruction, CURVE_PROGRAM_ID));
        let expected = match wire_side {
            None => None,
            Some(0) => Some(pool::TokenSide::Token0),
            Some(1) => Some(pool::TokenSide::Token1),
            Some(_) => {
                assert!(result.is_err());
                continue;
            }
        };
        let (posts, _) = result.expect("valid restriction");
        assert_eq!(
            PoolAccount::try_from(&posts[0].account().data)
                .unwrap()
                .allowed_input_token,
            expected
        );
    }
}

#[test]
fn the_pool_pda_hashes_the_pair_in_fixed_order() {
    let owner = AccountId::new([3; 32]);
    let token0 = AccountId::new([1; 32]);
    let token1 = AccountId::new([2; 32]);
    assert_ne!(
        compute_pool_pda(
            AccountId::new([0xAD; 32]),
            CURVE_PROGRAM_ID,
            token0,
            token1,
            owner
        ),
        compute_pool_pda(
            AccountId::new([0xAD; 32]),
            CURVE_PROGRAM_ID,
            token1,
            token0,
            owner
        )
    );
}

#[test]
fn different_owners_have_distinct_pool_pdas_for_the_same_pair() {
    let token0 = AccountId::new([1; 32]);
    let token1 = AccountId::new([2; 32]);
    assert_ne!(
        compute_pool_pda(
            AccountId::new([0xAD; 32]),
            CURVE_PROGRAM_ID,
            token0,
            token1,
            AccountId::new([3; 32]),
        ),
        compute_pool_pda(
            AccountId::new([0xAD; 32]),
            CURVE_PROGRAM_ID,
            token0,
            token1,
            AccountId::new([4; 32]),
        )
    );
}

// Two independent operators on the same program image. These helpers initialize through
// the public dispatcher so the tests exercise the permissionless creation gate as well.
fn namespace_config(namespace: AccountId, fee: u16, treasury: AccountId) -> AccountWithMetadata {
    let empty = AccountWithMetadata {
        account: Account::default(),
        is_authorized: false,
        account_id: compute_config_pda(namespace, CURVE_PROGRAM_ID),
    };
    let (posts, calls) = process_instruction(
        vec![empty, admin_signer(namespace)],
        Instruction::InitializeNamespace {
            namespace,
            protocol_fee_bps: u32::from(fee),
            treasury,
        },
        CURVE_PROGRAM_ID,
    );
    assert!(calls.is_empty());
    let mut account = posts[0].account().clone();
    account.program_owner = CURVE_PROGRAM_ID;
    AccountWithMetadata {
        account,
        is_authorized: false,
        account_id: compute_config_pda(namespace, CURVE_PROGRAM_ID),
    }
}

fn namespace_pool(namespace: AccountId) -> AccountWithMetadata {
    let state = PoolAccount {
        allowed_input_token: None,
        funded: true,
        owner_program: None,
        namespace,
        token0_definition_id: AccountId::new([1; 32]),
        token1_definition_id: AccountId::new([2; 32]),
        owner: AccountId::new([3; 32]),

        pool: Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 100,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            close_timestamp: None,
            close_on_depletion: None,
        })
        .unwrap(),
    };
    AccountWithMetadata {
        account: Account {
            program_owner: CURVE_PROGRAM_ID,
            data: Data::from(&state),
            ..Account::default()
        },
        is_authorized: false,
        account_id: compute_pool_pda(
            namespace,
            CURVE_PROGRAM_ID,
            state.token0_definition_id,
            state.token1_definition_id,
            state.owner,
        ),
    }
}

#[test]
fn permissionless_namespaces_have_independent_addresses_fees_and_updates() {
    let a = AccountId::new([51; 32]);
    let b = AccountId::new([52; 32]);
    let config_a = namespace_config(a, 100, AccountId::new([61; 32]));
    let config_b = namespace_config(b, 1000, AccountId::new([62; 32]));
    let pool_a = namespace_pool(a);
    let pool_b = namespace_pool(b);
    assert_ne!(config_a.account_id, config_b.account_id);
    assert_ne!(pool_a.account_id, pool_b.account_id);
    let quote = |pool, config| {
        swap_exact_input(
            pool,
            config,
            None,
            25,
            0,
            AccountId::new([2; 32]),
            CURVE_PROGRAM_ID,
        )
        .1
    };
    let before_b = quote(pool_b.clone(), config_b.clone());
    assert_eq!(quote(pool_a.clone(), config_a.clone()).protocol_fee, 1);
    assert_eq!(before_b.protocol_fee, 3);
    let (posts, _) = process_instruction(
        vec![config_a.clone(), admin_signer(a)],
        Instruction::UpdateFee {
            namespace: a,
            protocol_fee_bps: 2000,
        },
        CURVE_PROGRAM_ID,
    );
    let updated_a = AccountWithMetadata {
        account: posts[0].account().clone(),
        ..config_a
    };
    let (posts, _) = process_instruction(
        vec![updated_a.clone(), admin_signer(a)],
        Instruction::UpdateTreasury {
            namespace: a,
            treasury: AccountId::new([63; 32]),
        },
        CURVE_PROGRAM_ID,
    );
    let updated_a = AccountWithMetadata {
        account: posts[0].account().clone(),
        ..updated_a
    };
    let after_a = quote(pool_a, updated_a);
    assert_eq!(after_a.protocol_fee, 5);
    assert_eq!(after_a.treasury, AccountId::new([63; 32]));
    assert_eq!(quote(pool_b, config_b), before_b);
}

#[test]
#[should_panic(expected = "Config account ID does not match PDA")]
fn swap_rejects_another_namespaces_config() {
    let _ = swap_exact_input(
        namespace_pool(AccountId::new([51; 32])),
        namespace_config(AccountId::new([52; 32]), 0, new_treasury()),
        None,
        25,
        0,
        AccountId::new([2; 32]),
        CURVE_PROGRAM_ID,
    );
}

#[test]
fn transfer_then_renounce_preserves_trading_but_permanently_removes_admin_power() {
    let namespace = AccountId::new([51; 32]);
    let successor = AccountId::new([52; 32]);
    let config = namespace_config(namespace, 100, new_treasury());
    let (posts, _) = process_instruction(
        vec![
            config.clone(),
            admin_signer(namespace),
            admin_signer(successor),
        ],
        Instruction::TransferAdmin {
            namespace,
            new_admin: successor,
            candidate: AdminCandidate::Signer,
        },
        CURVE_PROGRAM_ID,
    );
    let transferred = AccountWithMetadata {
        account: posts[0].account().clone(),
        ..config
    };
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            vec![transferred.clone(), admin_signer(namespace)],
            Instruction::UpdateFee {
                namespace,
                protocol_fee_bps: 0,
            },
            CURVE_PROGRAM_ID
        ))
        .is_err()
    );
    let pre_states = vec![transferred.clone(), admin_signer(successor)];
    let (posts, _) = process_instruction(
        pre_states.clone(),
        Instruction::RenounceAdmin { namespace },
        CURVE_PROGRAM_ID,
    );
    lee_core::program::validate_execution(&pre_states, &posts, CURVE_PROGRAM_ID)
        .expect("renunciation must pass runtime account validation");
    let renounced = AccountWithMetadata {
        account: posts[0].account().clone(),
        ..transferred
    };
    let state = Config::try_from(&renounced.account.data).unwrap();
    assert!(std::panic::catch_unwind(|| state.assert_admin(&admin_signer(successor))).is_err());
    assert_eq!(
        swap_exact_input(
            namespace_pool(namespace),
            renounced.clone(),
            None,
            25,
            0,
            AccountId::new([2; 32]),
            CURVE_PROGRAM_ID
        )
        .1
        .protocol_fee,
        1
    );
    for actor in [namespace, successor, AccountId::default()] {
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                vec![renounced.clone(), admin_signer(actor)],
                Instruction::UpdateFee {
                    namespace,
                    protocol_fee_bps: 0,
                },
                CURVE_PROGRAM_ID
            ))
            .is_err()
        );
    }
}

#[test]
fn execution_fee_update_respects_buy_and_sell_slippage() {
    let namespace = AccountId::new([51; 32]);
    let config = namespace_config(namespace, 0, new_treasury());
    let pool = namespace_pool(namespace);
    let token = AccountId::new([1; 32]);
    let collateral = AccountId::new([2; 32]);
    let buy = swap_exact_input(
        pool.clone(),
        config.clone(),
        None,
        25,
        0,
        collateral,
        CURVE_PROGRAM_ID,
    )
    .1;
    let sell = swap_exact_input(
        pool.clone(),
        config.clone(),
        None,
        250,
        0,
        token,
        CURVE_PROGRAM_ID,
    )
    .1;
    let (posts, _) = process_instruction(
        vec![config.clone(), admin_signer(namespace)],
        Instruction::UpdateFee {
            namespace,
            protocol_fee_bps: 1000,
        },
        CURVE_PROGRAM_ID,
    );
    let updated = AccountWithMetadata {
        account: posts[0].account().clone(),
        ..config
    };
    assert!(
        std::panic::catch_unwind(|| swap_exact_input(
            pool.clone(),
            updated.clone(),
            None,
            25,
            buy.amount_out,
            collateral,
            CURVE_PROGRAM_ID
        ))
        .is_err()
    );
    assert!(
        std::panic::catch_unwind(|| swap_exact_input(
            pool.clone(),
            updated.clone(),
            None,
            250,
            sell.amount_out,
            token,
            CURVE_PROGRAM_ID
        ))
        .is_err()
    );
}

#[test]
#[should_panic(expected = "Input token is restricted")]
fn restricted_pool_rejects_selling_through_public_swap() {
    let mut pool = namespace_pool(NAMESPACE_CREATOR);
    let mut state = PoolAccount::try_from(&pool.account.data).unwrap();
    state.allowed_input_token = Some(pool::TokenSide::Token1);
    pool.account.data = Data::from(&state);
    let participant = AccountId::new([4; 32]);
    let accounts = vec![
        pool.clone(),
        initialized_config(new_admin()),
        signer(participant),
        holding_ata(participant, state.token0_definition_id, 250),
        holding_ata(pool.account_id, state.token0_definition_id, 800),
        holding_ata(pool.account_id, state.token1_definition_id, 100),
        holding_ata(participant, state.token1_definition_id, 0),
        trusted_clock(1),
    ];
    let _ = process_instruction(
        accounts,
        Instruction::SwapExactInput {
            amount_in: 250,
            min_amount_out: 19,
            token_in: state.token0_definition_id,
        },
        CURVE_PROGRAM_ID,
    );
}

#[test]
fn neutral_input_restrictions_apply_to_both_public_buy_forms() {
    let participant = AccountId::new([4; 32]);
    for restriction in [
        None,
        Some(pool::TokenSide::Token1),
        Some(pool::TokenSide::Token0),
    ] {
        let mut pool = namespace_pool(NAMESPACE_CREATOR);
        let mut state = PoolAccount::try_from(&pool.account.data).unwrap();
        state.allowed_input_token = restriction;
        pool.account.data = Data::from(&state);
        let pre = vec![
            pool.clone(),
            namespace_config(NAMESPACE_CREATOR, 0, new_treasury()),
            signer(participant),
            holding_ata(participant, state.token1_definition_id, 25),
            holding_ata(pool.account_id, state.token1_definition_id, 100),
            holding_ata(pool.account_id, state.token0_definition_id, 800),
            holding_ata(participant, state.token0_definition_id, 0),
            trusted_clock(1),
        ];
        for instruction in [
            Instruction::SwapExactInput {
                amount_in: 25,
                min_amount_out: 200,
                token_in: state.token1_definition_id,
            },
            Instruction::SwapExactOutput {
                amount_out: 200,
                max_amount_in: 25,
                token_in: state.token1_definition_id,
            },
        ] {
            let result = std::panic::catch_unwind(|| {
                process_instruction(pre.clone(), instruction, CURVE_PROGRAM_ID)
            });
            if restriction == Some(pool::TokenSide::Token0) {
                let panic = result.expect_err("opposite input must be restricted");
                assert_eq!(
                    panic
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| panic.downcast_ref::<&str>().copied()),
                    Some("Input token is restricted")
                );
                continue;
            }
            let (posts, calls) = result.expect("allowed collateral input");
            lee_core::program::validate_execution(&pre, &posts, CURVE_PROGRAM_ID).unwrap();
            let updated = PoolAccount::try_from(&posts[0].account().data).unwrap();
            assert_eq!(
                (updated.pool.real_reserve0, updated.pool.real_reserve1),
                (600, 125)
            );
            assert_eq!(updated.allowed_input_token, restriction);
            assert_eq!(calls.len(), 2);
        }
    }
}

#[test]
fn swaps_reject_foreign_reserve_accounts() {
    let namespace = AccountId::new([51; 32]);
    let other = AccountId::new([52; 32]);
    let pool = namespace_pool(namespace);
    let foreign_pool = namespace_pool(other);
    let config = namespace_config(namespace, 100, new_treasury());
    let trader = AccountId::new([4; 32]);
    let token = AccountId::new([1; 32]);
    let collateral = AccountId::new([2; 32]);
    let valid = vec![
        pool.clone(),
        config,
        signer(trader),
        holding_ata(trader, collateral, 100),
        holding_ata(pool.account_id, collateral, 100),
        holding_ata(pool.account_id, token, 800),
        holding_ata(trader, token, 0),
        trusted_clock(1),
    ];
    let instruction = Instruction::SwapExactInput {
        amount_in: 25,
        min_amount_out: 0,
        token_in: collateral,
    };
    let (_, calls) = process_instruction(valid.clone(), instruction.clone(), CURVE_PROGRAM_ID);
    assert_eq!(calls.len(), 2);
    for (index, foreign) in [
        (4, holding_ata(foreign_pool.account_id, collateral, 100)),
        (5, holding_ata(foreign_pool.account_id, token, 800)),
    ] {
        let mut invalid = valid.clone();
        invalid[index] = foreign;
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                invalid,
                instruction.clone(),
                CURVE_PROGRAM_ID
            ))
            .is_err()
        );
    }
}

#[test]
fn pool_creation_binds_namespace_and_rejects_a_different_config() {
    let a = AccountId::new([51; 32]);
    let b = AccountId::new([52; 32]);
    let build = |namespace| {
        let f = valid_create_pool_accounts();
        let pool_id = compute_pool_pda(
            namespace,
            CURVE_PROGRAM_ID,
            f.token0_definition.account_id,
            f.token1_definition.account_id,
            f.owner.account_id,
        );
        let pool = AccountWithMetadata {
            account_id: pool_id,
            ..f.pool
        };
        let token0_ata = ata(pool_id, f.token0_definition.account_id, Account::default());
        let token1_ata = ata(pool_id, f.token1_definition.account_id, Account::default());
        let owner = f.owner.account_id;
        (
            vec![
                pool,
                f.owner,
                f.token0_definition,
                f.token1_definition,
                f.owner_token0_ata,
                f.owner_token1_ata,
                token0_ata,
                token1_ata,
                trusted_clock(1),
                namespace_config(namespace, 0, new_treasury()),
            ],
            Instruction::CreatePool {
                allowed_input_token: None,
                defer_funding: false,
                namespace,
                token0_amount: 800,
                token1_amount: 0,
                virtual_reserve0: 1000,
                virtual_reserve1: 100,
                close_timestamp: None,
                close_on_depletion: None,
                owner,
                owner_program_id: None,
                owner_program_seed: None,
                curve_program_id: CURVE_PROGRAM_ID,
            },
        )
    };
    let mut pool_ids = vec![];
    for namespace in [a, b] {
        let (accounts, instruction) = build(namespace);
        pool_ids.push(accounts[0].account_id);
        let (posts, calls) = process_instruction(accounts, instruction, CURVE_PROGRAM_ID);
        assert_eq!(
            PoolAccount::try_from(&posts[0].account().data)
                .unwrap()
                .namespace,
            namespace
        );
        assert_eq!(posts.len(), 10);
        assert_eq!(calls.len(), 3);
    }
    assert_ne!(pool_ids[0], pool_ids[1]);
    let (mut accounts, instruction) = build(a);
    accounts[9] = namespace_config(b, 0, new_treasury());
    assert!(
        std::panic::catch_unwind(|| process_instruction(accounts, instruction, CURVE_PROGRAM_ID))
            .is_err()
    );
}

#[test]
fn pool_close_rights_follow_the_owner_nft() {
    let mut pool = namespace_pool(NAMESPACE_CREATOR);
    let state = PoolAccount::try_from(&pool.account.data).unwrap();
    let mut holder = signer(state.owner);
    holder.account_id = AccountId::new([91; 32]);
    let mut clock = trusted_clock(1);
    clock.account.program_owner = [88; 8];
    let pre = vec![pool.clone(), holder, clock];
    let (post, _) = process_instruction(pre.clone(), Instruction::ClosePool, CURVE_PROGRAM_ID);
    lee_core::program::validate_execution(&pre, &post, CURVE_PROGRAM_ID).unwrap();
    pool.account = post[0].account().clone();
    assert_eq!(
        PoolAccount::try_from(&pool.account.data)
            .unwrap()
            .pool
            .effective_lifecycle(1),
        pool::PoolLifecycle::Closed
    );
}

#[test]
fn collection_accepts_unique_accounts_when_fees_are_already_at_the_treasury() {
    let mut pool = namespace_pool(NAMESPACE_CREATOR);
    let mut state = PoolAccount::try_from(&pool.account.data).unwrap();
    state.pool.fees_accrued = 7;
    pool.account.data = Data::from(&state);
    let pre = vec![
        pool.clone(),
        namespace_config(NAMESPACE_CREATOR, 100, pool.account_id),
        holding_ata(pool.account_id, state.token1_definition_id, 107),
    ];
    let (posts, calls) =
        process_instruction(pre.clone(), Instruction::CollectFees, CURVE_PROGRAM_ID);
    lee_core::program::validate_execution(&pre, &posts, CURVE_PROGRAM_ID).unwrap();
    assert_eq!(posts.len(), 3);
    assert!(calls.is_empty());
    assert_eq!(posts[2].account(), &pre[2].account);
    let collected = PoolAccount::try_from(&posts[0].account().data).unwrap();
    assert_eq!(collected.pool.real_reserve1, 100);
    assert_eq!(collected.pool.fees_accrued, 0);
    assert_eq!(collected.pool.fees_collected, 7);
    let mut replay = pre.clone();
    replay[0].account = posts[0].account().clone();
    let (_, calls) = process_instruction(replay, Instruction::CollectFees, CURVE_PROGRAM_ID);
    assert!(calls.is_empty());
    let mut missing_treasury = pre;
    missing_treasury[1] = namespace_config(NAMESPACE_CREATOR, 100, new_treasury());
    assert!(
        std::panic::catch_unwind(|| process_instruction(
            missing_treasury,
            Instruction::CollectFees,
            CURVE_PROGRAM_ID
        ))
        .is_err()
    );
}

#[test]
fn permissionless_collection_uses_live_treasury_and_preserves_principal() {
    let namespace = AccountId::new([51; 32]);
    let treasury = AccountId::new([64; 32]);
    let mut pool = namespace_pool(namespace);
    let mut state = PoolAccount::try_from(&pool.account.data).unwrap();
    state.pool.fees_accrued = 7;
    pool.account.data = Data::from(&state);
    let collateral = state.token1_definition_id;
    let pre = vec![
        pool.clone(),
        namespace_config(namespace, 999, treasury),
        holding_ata(
            pool.account_id,
            collateral,
            state.pool.real_reserve1 + 7 + 11,
        ),
        holding_ata(treasury, collateral, 0),
    ];
    assert!(pre.iter().all(|a| !a.is_authorized));
    let (posts, calls) =
        process_instruction(pre.clone(), Instruction::CollectFees, CURVE_PROGRAM_ID);
    lee_core::program::validate_execution(&pre, &posts, CURVE_PROGRAM_ID).unwrap();
    let after = PoolAccount::try_from(&posts[0].account().data).unwrap();
    let mut expected = state.clone();
    expected.pool.fees_accrued = 0;
    expected.pool.fees_collected = 7;
    assert_eq!(
        after, expected,
        "collection leaves pricing, reserves and lifecycle unchanged"
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].pre_states[2].account_id, pre[3].account_id);
    let instruction: AtaInstruction =
        risc0_zkvm::serde::from_slice(&calls[0].instruction_data).unwrap();
    assert!(matches!(
        instruction,
        AtaInstruction::Transfer { amount: 7, .. }
    ));
    let mut replay = pre.clone();
    replay[0].account = posts[0].account().clone();
    replay[2] = holding_ata(pool.account_id, collateral, state.pool.real_reserve1 + 11);
    replay[3] = holding_ata(treasury, collateral, 7);
    let (_, calls) = process_instruction(replay, Instruction::CollectFees, CURVE_PROGRAM_ID);
    assert!(
        calls.is_empty(),
        "collection is idempotent; donations are not fees"
    );
    for (index, replacement) in [
        (1, namespace_config(AccountId::new([52; 32]), 999, treasury)),
        (3, holding_ata(new_treasury(), collateral, 0)),
        (
            2,
            holding_ata(pool.account_id, collateral, state.pool.real_reserve1 + 6),
        ),
        (
            2,
            holding_ata(
                namespace_pool(AccountId::new([52; 32])).account_id,
                collateral,
                1000,
            ),
        ),
    ] {
        let mut invalid = pre.clone();
        invalid[index] = replacement;
        assert!(
            std::panic::catch_unwind(|| process_instruction(
                invalid,
                Instruction::CollectFees,
                CURVE_PROGRAM_ID
            ))
            .is_err()
        );
    }
}

#[test]
fn accrued_fees_cannot_back_a_sell_or_withdrawal() {
    let namespace = AccountId::new([51; 32]);
    let mut pool = namespace_pool(namespace);
    let mut state = PoolAccount::try_from(&pool.account.data).unwrap();
    state.pool.real_reserve1 = 0;
    state.pool.fees_accrued = 100;
    pool.account.data = Data::from(&state);
    assert!(
        std::panic::catch_unwind(|| swap_exact_input(
            pool,
            namespace_config(namespace, 100, new_treasury()),
            None,
            250,
            1,
            state.token0_definition_id,
            CURVE_PROGRAM_ID
        ))
        .is_err()
    );
}
