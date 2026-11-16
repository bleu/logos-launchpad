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
            .with_programs_at(
                [
                    (CURVE, curve),
                    (TOKEN_PROGRAM_ACCOUNT_ID, token),
                    (clock_core::clock_account_id(), programs::clock()),
                ]
                .map(|(address, program)| (address, program, true)),
            )
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
            clock_core::CLOCK_PROGRAM_ACCOUNT_ID,
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
        clock_core::clock_actor(),
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
        .with_programs_at(
            [
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
            ]
            .map(|(address, program)| (address, program, true)),
        )
        .with_public_accounts(accounts);
    (
        clock_state(fixed_config(state, 333, TREASURY, false), 100_000),
        pool,
    )
}

fn fixed_pool(state: &V03State, pool: AccountId) -> lbp_core::PoolAccount {
    lbp_core::PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(LBP)).unwrap()
}

fn weighted_state(buyer: AccountId) -> (V03State, AccountId) {
    let (state, pool) = fixed_state(buyer, Some(200));
    let mut sale = fixed_pool(&state, pool);
    sale.mode = lbp_core::PoolMode::Weighted {
        paused: false,
        token_weight_start: lbp_core::WEIGHT_TOTAL / 2,
        token_weight_end: lbp_core::WEIGHT_TOTAL / 2,
        collateral_seed: 100,
        pricing_collateral: 100,
        checkpoint_time: 0,
        checkpoint_token_weight: 0,
    };
    sale.price = lbp_core::FixedPrice {
        collateral_units: 0,
        token_units: 0,
    };
    sale.recorded_collateral = 100;
    let state = state.with_public_accounts([
        account(Actor::new(pool, LBP), borsh::to_vec(&sale).unwrap()),
        account(
            holding(pool, COLLATERAL),
            borsh::to_vec(&TokenState::Holding {
                definition: COLLATERAL,
                owner: pool,
                kind: TokenKind::Fungible,
                // Include surplus. Only the recorded 100 units enter pricing.
                balance: 141,
            })
            .unwrap(),
        ),
    ]);
    (state, pool)
}

#[test]
fn weighted_clock_prices_at_saved_time_and_rejects_outside_sale() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (state, pool) = weighted_state(buyer);
    let mut sale = fixed_pool(&state, pool);
    if let lbp_core::PoolMode::Weighted {
        token_weight_end, ..
    } = &mut sale.mode
    {
        *token_weight_end = 90 * lbp_core::WEIGHT_TOTAL / 100;
    }
    let state = state.with_public_accounts([account(
        Actor::new(pool, LBP),
        borsh::to_vec(&sale).unwrap(),
    )]);
    let buy = lbp_core::Message::BuyWeightedExactInput {
        amount_in: 50,
        min_amount_out: 1,
    };
    for timestamp in [100_000, 130_000, 199_999] {
        let mut valid = clock_state(state.clone(), timestamp);
        fixed_execute(&mut valid, pool, &key, buy.clone()).unwrap();
        assert!(balance(&valid, buyer, LAUNCH) > 0);
        if timestamp == 100_000 {
            assert_eq!(balance(&valid, buyer, LAUNCH), 333);
        }
        if timestamp == 199_999 {
            assert!(balance(&valid, buyer, LAUNCH) < 100);
        }
        assert_eq!(balance(&valid, buyer, COLLATERAL), 1950);
    }
    for timestamp in [99_999, 200_000] {
        let mut invalid = clock_state(state.clone(), timestamp);
        fixed_rejected(&mut invalid, pool, &key, buy.clone(), "Closed");
    }
}

#[test]
fn weighted_inclusion_window_rejects_stale_clock_buys_and_preserves_state() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (state, pool) = weighted_state(buyer);
    for buy in [
        lbp_core::Message::BuyWeightedExactInput {
            amount_in: 50,
            min_amount_out: 1,
        },
        lbp_core::Message::BuyWeightedExactOutput {
            amount_out: 10,
            max_amount_in: 100,
        },
    ] {
        // Saved pricing time stays fixed while inclusion time crosses the sale bounds.
        let transaction = fixed_transaction(&state, pool, &key, buy);
        for timestamp in [100_000, 130_000, 199_999] {
            let mut admitted = state.clone();
            admitted
                .transition_from_public_transaction(&transaction, 1, timestamp)
                .unwrap();
            assert!(balance(&admitted, buyer, LAUNCH) > 0);
        }
        for timestamp in [99_999, 200_000, 200_001] {
            let mut rejected = state.clone();
            assert!(matches!(
                rejected.transition_from_public_transaction(&transaction, 1, timestamp),
                Err(lee::error::LeeError::OutOfValidityWindow)
            ));
            assert!(
                rejected == state,
                "out-of-window buy changed state or nonce"
            );
        }
    }
}

#[test]
fn weighted_collection_keeps_pricing_and_same_second_buys_use_current_reserves() {
    use lbp_core::Message::{BuyWeightedExactInput, BuyWeightedExactOutput, CollectFees};
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (mut state, pool) = weighted_state(buyer);
    fixed_execute(
        &mut state,
        pool,
        &key,
        BuyWeightedExactInput {
            amount_in: 50,
            min_amount_out: 333,
        },
    )
    .unwrap();
    let mut without_collection = state.clone();
    fixed_execute(&mut state, pool, &key, CollectFees).unwrap();
    assert_eq!(balance(&state, TREASURY, COLLATERAL), 2);
    assert_eq!(fixed_pool(&state, pool).accrued_fees, 0);
    let next_buy = BuyWeightedExactOutput {
        amount_out: 100,
        max_amount_in: 27,
    };
    fixed_execute(&mut state, pool, &key, next_buy.clone()).unwrap();
    fixed_execute(&mut without_collection, pool, &key, next_buy).unwrap();
    assert_eq!(balance(&state, buyer, COLLATERAL), 1923);
    assert_eq!(balance(&state, buyer, LAUNCH), 433);
    assert_eq!(
        balance(&state, buyer, COLLATERAL),
        balance(&without_collection, buyer, COLLATERAL)
    );
    let sale = fixed_pool(&state, pool);
    assert_eq!(sale.remaining_inventory, 567);
    assert!(matches!(
        sale.mode,
        lbp_core::PoolMode::Weighted {
            pricing_collateral: 177,
            ..
        }
    ));
    assert_eq!(balance(&state, pool, COLLATERAL), 216);
    assert_eq!(sale.recorded_collateral + sale.accrued_fees, 175);
}

#[test]
fn weighted_accuracy_and_vault_overflow_rejections_preserve_state_and_nonce() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (state, pool) = weighted_state(buyer);
    let mut sale = fixed_pool(&state, pool);
    if let lbp_core::PoolMode::Weighted {
        token_weight_start,
        token_weight_end,
        ..
    } = &mut sale.mode
    {
        *token_weight_start = 99 * lbp_core::WEIGHT_TOTAL / 100;
        *token_weight_end = 99 * lbp_core::WEIGHT_TOTAL / 100;
    }
    let mut inaccurate = state.clone().with_public_accounts([account(
        Actor::new(pool, LBP),
        borsh::to_vec(&sale).unwrap(),
    )]);
    fixed_rejected(
        &mut inaccurate,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactOutput {
            amount_out: 300,
            max_amount_in: u128::MAX,
        },
        "MathAccuracy",
    );

    let mut overflow = state.with_public_accounts([account(
        holding(pool, COLLATERAL),
        borsh::to_vec(&TokenState::Holding {
            definition: COLLATERAL,
            owner: pool,
            kind: TokenKind::Fungible,
            balance: u128::MAX,
        })
        .unwrap(),
    )]);
    fixed_rejected(
        &mut overflow,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactInput {
            amount_in: 50,
            min_amount_out: 333,
        },
        "Arithmetic",
    );
}

#[test]
fn weighted_output_credit_failure_rolls_back_input_records_and_nonce() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (state, pool) = weighted_state(buyer);
    let mut state = state.with_public_accounts([account(
        holding(buyer, LAUNCH),
        borsh::to_vec(&TokenState::Holding {
            definition: LAUNCH,
            owner: buyer,
            kind: TokenKind::Fungible,
            balance: u128::MAX,
        })
        .unwrap(),
    )]);
    let before = fixed_pool(&state, pool);
    fixed_rejected(
        &mut state,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactInput {
            amount_in: 50,
            min_amount_out: 333,
        },
        "Overflow",
    );
    assert_eq!(balance(&state, buyer, COLLATERAL), 2_000);
    assert_eq!(balance(&state, buyer, LAUNCH), u128::MAX);
    assert_eq!(balance(&state, pool, COLLATERAL), 141);
    assert_eq!(balance(&state, pool, LAUNCH), 1_007);
    assert_eq!(fixed_pool(&state, pool), before);
}

fn fixed_transaction(
    state: &V03State,
    pool: AccountId,
    key: &PrivateKey,
    instruction: lbp_core::Message,
) -> PublicTransaction {
    let launch = fixed_pool(state, pool).token_definition_id;
    let buyer = AccountId::from(&PublicKey::new_from_private_key(key));
    let collect = matches!(instruction, lbp_core::Message::CollectFees);
    let receiver = Actor::new(if collect { pool } else { buyer }, LBP);
    let actors = vec![
        Actor::new(buyer, LBP),
        Actor::new(pool, LBP),
        Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
        Actor::new(buyer, TOKEN_PROGRAM_ACCOUNT_ID),
        Actor::new(pool, TOKEN_PROGRAM_ACCOUNT_ID),
        holding(pool, launch),
        holding(pool, COLLATERAL),
        holding(buyer, launch),
        holding(buyer, COLLATERAL),
        holding(TREASURY, COLLATERAL),
        holding(NEXT_TREASURY, COLLATERAL),
        Actor::new(
            clock_core::CLOCK_PROGRAM_ACCOUNT_ID,
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
    let mut message = Message::try_new(receiver, actors, nonces, instruction).unwrap();
    message
        .admission_evidence
        .push(lee::PublicAccountEvidence::Pda {
            program: TOKEN_PROGRAM_ACCOUNT_ID,
            seed: token::holding_seed(buyer, launch),
        });
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
    fixed_execute_at(state, pool, key, instruction, 100_000)
}

fn fixed_execute_at(
    state: &mut V03State,
    pool: AccountId,
    key: &PrivateKey,
    instruction: lbp_core::Message,
    timestamp: u64,
) -> Result<(), lee::error::LeeError> {
    let transaction = fixed_transaction(state, pool, key, instruction);
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
fn fixed_price_buys_deduct_live_fees_before_pricing_and_collect_to_current_treasury() {
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
            min_amount_out: 64,
            now: 100,
        },
    )
    .unwrap();
    assert_eq!(balance(&state, buyer, LAUNCH), 64);
    assert_eq!(balance(&state, buyer, COLLATERAL), 1_899);
    let sale = fixed_pool(&state, pool);
    assert_eq!(
        (
            sale.remaining_inventory,
            sale.recorded_collateral,
            sale.accrued_fees
        ),
        (936, 97, 4)
    );

    state = fixed_config(state, 1_001, NEXT_TREASURY, false);
    fixed_execute(
        &mut state,
        pool,
        &key,
        BuyExactOutput {
            amount_out: 13,
            max_amount_in: 23,
            now: 100,
        },
    )
    .unwrap();
    assert_eq!(balance(&state, buyer, LAUNCH), 77);
    assert_eq!(balance(&state, buyer, COLLATERAL), 1_876);
    let sale = fixed_pool(&state, pool);
    assert_eq!(
        (
            sale.remaining_inventory,
            sale.recorded_collateral,
            sale.accrued_fees
        ),
        (923, 117, 7)
    );
    assert_eq!(balance(&state, pool, COLLATERAL), 165);
    assert_eq!(balance(&state, pool, LAUNCH), 930);

    state = clock_state(fixed_config(state, 1_001, NEXT_TREASURY, true), 200_001);
    fixed_execute_at(&mut state, pool, &key, CollectFees, 200_001).unwrap();
    assert_eq!(balance(&state, TREASURY, COLLATERAL), 0);
    assert_eq!(balance(&state, NEXT_TREASURY, COLLATERAL), 7);
    assert_eq!(balance(&state, pool, COLLATERAL), 158);
    let sale = fixed_pool(&state, pool);
    assert_eq!((sale.recorded_collateral, sale.accrued_fees), (117, 0));
    assert_eq!(
        balance(&state, pool, COLLATERAL) - sale.recorded_collateral,
        41
    );
    assert_eq!(balance(&state, pool, LAUNCH) - sale.remaining_inventory, 7);
    let before = state.clone();
    fixed_execute_at(&mut state, pool, &key, CollectFees, 200_001).unwrap();
    assert!(state == before, "zero-fee retry changed runtime state");
}

#[test]
fn fixed_price_without_end_checks_inclusion_start_and_stops_at_sellout() {
    use lbp_core::Message::{BuyExactInput, BuyExactOutput, CollectFees};
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (mut state, pool) = fixed_state(buyer, None);
    let transaction = fixed_transaction(
        &state,
        pool,
        &key,
        BuyExactInput {
            amount_in: 4,
            min_amount_out: 2,
            now: 100,
        },
    );
    let before = state.clone();
    assert!(matches!(
        state.transition_from_public_transaction(&transaction, 1, 99_999),
        Err(lee::error::LeeError::OutOfValidityWindow)
    ));
    assert!(state == before, "early buy changed runtime state");
    assert_eq!(
        state.get_account_by_id(buyer).nonce,
        before.get_account_by_id(buyer).nonce
    );
    fixed_execute(
        &mut state,
        pool,
        &key,
        BuyExactOutput {
            amount_out: 1_000,
            max_amount_in: 1_552,
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
        (0, 1_500, 52)
    );
    assert_eq!(balance(&state, buyer, LAUNCH), 1_000);
    assert_eq!(balance(&state, buyer, COLLATERAL), 448);
    assert_eq!(balance(&state, pool, COLLATERAL), 1_593);
    assert_eq!(balance(&state, pool, LAUNCH), 7);
    fixed_rejected(
        &mut state,
        pool,
        &key,
        BuyExactInput {
            amount_in: 4,
            min_amount_out: 2,
            now: 100,
        },
        "Closed",
    );
    fixed_execute_at(&mut state, pool, &key, CollectFees, 200_001).unwrap();
    assert_eq!(balance(&state, TREASURY, COLLATERAL), 52);
    assert_eq!(balance(&state, pool, COLLATERAL), 1_541);
}

#[test]
fn fixed_price_fee_only_slippage_and_fee_consuming_dust_preserve_state_and_nonce() {
    use lbp_core::Message::{BuyExactInput, BuyExactOutput};
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (mut state, pool) = fixed_state(buyer, None);
    fixed_rejected(
        &mut state,
        pool,
        &key,
        BuyExactInput {
            amount_in: 101,
            min_amount_out: 67,
            now: 100,
        },
        "Slippage",
    );
    state = fixed_config(state, 1_001, TREASURY, false);
    fixed_rejected(
        &mut state,
        pool,
        &key,
        BuyExactOutput {
            amount_out: 13,
            max_amount_in: 20,
            now: 100,
        },
        "Slippage",
    );
    state = fixed_config(state, 9_999, TREASURY, false);
    fixed_rejected(
        &mut state,
        pool,
        &key,
        BuyExactInput {
            amount_in: 3,
            min_amount_out: 0,
            now: 100,
        },
        "InvalidAmount",
    );
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
            amount_in: 4,
            min_amount_out: 2,
            now: 100,
        },
        "Frozen",
    );
    state = fixed_config(state, 333, TREASURY, false);
    let instruction = BuyExactInput {
        amount_in: 4,
        min_amount_out: 2,
        now: 100,
    };
    let transaction = fixed_transaction(&state, pool, &key, instruction.clone());
    for timestamp in [100_000, 199_999] {
        let mut admitted = state.clone();
        admitted
            .transition_from_public_transaction(&transaction, 1, timestamp)
            .unwrap();
        assert_eq!(balance(&admitted, buyer, LAUNCH), 2);
    }
    for timestamp in [99_999, 200_000, 200_001] {
        let mut rejected = state.clone();
        let before = rejected.clone();
        let nonce = rejected.get_account_by_id(buyer).nonce;
        assert!(matches!(
            rejected.transition_from_public_transaction(&transaction, 1, timestamp),
            Err(lee::error::LeeError::OutOfValidityWindow)
        ));
        assert!(
            rejected == before,
            "out-of-window buy changed runtime state"
        );
        assert_eq!(rejected.get_account_by_id(buyer).nonce, nonce);
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
            amount_in: 4,
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
    let mut before_end = state.clone();
    before_end
        .transition_from_public_transaction(&transaction, 1, DEADLINE * 1_000 - 1)
        .expect("the last millisecond before expiry must remain valid");
    assert_eq!(balance(&before_end, bob, LAUNCH), 1_000);
    for timestamp in [DEADLINE * 1_000, DEADLINE * 1_000 + 1] {
        let mut late = state.clone();
        let before = late.clone();
        let nonce = late.get_account_by_id(bob).nonce;
        let rejected = late.transition_from_public_transaction(&transaction, 1, timestamp);
        assert!(matches!(
            rejected,
            Err(lee::error::LeeError::OutOfValidityWindow)
        ));
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

#[test]
fn weighted_factory_executes_all_stages_and_rolls_back_failed_funding() {
    use factory_core::{
        CreatorAllocationRelease, LbpCreationStage, LbpLaunchState, WeightedInputs,
    };
    const FACTORY: AccountId = AccountId::new([17; 32]);
    for release in [
        CreatorAllocationRelease::AfterEnd,
        CreatorAllocationRelease::AfterSetup,
    ] {
        let key = PrivateKey::try_new([23; 32]).unwrap();
        let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
        let salt = [24; 32];
        let factory = factory_core::compute_factory_pda(NAMESPACE, FACTORY, salt);
        let launch = factory_core::compute_definition_pda(NAMESPACE, FACTORY, salt);
        let mint = factory_core::compute_mint_pda(NAMESPACE, FACTORY, salt);
        let escrow = factory_core::compute_escrow_pda(NAMESPACE, FACTORY, salt);
        let role = factory_core::compute_creator_role_pda(NAMESPACE, FACTORY, salt);
        let pool = lbp_core::compute_pool_pda(NAMESPACE, LBP, launch, COLLATERAL, factory);
        let authority = factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID;
        let mut state = V03State::new()
            .with_programs_at(
                [
                    (
                        FACTORY,
                        Program::new(Cow::Borrowed(launchpad_methods::FACTORY_ELF)).unwrap(),
                    ),
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
                    (
                        authority,
                        Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_AUTHORITY_ELF))
                            .unwrap(),
                    ),
                    (clock_core::clock_account_id(), programs::clock()),
                ]
                .map(|(address, program)| (address, program, true)),
            )
            .with_public_accounts([
                account(
                    Actor::new(COLLATERAL, TOKEN_PROGRAM_ACCOUNT_ID),
                    borsh::to_vec(&TokenState::Definition {
                        name: "Collateral".into(),
                        kind: TokenKind::Fungible,
                        supply: 10_000,
                        metadata: None,
                    })
                    .unwrap(),
                ),
                account(
                    holding(creator, COLLATERAL),
                    borsh::to_vec(&TokenState::Holding {
                        owner: creator,
                        definition: COLLATERAL,
                        kind: TokenKind::Fungible,
                        balance: 99,
                    })
                    .unwrap(),
                ),
            ]);
        state = fixed_config(clock_state(state, 100_000), 100, TREASURY, false);
        let mut actors = vec![
            Actor::new(factory, FACTORY),
            Actor::new(pool, LBP),
            Actor::new(role, authority),
            Actor::new(creator, authority),
            Actor::new(launch, TOKEN_PROGRAM_ACCOUNT_ID),
            Actor::new(COLLATERAL, TOKEN_PROGRAM_ACCOUNT_ID),
            Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
            Actor::new(
                clock_core::CLOCK_PROGRAM_ACCOUNT_ID,
                clock_core::clock_account_id(),
            ),
        ];
        for owner in [factory, mint, escrow, pool, creator, role, launch] {
            actors.push(Actor::new(owner, FACTORY));
            actors.push(Actor::new(owner, TOKEN_PROGRAM_ACCOUNT_ID));
            actors.push(holding(owner, launch));
            actors.push(holding(owner, COLLATERAL));
        }
        let execute_at = |state: &mut V03State,
                          instruction: factory_core::Message,
                          timestamp: u64| {
            let mut message = Message::try_new(
                Actor::new(factory, FACTORY),
                actors.clone(),
                BTreeMap::from([(creator, state.get_account_by_id(creator).nonce)]),
                instruction,
            )
            .unwrap();
            message.admission_evidence = vec![
                lee::PublicAccountEvidence::Pda {
                    program: FACTORY,
                    seed: factory_core::compute_factory_seed(NAMESPACE, salt),
                },
                lee::PublicAccountEvidence::Pda {
                    program: LBP,
                    seed: lbp_core::compute_pool_pda_seed(NAMESPACE, launch, COLLATERAL, factory),
                },
            ];
            for owner in [factory, mint, escrow, pool, creator] {
                for definition in [launch, COLLATERAL] {
                    message
                        .admission_evidence
                        .push(lee::PublicAccountEvidence::Pda {
                            program: TOKEN_PROGRAM_ACCOUNT_ID,
                            seed: token::holding_seed(owner, definition),
                        });
                }
            }
            let witnesses = WitnessSet::for_message(&message, &[&key]);
            state.transition_from_public_transaction(
                &PublicTransaction::new(message, witnesses),
                1,
                timestamp,
            )
        };
        let execute = |state: &mut V03State, instruction| execute_at(state, instruction, 100_000);
        let creation = factory_core::Message::CreateWeightedLbp {
            inputs: WeightedInputs {
                namespace: NAMESPACE,
                launch_salt: salt,
                name: "Weighted".into(),
                uri: "https://example.test/weighted".into(),
                sale_inventory: 800,
                creator_allocation: 200,
                collateral_seed: 100,
                token_weight_start: 95 * lbp_core::WEIGHT_TOTAL / 100 + 1,
                token_weight_end: lbp_core::WEIGHT_TOTAL / 2,
                start_timestamp: 150,
                end_timestamp: 300,
                allocation_release: release,
                namespace_program_id: CURVE,
                lbp_program_id: LBP,
                collateral_definition_id: COLLATERAL,
                creator,
            },
        };
        for timestamp in [150_001, 299_999, 300_000] {
            let mut rejected = state.clone();
            assert!(matches!(
                execute_at(&mut rejected, creation.clone(), timestamp),
                Err(lee::error::LeeError::OutOfValidityWindow)
            ));
            assert!(rejected == state, "late creation changed state or nonce");
        }
        // The saved-clock check also runs when inclusion alone would permit setup.
        let mut future_clock = clock_state(state.clone(), 150_001);
        let unchanged = future_clock.clone();
        assert!(
            matches!(execute_at(&mut future_clock, creation.clone(), 150_000),
            Err(lee::error::LeeError::ProgramExecutionFailed(message)) if message.contains("InvalidTerms"))
        );
        assert!(
            future_clock == unchanged,
            "rejected clock changed creation effects or nonce"
        );
        let mut at_start = clock_state(state.clone(), 150_000);
        execute_at(&mut at_start, creation.clone(), 150_000).unwrap();
        execute(&mut state, creation).unwrap();
        assert_eq!(balance(&state, mint, launch), 1000);
        for stage in [LbpCreationStage::Minted, LbpCreationStage::Allocated] {
            for timestamp in [300_000, 300_001] {
                let mut rejected = state.clone();
                assert!(matches!(
                    execute_at(
                        &mut rejected,
                        factory_core::Message::ContinueWeightedCreation {
                            expected_stage: stage,
                            creator
                        },
                        timestamp
                    ),
                    Err(lee::error::LeeError::OutOfValidityWindow)
                ));
                assert!(rejected == state, "late setup changed state or nonce");
            }
            let mut ended_clock = clock_state(state.clone(), 300_000);
            let unchanged = ended_clock.clone();
            assert!(matches!(execute_at(&mut ended_clock,
                factory_core::Message::ContinueWeightedCreation { expected_stage: stage, creator }, 299_999),
                Err(lee::error::LeeError::ProgramExecutionFailed(message)) if message.contains("InvalidTerms")));
            assert!(
                ended_clock == unchanged,
                "rejected saved deadline changed setup effects or nonce"
            );
            let mut before_end = state.clone();
            execute_at(
                &mut before_end,
                factory_core::Message::ContinueWeightedCreation {
                    expected_stage: stage,
                    creator,
                },
                299_999,
            )
            .unwrap();
            execute(
                &mut state,
                factory_core::Message::ContinueWeightedCreation {
                    expected_stage: stage,
                    creator,
                },
            )
            .unwrap();
        }
        assert_eq!(balance(&state, factory, launch), 800);
        assert_eq!(balance(&state, escrow, launch), 200);
        assert!(!fixed_pool(&state, pool).funded);
        let instruction = || factory_core::Message::ContinueWeightedCreation {
            expected_stage: LbpCreationStage::PoolPrepared,
            creator,
        };
        let before = state.clone();
        assert!(execute(&mut state, instruction()).is_err());
        assert!(state == before, "failed setup changed runtime state");
        // A donation cannot replace the creator's seed.
        state = state.with_public_accounts([account(
            holding(pool, COLLATERAL),
            borsh::to_vec(&TokenState::Holding {
                owner: pool,
                definition: COLLATERAL,
                kind: TokenKind::Fungible,
                balance: 41,
            })
            .unwrap(),
        )]);
        let before = state.clone();
        assert!(execute(&mut state, instruction()).is_err());
        assert!(state == before, "failed setup changed runtime state");
        state = state.with_public_accounts([account(
            holding(creator, COLLATERAL),
            borsh::to_vec(&TokenState::Holding {
                owner: creator,
                definition: COLLATERAL,
                kind: TokenKind::Fungible,
                balance: 100,
            })
            .unwrap(),
        )]);
        if release == CreatorAllocationRelease::AfterSetup {
            let creator_tokens = |balance| {
                account(
                    holding(creator, launch),
                    borsh::to_vec(&TokenState::Holding {
                        owner: creator,
                        definition: launch,
                        kind: TokenKind::Fungible,
                        balance,
                    })
                    .unwrap(),
                )
            };
            state = state.with_public_accounts([creator_tokens(u128::MAX)]);
            let before = state.clone();
            assert!(execute(&mut state, instruction()).is_err());
            assert!(
                state == before,
                "failed allocation credit changed funding, activation or nonce"
            );
            state = state.with_public_accounts([creator_tokens(0)]);
        }
        for timestamp in [300_000, 300_001] {
            let mut rejected = state.clone();
            assert!(matches!(
                execute_at(&mut rejected, instruction(), timestamp),
                Err(lee::error::LeeError::OutOfValidityWindow)
            ));
            assert!(
                rejected == state,
                "late activation changed transfers, state or nonce"
            );
        }
        let mut before_end = state.clone();
        execute_at(&mut before_end, instruction(), 299_999).unwrap();
        execute(&mut state, instruction()).unwrap();
        let launch_state =
            LbpLaunchState::decode(state.get_account_by_id(factory).data.actor_state(FACTORY))
                .unwrap();
        assert_eq!(launch_state.creation_stage, LbpCreationStage::Complete);
        assert_eq!(balance(&state, pool, launch), 800);
        assert_eq!(balance(&state, pool, COLLATERAL), 141);
        assert_eq!(balance(&state, creator, COLLATERAL), 0);
        if release == CreatorAllocationRelease::AfterSetup {
            assert_eq!(balance(&state, creator, launch), 200);
        } else {
            assert!(
                state
                    .get_account_by_id(holding(creator, launch).account_id)
                    .data
                    .actor_state(TOKEN_PROGRAM_ACCOUNT_ID)
                    .is_empty()
            );
        }
        assert_eq!(
            balance(&state, escrow, launch),
            if release == CreatorAllocationRelease::AfterEnd {
                200
            } else {
                0
            }
        );
        let sale = fixed_pool(&state, pool);
        assert!(sale.funded);
        assert!(matches!(
            sale.mode,
            lbp_core::PoolMode::Weighted {
                pricing_collateral: 100,
                ..
            }
        ));
        state = clock_state(state, 150_000).with_public_accounts([account(
            holding(creator, COLLATERAL),
            borsh::to_vec(&TokenState::Holding {
                owner: creator,
                definition: COLLATERAL,
                kind: TokenKind::Fungible,
                balance: 1000,
            })
            .unwrap(),
        )]);
        fixed_execute_at(
            &mut state,
            pool,
            &key,
            lbp_core::Message::BuyWeightedExactOutput {
                amount_out: 10,
                max_amount_in: 100,
            },
            150_000,
        )
        .unwrap();
        assert_eq!(balance(&state, pool, launch), 790);
        assert_eq!(
            balance(&state, creator, launch),
            if release == CreatorAllocationRelease::AfterSetup {
                210
            } else {
                10
            }
        );
    }
}

#[test]
fn weighted_clock_failure_and_freeze_preserve_state() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (state, pool) = weighted_state(buyer);
    let buy = lbp_core::Message::BuyWeightedExactInput {
        amount_in: 50,
        min_amount_out: 1,
    };
    // Missing or malformed fixture time must fail before any trade commits.
    for bytes in [vec![], vec![1, 2, 3]] {
        let mut invalid = state
            .clone()
            .with_public_accounts([account(clock_core::clock_actor(), bytes)]);
        fixed_rejected(
            &mut invalid,
            pool,
            &key,
            buy.clone(),
            "ClockAccountData deserialization should not fail",
        );
    }
    let mut frozen = fixed_config(state, 333, TREASURY, true);
    fixed_rejected(&mut frozen, pool, &key, buy, "Frozen");
    fixed_execute(&mut frozen, pool, &key, lbp_core::Message::CollectFees).unwrap();
}

// ---------------------------------------------------------------------------
// Factory-level weighted controls and settlement guest tests
// ---------------------------------------------------------------------------

/// Build a complete weighted factory environment, run setup to completion, execute
/// a buy, then return the state ready for controls and settlement testing.
fn weighted_factory_env() -> (
    V03State,
    AccountId, // factory
    AccountId, // pool
    AccountId, // launch (token definition)
    AccountId, // escrow
    PrivateKey,
    Vec<Actor>, // actors for factory transactions
    [u8; 32],   // salt
) {
    use factory_core::{CreatorAllocationRelease, LbpCreationStage, WeightedInputs};
    const FACTORY: AccountId = AccountId::new([17; 32]);
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
    let salt = [24; 32];
    let factory = factory_core::compute_factory_pda(NAMESPACE, FACTORY, salt);
    let launch = factory_core::compute_definition_pda(NAMESPACE, FACTORY, salt);
    let mint = factory_core::compute_mint_pda(NAMESPACE, FACTORY, salt);
    let escrow = factory_core::compute_escrow_pda(NAMESPACE, FACTORY, salt);
    let role = factory_core::compute_creator_role_pda(NAMESPACE, FACTORY, salt);
    let pool = lbp_core::compute_pool_pda(NAMESPACE, LBP, launch, COLLATERAL, factory);
    let authority = factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID;
    let mut state = V03State::new()
        .with_programs_at(
            [
                (
                    FACTORY,
                    Program::new(Cow::Borrowed(launchpad_methods::FACTORY_ELF)).unwrap(),
                ),
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
                (
                    authority,
                    Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_AUTHORITY_ELF)).unwrap(),
                ),
                (clock_core::clock_account_id(), programs::clock()),
            ]
            .map(|(address, program)| (address, program, true)),
        )
        .with_public_accounts([
            account(
                Actor::new(COLLATERAL, TOKEN_PROGRAM_ACCOUNT_ID),
                borsh::to_vec(&TokenState::Definition {
                    name: "Collateral".into(),
                    kind: TokenKind::Fungible,
                    supply: 10_000,
                    metadata: None,
                })
                .unwrap(),
            ),
            account(
                holding(creator, COLLATERAL),
                borsh::to_vec(&TokenState::Holding {
                    owner: creator,
                    definition: COLLATERAL,
                    kind: TokenKind::Fungible,
                    balance: 1000,
                })
                .unwrap(),
            ),
            account(
                holding(TREASURY, COLLATERAL),
                borsh::to_vec(&TokenState::Holding {
                    owner: TREASURY,
                    definition: COLLATERAL,
                    kind: TokenKind::Fungible,
                    balance: 0,
                })
                .unwrap(),
            ),
            account(
                holding(NEXT_TREASURY, COLLATERAL),
                borsh::to_vec(&TokenState::Holding {
                    owner: NEXT_TREASURY,
                    definition: COLLATERAL,
                    kind: TokenKind::Fungible,
                    balance: 0,
                })
                .unwrap(),
            ),
        ]);
    state = fixed_config(clock_state(state, 100_000), 100, TREASURY, false);
    let mut actors = vec![
        Actor::new(factory, FACTORY),
        Actor::new(pool, LBP),
        Actor::new(role, authority),
        Actor::new(creator, authority),
        Actor::new(launch, TOKEN_PROGRAM_ACCOUNT_ID),
        Actor::new(COLLATERAL, TOKEN_PROGRAM_ACCOUNT_ID),
        Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
        Actor::new(
            clock_core::CLOCK_PROGRAM_ACCOUNT_ID,
            clock_core::clock_account_id(),
        ),
        holding(TREASURY, launch),
        holding(NEXT_TREASURY, launch),
        holding(TREASURY, COLLATERAL),
        holding(NEXT_TREASURY, COLLATERAL),
    ];
    for owner in [factory, mint, escrow, pool, creator, role, launch] {
        actors.push(Actor::new(owner, FACTORY));
        actors.push(Actor::new(owner, TOKEN_PROGRAM_ACCOUNT_ID));
        actors.push(holding(owner, launch));
        actors.push(holding(owner, COLLATERAL));
    }
    let execute_at = |state: &mut V03State, instruction: factory_core::Message, timestamp: u64| {
        let mut message = Message::try_new(
            Actor::new(factory, FACTORY),
            actors.clone(),
            BTreeMap::from([(creator, state.get_account_by_id(creator).nonce)]),
            instruction,
        )
        .unwrap();
        message.admission_evidence = vec![
            lee::PublicAccountEvidence::Pda {
                program: FACTORY,
                seed: factory_core::compute_factory_seed(NAMESPACE, salt),
            },
            lee::PublicAccountEvidence::Pda {
                program: LBP,
                seed: lbp_core::compute_pool_pda_seed(NAMESPACE, launch, COLLATERAL, factory),
            },
        ];
        for owner in [factory, mint, escrow, pool, creator] {
            for definition in [launch, COLLATERAL] {
                message
                    .admission_evidence
                    .push(lee::PublicAccountEvidence::Pda {
                        program: TOKEN_PROGRAM_ACCOUNT_ID,
                        seed: token::holding_seed(owner, definition),
                    });
            }
        }
        let witnesses = WitnessSet::for_message(&message, &[&key]);
        state.transition_from_public_transaction(
            &PublicTransaction::new(message, witnesses),
            1,
            timestamp,
        )
    };
    let execute = |state: &mut V03State, instruction| execute_at(state, instruction, 100_000);
    // Create the weighted LBP
    execute(
        &mut state,
        factory_core::Message::CreateWeightedLbp {
            inputs: WeightedInputs {
                namespace: NAMESPACE,
                launch_salt: salt,
                name: "Weighted".into(),
                uri: "https://example.test/weighted".into(),
                sale_inventory: 800,
                creator_allocation: 200,
                collateral_seed: 100,
                token_weight_start: lbp_core::WEIGHT_TOTAL / 2,
                token_weight_end: lbp_core::WEIGHT_TOTAL / 2,
                start_timestamp: 150,
                end_timestamp: 300,
                allocation_release: CreatorAllocationRelease::AfterEnd,
                namespace_program_id: CURVE,
                lbp_program_id: LBP,
                collateral_definition_id: COLLATERAL,
                creator,
            },
        },
    )
    .unwrap();
    // Run through all creation stages
    for stage in [LbpCreationStage::Minted, LbpCreationStage::Allocated] {
        execute(
            &mut state,
            factory_core::Message::ContinueWeightedCreation {
                expected_stage: stage,
                creator,
            },
        )
        .unwrap();
    }
    // Fund the creator with enough collateral for the seed
    state = state.with_public_accounts([account(
        holding(creator, COLLATERAL),
        borsh::to_vec(&TokenState::Holding {
            owner: creator,
            definition: COLLATERAL,
            kind: TokenKind::Fungible,
            balance: 1000,
        })
        .unwrap(),
    )]);
    // Complete pool setup (transfers seed, activates)
    execute(
        &mut state,
        factory_core::Message::ContinueWeightedCreation {
            expected_stage: LbpCreationStage::PoolPrepared,
            creator,
        },
    )
    .unwrap();
    assert!(fixed_pool(&state, pool).funded);
    // Set clock for buy window and execute a buy
    state = clock_state(state, 150_000);
    fixed_execute_at(
        &mut state,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactInput {
            amount_in: 50,
            min_amount_out: 1,
        },
        150_000,
    )
    .unwrap();
    (state, factory, pool, launch, escrow, key, actors, salt)
}

fn factory_execute_at(
    state: &mut V03State,
    factory: AccountId,
    actors: &[Actor],
    key: &PrivateKey,
    salt: [u8; 32],
    launch: AccountId,
    pool: AccountId,
    instruction: factory_core::Message,
    timestamp: u64,
) -> Result<(), lee::error::LeeError> {
    const FACTORY_PROGRAM: AccountId = AccountId::new([17; 32]);
    let creator = AccountId::from(&PublicKey::new_from_private_key(key));
    let mut message = Message::try_new(
        Actor::new(factory, FACTORY_PROGRAM),
        actors.to_vec(),
        BTreeMap::from([(creator, state.get_account_by_id(creator).nonce)]),
        instruction,
    )
    .unwrap();
    message.admission_evidence = vec![
        lee::PublicAccountEvidence::Pda {
            program: FACTORY_PROGRAM,
            seed: factory_core::compute_factory_seed(NAMESPACE, salt),
        },
        lee::PublicAccountEvidence::Pda {
            program: LBP,
            seed: lbp_core::compute_pool_pda_seed(NAMESPACE, launch, COLLATERAL, factory),
        },
    ];
    let mint = factory_core::compute_mint_pda(NAMESPACE, FACTORY_PROGRAM, salt);
    let escrow = factory_core::compute_escrow_pda(NAMESPACE, FACTORY_PROGRAM, salt);
    for owner in [
        factory,
        mint,
        escrow,
        pool,
        creator,
        TREASURY,
        NEXT_TREASURY,
    ] {
        for definition in [launch, COLLATERAL] {
            message
                .admission_evidence
                .push(lee::PublicAccountEvidence::Pda {
                    program: TOKEN_PROGRAM_ACCOUNT_ID,
                    seed: token::holding_seed(owner, definition),
                });
        }
    }
    let witnesses = WitnessSet::for_message(&message, &[key]);
    state
        .transition_from_public_transaction(
            &PublicTransaction::new(message, witnesses),
            1,
            timestamp,
        )
        .map(|_| ())
}

#[test]
fn weighted_factory_controls_and_settlement_full_flow() {
    let (mut state, factory, pool, launch, escrow, key, actors, salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));

    let sale = fixed_pool(&state, pool);
    assert!(sale.funded);
    assert!(sale.accrued_fees > 0 || sale.recorded_collateral > 100);

    // --- Pause ---
    // Pause before end must succeed
    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::SetWeightedPaused {
            creator,
            paused: true,
        },
        150_000,
    )
    .unwrap();

    // Buys must fail while paused
    let buy_result = fixed_execute_at(
        &mut state,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactInput {
            amount_in: 10,
            min_amount_out: 1,
        },
        150_000,
    );
    assert!(buy_result.is_err());

    // Repeated pause is a no-op (succeeds)
    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::SetWeightedPaused {
            creator,
            paused: true,
        },
        150_000,
    )
    .unwrap();

    // --- Resume ---
    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::SetWeightedPaused {
            creator,
            paused: false,
        },
        150_000,
    )
    .unwrap();

    // Buys work after resume
    fixed_execute_at(
        &mut state,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactInput {
            amount_in: 10,
            min_amount_out: 1,
        },
        150_000,
    )
    .unwrap();

    // --- Collect fees (permissionless) ---
    fixed_execute_at(
        &mut state,
        pool,
        &key,
        lbp_core::Message::CollectFees,
        150_000,
    )
    .unwrap();
    let fee_collected = balance(&state, TREASURY, COLLATERAL);
    assert!(fee_collected > 0, "fees should have been collected");

    // --- Settlement (after end) ---
    // Move clock past end (300 seconds → 300_000 ms)
    state = clock_state(state, 300_000);

    // Record balances before settlement
    let creator_collateral_before = balance(&state, creator, COLLATERAL);
    let pool_token_before = balance(&state, pool, launch);
    let sale_before = fixed_pool(&state, pool);
    let recorded = sale_before.recorded_collateral;
    let fees = sale_before.accrued_fees;
    let unsold = sale_before.remaining_inventory;

    // Settle weighted proceeds
    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::WithdrawWeightedProceeds { creator },
        300_000,
    )
    .unwrap();

    let sale_after = fixed_pool(&state, pool);
    assert!(sale_after.proceeds_settled);
    assert_eq!(sale_after.recorded_collateral, 0);
    assert_eq!(sale_after.accrued_fees, 0);

    // Creator received proceeds
    let creator_collateral_after = balance(&state, creator, COLLATERAL);
    assert_eq!(
        creator_collateral_after,
        creator_collateral_before + recorded
    );

    // Treasury received remaining fees
    let treasury_after = balance(&state, TREASURY, COLLATERAL);
    assert_eq!(treasury_after, fee_collected + fees);

    // Unsold inventory was burned (pool token balance decreased by unsold)
    let pool_token_after = balance(&state, pool, launch);
    assert_eq!(pool_token_after, pool_token_before - unsold);

    // --- Repeated settlement is idempotent ---
    let before_retry = state.clone();
    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::WithdrawWeightedProceeds { creator },
        300_000,
    )
    .unwrap();
    // Balances unchanged after retry
    assert_eq!(
        balance(&state, creator, COLLATERAL),
        creator_collateral_after
    );

    // --- Allocation claim (AfterEnd) ---
    let escrow_tokens = balance(&state, escrow, launch);
    assert_eq!(escrow_tokens, 200);
    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::ClaimWeightedAllocation { creator },
        300_000,
    )
    .unwrap();
    assert_eq!(
        balance(&state, creator, launch),
        balance(&before_retry, creator, launch) + 200
    );
    assert_eq!(balance(&state, escrow, launch), 0);

    // Repeated allocation claim is idempotent
    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::ClaimWeightedAllocation { creator },
        300_000,
    )
    .unwrap();
}

#[test]
fn weighted_settlement_before_end_rejected_by_inclusion_window() {
    let (mut state, factory, pool, launch, _escrow, key, actors, salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));

    // Settlement at 299_999 ms (before the 300s end = 300_000 ms) must be rejected
    state = clock_state(state, 299_999);
    let before = state.clone();
    let result = factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::WithdrawWeightedProceeds { creator },
        299_999,
    );
    assert!(
        matches!(result, Err(lee::error::LeeError::OutOfValidityWindow)),
        "early settlement should be rejected: {result:?}"
    );
    assert!(state == before, "rejected settlement changed state");
}

#[test]
fn weighted_controls_after_end_rejected_by_inclusion_window() {
    let (mut state, factory, pool, launch, _escrow, key, actors, salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));

    // Pause after end must be rejected (pool enforces time bound before end)
    state = clock_state(state, 300_000);
    let before = state.clone();
    let result = factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::SetWeightedPaused {
            creator,
            paused: true,
        },
        300_000,
    );
    assert!(
        matches!(result, Err(lee::error::LeeError::OutOfValidityWindow)),
        "late pause should be rejected: {result:?}"
    );
    assert!(state == before, "rejected pause changed state");
}

#[test]
fn weighted_unauthorized_factory_controls_rejected() {
    let (mut state, factory, pool, launch, _escrow, _key, actors, salt) = weighted_factory_env();

    // Use a different key that doesn't have creator authority
    let wrong_key = PrivateKey::try_new([99; 32]).unwrap();
    let wrong_caller = AccountId::from(&PublicKey::new_from_private_key(&wrong_key));

    // The factory message references a creator that signed, but the authority
    // check should fail because wrong_caller doesn't hold the creator role.
    // We try the correct creator ID but with the wrong signer.
    let mut actors_with_wrong = actors.clone();
    actors_with_wrong.push(Actor::new(
        wrong_caller,
        factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
    ));
    actors_with_wrong.push(Actor::new(wrong_caller, TOKEN_PROGRAM_ACCOUNT_ID));
    actors_with_wrong.push(holding(wrong_caller, launch));
    actors_with_wrong.push(holding(wrong_caller, COLLATERAL));

    let result = factory_execute_at(
        &mut state,
        factory,
        &actors_with_wrong,
        &wrong_key,
        salt,
        launch,
        pool,
        factory_core::Message::SetWeightedPaused {
            creator: wrong_caller,
            paused: true,
        },
        150_000,
    );
    assert!(result.is_err(), "unauthorized pause should fail");
}

#[test]
fn weighted_frozen_buys_blocked_but_settlement_allowed() {
    let (mut state, factory, pool, launch, _escrow, key, actors, salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));

    // Freeze the namespace
    state = fixed_config(state, 100, TREASURY, true);

    // Buys must fail when frozen
    let buy_result = fixed_execute_at(
        &mut state,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactInput {
            amount_in: 10,
            min_amount_out: 1,
        },
        150_000,
    );
    assert!(buy_result.is_err());

    // Fee collection must still work when frozen
    fixed_execute_at(
        &mut state,
        pool,
        &key,
        lbp_core::Message::CollectFees,
        150_000,
    )
    .unwrap();

    // Settlement must work when frozen (after end)
    state = clock_state(fixed_config(state, 100, TREASURY, true), 300_000);
    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::WithdrawWeightedProceeds { creator },
        300_000,
    )
    .unwrap();
    assert!(fixed_pool(&state, pool).proceeds_settled);

    // Allocation claim must work when frozen
    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::ClaimWeightedAllocation { creator },
        300_000,
    )
    .unwrap();
}

#[test]
fn weighted_allocation_before_settlement_then_settlement() {
    let (mut state, factory, pool, launch, escrow, key, actors, salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));

    // Move past end
    state = clock_state(state, 300_000);

    // Claim allocation FIRST (before settlement)
    let creator_tokens_before = balance(&state, creator, launch);
    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::ClaimWeightedAllocation { creator },
        300_000,
    )
    .unwrap();
    assert_eq!(
        balance(&state, creator, launch),
        creator_tokens_before + 200
    );
    assert_eq!(balance(&state, escrow, launch), 0);

    // Then settle proceeds
    let creator_collateral_before = balance(&state, creator, COLLATERAL);
    let sale = fixed_pool(&state, pool);
    let recorded = sale.recorded_collateral;
    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::WithdrawWeightedProceeds { creator },
        300_000,
    )
    .unwrap();
    assert!(fixed_pool(&state, pool).proceeds_settled);
    assert_eq!(
        balance(&state, creator, COLLATERAL),
        creator_collateral_before + recorded
    );
}

#[test]
fn weighted_settlement_failures_roll_back_all_effects_and_retry_once() {
    let (state, factory, pool, launch, escrow, key, actors, salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
    let state = clock_state(state, 300_000);
    let sale = fixed_pool(&state, pool);
    assert!(sale.accrued_fees > 0);
    assert!(sale.remaining_inventory > 0);
    let definition_actor = Actor::new(launch, TOKEN_PROGRAM_ACCOUNT_ID);
    let definition_bytes = state
        .get_account_by_id(launch)
        .data
        .actor_state(TOKEN_PROGRAM_ACCOUNT_ID)
        .to_vec();
    let definition = TokenState::decode(&definition_bytes).unwrap();
    let TokenState::Definition { supply, .. } = definition.clone() else {
        panic!("expected definition")
    };

    // Fail the first transfer, the burn after fee payment, or the final credit
    // after fee payment and burn. Each failure must restore the entire runtime.
    for fault in ["fee", "burn", "proceeds"] {
        let (actor, original, broken, reason) = if fault == "burn" {
            let mut broken = definition.clone();
            let TokenState::Definition { supply, .. } = &mut broken else {
                unreachable!()
            };
            *supply = 0;
            (
                definition_actor,
                definition_bytes.clone(),
                borsh::to_vec(&broken).unwrap(),
                "InsufficientBalance",
            )
        } else {
            let owner = if fault == "fee" { TREASURY } else { creator };
            let actor = holding(owner, COLLATERAL);
            let original = state
                .get_account_by_id(actor.account_id)
                .data
                .actor_state(TOKEN_PROGRAM_ACCOUNT_ID)
                .to_vec();
            let broken = borsh::to_vec(&TokenState::Holding {
                owner,
                definition: COLLATERAL,
                kind: TokenKind::Fungible,
                balance: u128::MAX,
            })
            .unwrap();
            (actor, original, broken, "Overflow")
        };
        let mut rejected = state.clone().with_public_accounts([account(actor, broken)]);
        let before = rejected.clone();
        let nonce = rejected.get_account_by_id(creator).nonce;
        let result = factory_execute_at(
            &mut rejected,
            factory,
            &actors,
            &key,
            salt,
            launch,
            pool,
            factory_core::Message::WithdrawWeightedProceeds { creator },
            300_000,
        );
        assert!(
            matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains(reason)),
            "{fault}: expected {reason}, got {result:?}"
        );
        assert!(
            rejected == before,
            "{fault}: failed settlement changed runtime state"
        );
        assert_eq!(rejected.get_account_by_id(creator).nonce, nonce);

        let mut repaired = rejected.with_public_accounts([account(actor, original)]);
        let creator_before = balance(&repaired, creator, COLLATERAL);
        let treasury_before = balance(&repaired, TREASURY, COLLATERAL);
        let tokens_before = balance(&repaired, pool, launch);
        for _ in 0..2 {
            factory_execute_at(
                &mut repaired,
                factory,
                &actors,
                &key,
                salt,
                launch,
                pool,
                factory_core::Message::WithdrawWeightedProceeds { creator },
                300_000,
            )
            .unwrap();
            assert!(fixed_pool(&repaired, pool).proceeds_settled);
            assert_eq!(
                balance(&repaired, creator, COLLATERAL),
                creator_before + sale.recorded_collateral
            );
            assert_eq!(
                balance(&repaired, TREASURY, COLLATERAL),
                treasury_before + sale.accrued_fees
            );
            assert_eq!(
                balance(&repaired, pool, launch),
                tokens_before - sale.remaining_inventory
            );
            let current = TokenState::decode(
                repaired
                    .get_account_by_id(launch)
                    .data
                    .actor_state(TOKEN_PROGRAM_ACCOUNT_ID),
            )
            .unwrap();
            let TokenState::Definition {
                supply: current_supply,
                ..
            } = current
            else {
                panic!("expected definition")
            };
            assert_eq!(current_supply, supply - sale.remaining_inventory);
            assert_eq!(balance(&repaired, escrow, launch), sale.creator_allocation);
        }
    }
}

#[test]
fn weighted_allocation_credit_failure_rolls_back_and_retry_pays_once() {
    let (state, factory, pool, launch, escrow, key, actors, salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
    let state = clock_state(state, 300_000);
    let recipient = holding(creator, launch);
    let original = state
        .get_account_by_id(recipient.account_id)
        .data
        .actor_state(TOKEN_PROGRAM_ACCOUNT_ID)
        .to_vec();
    let mut rejected = state.with_public_accounts([account(
        recipient,
        borsh::to_vec(&TokenState::Holding {
            owner: creator,
            definition: launch,
            kind: TokenKind::Fungible,
            balance: u128::MAX,
        })
        .unwrap(),
    )]);
    let before = rejected.clone();
    let nonce = rejected.get_account_by_id(creator).nonce;
    let result = factory_execute_at(
        &mut rejected,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::ClaimWeightedAllocation { creator },
        300_000,
    );
    assert!(
        matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains("Overflow")),
        "expected allocation credit overflow, got {result:?}"
    );
    assert!(
        rejected == before,
        "failed allocation changed runtime state"
    );
    assert_eq!(rejected.get_account_by_id(creator).nonce, nonce);
    let mut repaired = rejected.with_public_accounts([account(recipient, original)]);
    let balance_before = balance(&repaired, creator, launch);
    let allocation = balance(&repaired, escrow, launch);
    assert!(allocation > 0);
    for _ in 0..2 {
        factory_execute_at(
            &mut repaired,
            factory,
            &actors,
            &key,
            salt,
            launch,
            pool,
            factory_core::Message::ClaimWeightedAllocation { creator },
            300_000,
        )
        .unwrap();
        assert_eq!(
            balance(&repaired, creator, launch),
            balance_before + allocation
        );
        assert_eq!(balance(&repaired, escrow, launch), 0);
    }
}

#[test]
fn weighted_stale_clock_at_inclusion_boundary_preserves_state() {
    let (state, _factory, pool, _launch, _escrow, key, _actors, _salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));

    // Build a transaction with pricing time inside window but test inclusion time outside
    let buy = lbp_core::Message::BuyWeightedExactInput {
        amount_in: 10,
        min_amount_out: 1,
    };
    let transaction = fixed_transaction(&state, pool, &key, buy);

    // Inclusion time before start must be rejected
    for timestamp in [149_999u64] {
        let mut rejected = state.clone();
        let before = rejected.clone();
        let nonce = rejected.get_account_by_id(creator).nonce;
        assert!(matches!(
            rejected.transition_from_public_transaction(&transaction, 1, timestamp),
            Err(lee::error::LeeError::OutOfValidityWindow)
        ));
        assert!(
            rejected == before,
            "rejected buy changed state at {timestamp}"
        );
        assert_eq!(rejected.get_account_by_id(creator).nonce, nonce);
    }

    // Inclusion time after end must be rejected
    for timestamp in [300_000u64, 300_001] {
        let mut rejected = state.clone();
        let before = rejected.clone();
        assert!(matches!(
            rejected.transition_from_public_transaction(&transaction, 1, timestamp),
            Err(lee::error::LeeError::OutOfValidityWindow)
        ));
        assert!(
            rejected == before,
            "rejected buy changed state at {timestamp}"
        );
    }
}

/// Runs the real clock guest in the node-free state fixture. Sequencer guards and
/// end-of-block ordering are outside this fixture.
fn tick_clock(state: &mut V03State, timestamp: u64) {
    let clock = clock_core::clock_actor();
    let old = clock_core::ClockAccountData::from_bytes(
        state
            .get_account_by_id(clock.account_id)
            .data
            .actor_state(clock.program_account_id),
    );
    let message = Message::try_new(
        clock,
        vec![clock],
        BTreeMap::new(),
        clock_core::Message::Tick {
            timestamp,
            block_id: old.block_id + 1,
        },
    )
    .unwrap();
    let tx = PublicTransaction::new(message.clone(), WitnessSet::for_message(&message, &[]));
    state
        .transition_from_public_transaction(&tx, old.block_id + 1, timestamp)
        .unwrap();
    let updated = clock_core::ClockAccountData::from_bytes(
        state
            .get_account_by_id(clock.account_id)
            .data
            .actor_state(clock.program_account_id),
    );
    assert_eq!(
        (updated.block_id, updated.timestamp),
        (old.block_id + 1, timestamp)
    );
}

#[test]
fn upstream_clock_tick_changes_weighted_price_and_preserves_inclusion_deadline() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (mut state, pool) = weighted_state(buyer);
    let mut sale = fixed_pool(&state, pool);
    if let lbp_core::PoolMode::Weighted {
        token_weight_end, ..
    } = &mut sale.mode
    {
        *token_weight_end = 90 * lbp_core::WEIGHT_TOTAL / 100;
    }
    state = state.with_public_accounts([account(
        Actor::new(pool, LBP),
        borsh::to_vec(&sale).unwrap(),
    )]);
    let buy = lbp_core::Message::BuyWeightedExactInput {
        amount_in: 50,
        min_amount_out: 1,
    };
    let tx = fixed_transaction(&state, pool, &key, buy.clone());
    let mut early = state.clone();
    early
        .transition_from_public_transaction(&tx, 2, 100_000)
        .unwrap();
    let early_output = balance(&early, buyer, LAUNCH);
    tick_clock(&mut state, 199_999);
    let mut later = state.clone();
    later
        .transition_from_public_transaction(&tx, 3, 199_999)
        .unwrap();
    let later_output = balance(&later, buyer, LAUNCH);
    assert!(later_output < early_output);
    // This is still the same signed buy, with saved time held before the end.
    let before = state.clone();
    assert!(matches!(
        state.transition_from_public_transaction(&tx, 3, 200_000),
        Err(lee::error::LeeError::OutOfValidityWindow)
    ));
    assert!(state == before);
    // Advancing the real clock to the end also rejects pricing itself.
    tick_clock(&mut state, 200_000);
    fixed_rejected(&mut state, pool, &key, buy, "Closed");
}

// ---------------------------------------------------------------------------
// Poke and checkpoint guest tests
// ---------------------------------------------------------------------------

fn poke_transaction(state: &V03State, pool: AccountId) -> PublicTransaction {
    let sale = fixed_pool(state, pool);
    let launch = sale.token_definition_id;
    let actors = vec![
        Actor::new(pool, LBP),
        Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
        Actor::new(pool, TOKEN_PROGRAM_ACCOUNT_ID),
        holding(pool, launch),
        holding(pool, COLLATERAL),
        lbp_core::pricing_clock::actor(),
    ];
    let message = Message::try_new(
        Actor::new(pool, LBP),
        actors,
        BTreeMap::new(),
        lbp_core::Message::PokeWeighted,
    )
    .unwrap();
    let witnesses = WitnessSet::for_message(&message, &[]);
    PublicTransaction::new(message, witnesses)
}

fn poke_execute_at(
    state: &mut V03State,
    pool: AccountId,
    timestamp: u64,
) -> Result<(), lee::error::LeeError> {
    let transaction = poke_transaction(state, pool);
    state
        .transition_from_public_transaction(&transaction, 1, timestamp)
        .map(|_| ())
}

fn checkpoint_of(state: &V03State, pool: AccountId) -> (u64, u128) {
    let sale = fixed_pool(state, pool);
    if let lbp_core::PoolMode::Weighted {
        checkpoint_time,
        checkpoint_token_weight,
        ..
    } = sale.mode
    {
        (checkpoint_time, checkpoint_token_weight)
    } else {
        panic!("expected weighted mode");
    }
}

#[test]
fn guest_poke_records_checkpoint_permissionlessly() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (mut state, pool) = weighted_state(buyer);
    // Checkpoint starts at zero
    assert_eq!(checkpoint_of(&state, pool), (0, 0));
    // Clock is at 100_000ms = 100s (set in fixed_state). Inclusion time within sale window.
    poke_execute_at(&mut state, pool, 100_000).unwrap();
    let (time, w) = checkpoint_of(&state, pool);
    assert_eq!(time, 100);
    assert!(w > 0);
}

#[test]
fn upstream_clock_ticks_advance_poke_checkpoints_once_per_second() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (mut state, pool) = weighted_state(buyer);
    for (timestamp, block, expected_events) in [
        (100_000, 1, 1),
        (100_999, 2, 0),
        (101_000, 3, 1),
        (200_000, 4, 1),
    ] {
        if block > 1 {
            tick_clock(&mut state, timestamp);
        }
        let tx = poke_transaction(&state, pool);
        let events = state
            .transition_from_public_transaction(&tx, block, timestamp)
            .unwrap();
        let checkpoint = checkpoint_of(&state, pool);
        assert_eq!(checkpoint.0, timestamp / 1000);
        assert_eq!(events.len(), expected_events);
        if expected_events == 1 {
            assert_eq!(events[0].event.data, borsh::to_vec(&checkpoint).unwrap());
        }
    }
}

#[test]
fn guest_buy_updates_checkpoint_and_poke_is_idempotent() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (mut state, pool) = weighted_state(buyer);
    // Buy updates checkpoint
    let transaction = fixed_transaction(
        &state,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactInput {
            amount_in: 50,
            min_amount_out: 1,
        },
    );
    let events = state
        .transition_from_public_transaction(&transaction, 1, 100_000)
        .unwrap();
    let cp_after_buy = checkpoint_of(&state, pool);
    assert_eq!(cp_after_buy.0, 100); // clock is at 100_000ms = 100s
    assert!(cp_after_buy.1 > 0);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].account_id, LBP);
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let hash = Impl::hash_bytes(b"lbp::WeightedCheckpoint");
    assert_eq!(events[0].event.selector, hash.as_bytes()[..8]);
    assert_eq!(events[0].event.data, borsh::to_vec(&cp_after_buy).unwrap());
    // Another buy at the same pricing second does not emit a duplicate event.
    let transaction = fixed_transaction(
        &state,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactOutput {
            amount_out: 1,
            max_amount_in: 100,
        },
    );
    let events = state
        .transition_from_public_transaction(&transaction, 1, 150_000)
        .unwrap();
    assert!(events.is_empty());
    // Poke at same time is idempotent — checkpoint unchanged
    let transaction = poke_transaction(&state, pool);
    let events = state
        .transition_from_public_transaction(&transaction, 1, 150_000)
        .unwrap();
    assert!(events.is_empty());
    assert_eq!(checkpoint_of(&state, pool), cp_after_buy);
}

#[test]
fn guest_poke_in_settlement_flow() {
    let (mut state, factory, pool, launch, _escrow, key, actors, salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
    // Poke before settlement
    poke_execute_at(&mut state, pool, 150_000).unwrap();
    let (time, _) = checkpoint_of(&state, pool);
    assert_eq!(time, 150);
    // Move past end and settle
    state = clock_state(state, 300_000);
    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::WithdrawWeightedProceeds { creator },
        300_000,
    )
    .unwrap();
    // Settlement should succeed with a checkpoint already recorded
    let sale = fixed_pool(&state, pool);
    assert!(sale.proceeds_settled);
    // Observations remain available after settlement and use the real clock.
    tick_clock(&mut state, 301_000);
    let tx = poke_transaction(&state, pool);
    let events = state
        .transition_from_public_transaction(&tx, 3, 301_000)
        .unwrap();
    assert_eq!(checkpoint_of(&state, pool).0, 301);
    assert_eq!(events.len(), 1);
}
