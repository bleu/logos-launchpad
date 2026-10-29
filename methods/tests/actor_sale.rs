use curve_core::{Message as CurveMessage, PoolAccount, TOKEN_PROGRAM_ACCOUNT_ID};
use lee::{
    AccountId, Actor, PrivateKey, PublicKey, PublicTransaction, V03State,
    program::Program,
    public_transaction::{Message, WitnessSet},
};
use lee_core::account::Account;
use lez_compat_core::token::{self, TokenKind, TokenState};
use std::{borrow::Cow, collections::BTreeMap};

const CURVE: AccountId = AccountId::new([7; 32]);
const NAMESPACE: AccountId = AccountId::new([8; 32]);
const OWNER: AccountId = AccountId::new([9; 32]);
const LAUNCH: AccountId = AccountId::new([10; 32]);
const COLLATERAL: AccountId = AccountId::new([11; 32]);
const LBP: AccountId = AccountId::new([14; 32]);
const TREASURY: AccountId = AccountId::new([15; 32]);
const NEXT_TREASURY: AccountId = AccountId::new([16; 32]);

fn holding(owner: AccountId, definition: AccountId) -> Actor {
    Actor::new(
        token::holding_address(TOKEN_PROGRAM_ACCOUNT_ID, owner, definition),
        TOKEN_PROGRAM_ACCOUNT_ID,
    )
}

fn account(actor: Actor, bytes: Vec<u8>) -> (AccountId, Account) {
    (
        actor.account_id,
        Account::default().with_actor_state(actor.program_account_id, bytes.into()),
    )
}

fn seeded_state(
    alice: AccountId,
    bob: AccountId,
    close_timestamp: Option<u64>,
) -> (V03State, AccountId) {
    let pool = curve_core::compute_pool_pda(NAMESPACE, CURVE, LAUNCH, COLLATERAL, OWNER);
    let curve = Program::new(Cow::Borrowed(launchpad_methods::CURVE_ELF)).unwrap();
    let token = Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_TOKEN_ELF)).unwrap();
    let pool_state = PoolAccount {
        allowed_input_token: Some(pool::TokenSide::Token1),
        funded: true,
        namespace: NAMESPACE,
        token0_definition_id: LAUNCH,
        token1_definition_id: COLLATERAL,
        owner: OWNER,
        owner_program: None,
        pool: pool::Pool::create(pool::PoolCreateParams {
            token0_amount: 4_000,
            token1_amount: 0,
            virtual_reserve0: 10_000,
            virtual_reserve1: 1_000,
            close_timestamp,
            close_on_depletion: Some(pool::TokenSide::Token0),
        })
        .unwrap(),
    };
    // Seeded funded state does not test sale setup.
    let config = borsh::to_vec(&(
        *b"LZNSCONF",
        3u16,
        NAMESPACE,
        0u16,
        AccountId::new([12; 32]),
        (AccountId::new([13; 32]), false),
    ))
    .unwrap();
    curve_core::Config::decode(&config).unwrap();
    let mut accounts = vec![
        account(Actor::new(pool, CURVE), borsh::to_vec(&pool_state).unwrap()),
        account(
            Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
            config,
        ),
    ];
    for (owner, definition, balance) in [
        (pool, LAUNCH, 4_000),
        (pool, COLLATERAL, 0),
        (alice, LAUNCH, 0),
        (alice, COLLATERAL, 200),
        (bob, LAUNCH, 0),
        (bob, COLLATERAL, 200),
    ] {
        accounts.push(account(
            holding(owner, definition),
            borsh::to_vec(&TokenState::Holding {
                definition,
                owner,
                kind: TokenKind::Fungible,
                balance,
            })
            .unwrap(),
        ));
    }
    (
        V03State::new()
            .with_named_programs([
                (CURVE, curve),
                (TOKEN_PROGRAM_ACCOUNT_ID, token),
                (clock_core::clock_account_id(), programs::clock()),
            ])
            .with_public_accounts(accounts),
        pool,
    )
}

fn buy_transaction(
    state: &V03State,
    pool: AccountId,
    key: &PrivateKey,
    max_amount_in: u128,
    now: u64,
) -> PublicTransaction {
    let buyer = AccountId::from(&PublicKey::new_from_private_key(key));
    let actors = vec![
        Actor::new(buyer, CURVE),
        Actor::new(pool, CURVE),
        Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
        Actor::new(buyer, TOKEN_PROGRAM_ACCOUNT_ID),
        Actor::new(pool, TOKEN_PROGRAM_ACCOUNT_ID),
        holding(pool, LAUNCH),
        holding(pool, COLLATERAL),
        holding(buyer, LAUNCH),
        holding(buyer, COLLATERAL),
        Actor::new(
            clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
            clock_core::clock_account_id(),
        ),
    ];
    let message = Message::try_new(
        Actor::new(buyer, CURVE),
        actors,
        BTreeMap::from([(buyer, state.get_account_by_id(buyer).nonce)]),
        CurveMessage::Execute {
            pool,
            instruction: Box::new(CurveMessage::SwapExactOutput {
                amount_out: 1_000,
                max_amount_in,
                token_in: COLLATERAL,
                now,
            }),
        },
    )
    .unwrap();
    let witnesses = WitnessSet::for_message(&message, &[key]);
    PublicTransaction::new(message, witnesses)
}

fn buy(
    state: &mut V03State,
    pool: AccountId,
    key: &PrivateKey,
    max_amount_in: u128,
) -> Result<(), lee::error::LeeError> {
    let transaction = buy_transaction(state, pool, key, max_amount_in, 1);
    state
        .transition_from_public_transaction(&transaction, 1, 0)
        .map(|_| ())
}

fn balance(state: &V03State, owner: AccountId, definition: AccountId) -> u128 {
    match TokenState::decode(
        state
            .get_account_by_id(holding(owner, definition).account_id)
            .data
            .actor_state(TOKEN_PROGRAM_ACCOUNT_ID),
    )
    .unwrap()
    {
        TokenState::Holding { balance, .. } => balance,
        _ => panic!("expected holding"),
    }
}

fn clock_state(state: V03State, timestamp: u64) -> V03State {
    state.with_public_accounts([account(
        Actor::new(
            clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
            clock_core::clock_account_id(),
        ),
        clock_core::ClockAccountData {
            block_id: 1,
            timestamp,
        }
        .to_bytes(),
    )])
}

fn fixed_config(state: V03State, fee_bps: u16, treasury: AccountId, frozen: bool) -> V03State {
    let config = borsh::to_vec(&(
        *b"LZNSCONF",
        3u16,
        NAMESPACE,
        fee_bps,
        treasury,
        (AccountId::new([13; 32]), frozen),
    ))
    .unwrap();
    curve_core::Config::decode(&config).unwrap();
    state.with_public_accounts([account(
        Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
        config,
    )])
}

fn fixed_state(buyer: AccountId, end: Option<u64>) -> (V03State, AccountId) {
    let pool = lbp_core::compute_pool_pda(NAMESPACE, LBP, LAUNCH, COLLATERAL, OWNER);
    // Seeded funded state does not test the four setup stages.
    let bytes = borsh::to_vec(&(
        (
            lbp_core::PoolMode::FixedPrice,
            true,
            NAMESPACE,
            CURVE,
            OWNER,
            AccountId::new([17; 32]),
            [18u8; 32],
            LAUNCH,
            COLLATERAL,
        ),
        (
            1_000u128,
            1_000u128,
            0u128,
            lbp_core::FixedPrice {
                collateral_units: 3,
                token_units: 2,
            },
            100u64,
            end,
            lbp_core::CreatorAllocationRelease::AfterEnd,
            0u128,
            0u128,
            false,
            None::<(bool, u8)>,
        ),
    ))
    .unwrap();
    lbp_core::PoolAccount::decode(&bytes).unwrap();
    let mut accounts = vec![account(Actor::new(pool, LBP), bytes)];
    for (owner, definition, balance) in [
        (pool, LAUNCH, 1_007),
        (pool, COLLATERAL, 41),
        (buyer, LAUNCH, 0),
        (buyer, COLLATERAL, 2_000),
        (TREASURY, COLLATERAL, 0),
        (NEXT_TREASURY, COLLATERAL, 0),
    ] {
        accounts.push(account(
            holding(owner, definition),
            borsh::to_vec(&TokenState::Holding {
                definition,
                owner,
                kind: TokenKind::Fungible,
                balance,
            })
            .unwrap(),
        ));
    }
    let state = V03State::new()
        .with_named_programs([
            (
                LBP,
                Program::new(Cow::Borrowed(launchpad_methods::LBP_ELF)).unwrap(),
            ),
            (
                CURVE,
                Program::new(Cow::Borrowed(launchpad_methods::CURVE_ELF)).unwrap(),
            ),
            (
                TOKEN_PROGRAM_ACCOUNT_ID,
                Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_TOKEN_ELF)).unwrap(),
            ),
            (clock_core::clock_account_id(), programs::clock()),
        ])
        .with_public_accounts(accounts);
    (
        clock_state(fixed_config(state, 333, TREASURY, false), 100_000),
        pool,
    )
}

fn fixed_pool(state: &V03State, pool: AccountId) -> lbp_core::PoolAccount {
    lbp_core::PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(LBP)).unwrap()
}

fn fixed_transaction(
    state: &V03State,
    pool: AccountId,
    key: &PrivateKey,
    instruction: lbp_core::Message,
) -> PublicTransaction {
    let buyer = AccountId::from(&PublicKey::new_from_private_key(key));
    let collect = matches!(instruction, lbp_core::Message::CollectFees);
    let receiver = Actor::new(if collect { pool } else { buyer }, LBP);
    let actors = vec![
        Actor::new(buyer, LBP),
        Actor::new(pool, LBP),
        Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
        Actor::new(buyer, TOKEN_PROGRAM_ACCOUNT_ID),
        Actor::new(pool, TOKEN_PROGRAM_ACCOUNT_ID),
        holding(pool, LAUNCH),
        holding(pool, COLLATERAL),
        holding(buyer, LAUNCH),
        holding(buyer, COLLATERAL),
        holding(TREASURY, COLLATERAL),
        holding(NEXT_TREASURY, COLLATERAL),
        Actor::new(
            clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
            clock_core::clock_account_id(),
        ),
    ];
    let nonces = if collect {
        BTreeMap::new()
    } else {
        BTreeMap::from([(buyer, state.get_account_by_id(buyer).nonce)])
    };
    let instruction = if collect {
        instruction
    } else {
        lbp_core::Message::Execute {
            pool,
            instruction: Box::new(instruction),
        }
    };
    let message = Message::try_new(receiver, actors, nonces, instruction).unwrap();
    let keys = [key];
    let witnesses = WitnessSet::for_message(&message, if collect { &[] } else { &keys });
    PublicTransaction::new(message, witnesses)
}

fn fixed_execute(
    state: &mut V03State,
    pool: AccountId,
    key: &PrivateKey,
    instruction: lbp_core::Message,
) -> Result<(), lee::error::LeeError> {
    let transaction = fixed_transaction(state, pool, key, instruction);
    let timestamp = clock_core::ClockAccountData::from_bytes(
        state
            .get_account_by_id(clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID)
            .data
            .actor_state(clock_core::clock_account_id()),
    )
    .timestamp;
    state
        .transition_from_public_transaction(&transaction, 1, timestamp)
        .map(|_| ())
}

fn fixed_rejected(
    state: &mut V03State,
    pool: AccountId,
    key: &PrivateKey,
    instruction: lbp_core::Message,
    reason: &str,
) {
    let buyer = AccountId::from(&PublicKey::new_from_private_key(key));
    let before = state.clone();
    let nonce = state.get_account_by_id(buyer).nonce;
    let rejected = fixed_execute(state, pool, key, instruction);
    assert!(
        matches!(&rejected, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains(reason)),
        "expected {reason}, got {rejected:?}",
    );
    assert!(
        *state == before,
        "rejected fixed-price operation changed runtime state"
    );
    assert_eq!(state.get_account_by_id(buyer).nonce, nonce);
}

#[test]
fn fixed_price_buys_use_full_input_live_fees_and_current_treasury() {
    use lbp_core::Message::{BuyExactInput, BuyExactOutput, CollectFees};
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (mut state, pool) = fixed_state(buyer, Some(200));
    fixed_execute(
        &mut state,
        pool,
        &key,
        BuyExactInput {
            amount_in: 101,
            min_amount_out: 67,
            now: 100,
        },
    )
    .unwrap();
    assert_eq!(balance(&state, buyer, LAUNCH), 67);
    assert_eq!(balance(&state, buyer, COLLATERAL), 1_899);
    let sale = fixed_pool(&state, pool);
    assert_eq!(
        (
            sale.remaining_inventory,
            sale.recorded_collateral,
            sale.accrued_fees
        ),
        (933, 97, 4)
    );

    state = fixed_config(state, 1_001, NEXT_TREASURY, false);
    fixed_execute(
        &mut state,
        pool,
        &key,
        BuyExactOutput {
            amount_out: 13,
            max_amount_in: 20,
            now: 100,
        },
    )
    .unwrap();
    assert_eq!(balance(&state, buyer, LAUNCH), 80);
    assert_eq!(balance(&state, buyer, COLLATERAL), 1_879);
    let sale = fixed_pool(&state, pool);
    assert_eq!(
        (
            sale.remaining_inventory,
            sale.recorded_collateral,
            sale.accrued_fees
        ),
        (920, 114, 7)
    );
    assert_eq!(balance(&state, pool, COLLATERAL), 162);
    assert_eq!(balance(&state, pool, LAUNCH), 927);

    state = clock_state(fixed_config(state, 1_001, NEXT_TREASURY, true), 200_001);
    fixed_execute(&mut state, pool, &key, CollectFees).unwrap();
    assert_eq!(balance(&state, TREASURY, COLLATERAL), 0);
    assert_eq!(balance(&state, NEXT_TREASURY, COLLATERAL), 7);
    assert_eq!(balance(&state, pool, COLLATERAL), 155);
    let sale = fixed_pool(&state, pool);
    assert_eq!((sale.recorded_collateral, sale.accrued_fees), (114, 0));
    assert_eq!(
        balance(&state, pool, COLLATERAL) - sale.recorded_collateral,
        41
    );
    assert_eq!(balance(&state, pool, LAUNCH) - sale.remaining_inventory, 7);
    let before = state.clone();
    fixed_execute(&mut state, pool, &key, CollectFees).unwrap();
    assert!(state == before, "zero-fee retry changed runtime state");
}

#[test]
fn fixed_price_without_end_checks_start_clock_and_stops_at_sellout() {
    use lbp_core::Message::{BuyExactInput, BuyExactOutput, CollectFees};
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (state, pool) = fixed_state(buyer, None);
    let mut state = clock_state(state, 99_999);
    fixed_rejected(
        &mut state,
        pool,
        &key,
        BuyExactInput {
            amount_in: 3,
            min_amount_out: 2,
            now: 100,
        },
        "Clock timestamp",
    );
    state = clock_state(state, 100_000);
    fixed_execute(
        &mut state,
        pool,
        &key,
        BuyExactOutput {
            amount_out: 1_000,
            max_amount_in: 1_500,
            now: 100,
        },
    )
    .unwrap();
    let sale = fixed_pool(&state, pool);
    assert_eq!(
        (
            sale.remaining_inventory,
            sale.recorded_collateral,
            sale.accrued_fees
        ),
        (0, 1_450, 50)
    );
    assert_eq!(balance(&state, pool, LAUNCH), 7);
    fixed_rejected(
        &mut state,
        pool,
        &key,
        BuyExactInput {
            amount_in: 3,
            min_amount_out: 2,
            now: 100,
        },
        "Closed",
    );
    fixed_execute(&mut state, pool, &key, CollectFees).unwrap();
    assert_eq!(balance(&state, TREASURY, COLLATERAL), 50);
    assert_eq!(balance(&state, pool, COLLATERAL), 1_491);
}

#[test]
fn fixed_price_deadline_slippage_and_frozen_buys_roll_back() {
    use lbp_core::Message::{BuyExactInput, BuyExactOutput};
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (mut state, pool) = fixed_state(buyer, Some(200));
    fixed_rejected(
        &mut state,
        pool,
        &key,
        BuyExactInput {
            amount_in: 101,
            min_amount_out: 68,
            now: 100,
        },
        "Slippage",
    );
    fixed_rejected(
        &mut state,
        pool,
        &key,
        BuyExactOutput {
            amount_out: 3,
            max_amount_in: 4,
            now: 100,
        },
        "Slippage",
    );
    state = fixed_config(state, 333, TREASURY, true);
    fixed_rejected(
        &mut state,
        pool,
        &key,
        BuyExactInput {
            amount_in: 3,
            min_amount_out: 2,
            now: 100,
        },
        "Frozen",
    );
    state = fixed_config(state, 333, TREASURY, false);
    let instruction = BuyExactInput {
        amount_in: 3,
        min_amount_out: 2,
        now: 100,
    };
    let transaction = fixed_transaction(&state, pool, &key, instruction.clone());
    let mut before_end = clock_state(state.clone(), 199_999);
    before_end
        .transition_from_public_transaction(&transaction, 1, 199_999)
        .unwrap();
    assert_eq!(balance(&before_end, buyer, LAUNCH), 2);
    for timestamp in [200_000, 200_001] {
        let mut late = clock_state(state.clone(), timestamp);
        fixed_rejected(
            &mut late,
            pool,
            &key,
            instruction.clone(),
            "Clock timestamp",
        );
    }
}

#[test]
fn fixed_price_output_transfer_failure_rolls_back_input_and_nonce() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (state, pool) = fixed_state(buyer, None);
    let mut state = state.with_public_accounts([account(
        holding(buyer, LAUNCH),
        borsh::to_vec(&TokenState::Holding {
            definition: COLLATERAL,
            owner: buyer,
            kind: TokenKind::Fungible,
            balance: 0,
        })
        .unwrap(),
    )]);
    fixed_rejected(
        &mut state,
        pool,
        &key,
        lbp_core::Message::BuyExactInput {
            amount_in: 3,
            min_amount_out: 2,
            now: 100,
        },
        "WrongDefinition",
    );
    assert_eq!(balance(&state, buyer, COLLATERAL), 2_000);
    assert_eq!(balance(&state, pool, COLLATERAL), 41);
    let sale = fixed_pool(&state, pool);
    assert_eq!(
        (
            sale.remaining_inventory,
            sale.recorded_collateral,
            sale.accrued_fees
        ),
        (1_000, 0, 0)
    );
}

#[test]
fn timed_buy_uses_native_milliseconds_and_execution_deadline() {
    const BUILT_AT: u64 = 1_792_665_600;
    const DEADLINE: u64 = BUILT_AT + 60;
    let alice_key = PrivateKey::try_new([21; 32]).unwrap();
    let bob_key = PrivateKey::try_new([22; 32]).unwrap();
    let alice = AccountId::from(&PublicKey::new_from_private_key(&alice_key));
    let bob = AccountId::from(&PublicKey::new_from_private_key(&bob_key));
    let (state, pool) = seeded_state(alice, bob, Some(DEADLINE));
    let state = clock_state(state, BUILT_AT * 1_000);
    let transaction = buy_transaction(&state, pool, &alice_key, 200, BUILT_AT);
    let mut state = clock_state(state, (BUILT_AT + 5) * 1_000 + 123);
    state
        .transition_from_public_transaction(&transaction, 1, (BUILT_AT + 5) * 1_000 + 123)
        .expect("a delayed transaction before the deadline must execute");
    assert_eq!(balance(&state, alice, LAUNCH), 1_000);
    assert_eq!(balance(&state, alice, COLLATERAL), 88);
    assert_eq!(balance(&state, pool, LAUNCH), 3_000);
    assert_eq!(balance(&state, pool, COLLATERAL), 112);

    let transaction = buy_transaction(&state, pool, &bob_key, 200, BUILT_AT);
    for timestamp in [DEADLINE * 1_000, DEADLINE * 1_000 + 1] {
        let mut late = clock_state(state.clone(), timestamp);
        let before = late.clone();
        let nonce = late.get_account_by_id(bob).nonce;
        let rejected = late.transition_from_public_transaction(&transaction, 1, timestamp);
        assert!(
            matches!(rejected, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("Clock timestamp"))
        );
        assert!(late == before, "late buy changed runtime state");
        assert_eq!(late.get_account_by_id(bob).nonce, nonce);
    }
}

#[test]
fn exact_output_buy_executes_real_curve_and_token_guests() {
    let alice_key = PrivateKey::try_new([21; 32]).unwrap();
    let bob_key = PrivateKey::try_new([22; 32]).unwrap();
    let alice = AccountId::from(&PublicKey::new_from_private_key(&alice_key));
    let bob = AccountId::from(&PublicKey::new_from_private_key(&bob_key));
    let (mut state, pool) = seeded_state(alice, bob, None);
    buy(&mut state, pool, &alice_key, 200).unwrap();
    assert_eq!(balance(&state, alice, LAUNCH), 1_000);
    assert_eq!(balance(&state, alice, COLLATERAL), 88);
    assert_eq!(balance(&state, pool, LAUNCH), 3_000);
    assert_eq!(balance(&state, pool, COLLATERAL), 112);

    let before = state.clone();
    let nonce = state.get_account_by_id(bob).nonce;
    let rejected = buy(&mut state, pool, &bob_key, 112);
    assert!(
        matches!(rejected, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("InvalidPool"))
    );
    assert!(state == before, "rejected buy changed runtime state");
    assert_eq!(state.get_account_by_id(bob).nonce, nonce);

    buy(&mut state, pool, &bob_key, 200).unwrap();
    assert_eq!(balance(&state, bob, LAUNCH), 1_000);
    assert_eq!(balance(&state, bob, COLLATERAL), 62);
    assert_eq!(balance(&state, pool, LAUNCH), 2_000);
    assert_eq!(balance(&state, pool, COLLATERAL), 250);
    assert_eq!(balance(&state, alice, COLLATERAL), 88);
    let updated =
        PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(CURVE)).unwrap();
    assert_eq!(
        (updated.pool.real_reserve0, updated.pool.real_reserve1),
        (2_000, 250)
    );
}
