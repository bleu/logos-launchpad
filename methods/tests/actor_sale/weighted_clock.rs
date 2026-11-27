use super::*;

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
        membership: None,
        amount_in: 50,
        min_amount_out: 1,
    };
    for timestamp in [100_000, 130_000, 199_999] {
        let mut valid = clock_state(state.clone(), timestamp);
        fixed_execute(&mut valid, pool, &key, buy.clone()).unwrap();
        assert!(balance(&valid, buyer, LAUNCH) > 0);
        if timestamp == 100_000 {
            assert_eq!(balance(&valid, buyer, LAUNCH), 324);
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
            membership: None,
            amount_in: 50,
            min_amount_out: 1,
        },
        lbp_core::Message::BuyWeightedExactOutput {
            membership: None,
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
            membership: None,
            amount_in: 50,
            min_amount_out: 324,
        },
    )
    .unwrap();
    let mut without_collection = state.clone();
    fixed_execute(&mut state, pool, &key, CollectFees).unwrap();
    assert_eq!(balance(&state, TREASURY, COLLATERAL), 2);
    assert_eq!(fixed_pool(&state, pool).accrued_fees, 0);
    let next_buy = BuyWeightedExactOutput {
        membership: None,
        amount_out: 100,
        max_amount_in: 27,
    };
    fixed_execute(&mut state, pool, &key, next_buy.clone()).unwrap();
    fixed_execute(&mut without_collection, pool, &key, next_buy).unwrap();
    assert_eq!(balance(&state, buyer, COLLATERAL), 1923);
    assert_eq!(balance(&state, buyer, LAUNCH), 424);
    assert_eq!(
        balance(&state, buyer, COLLATERAL),
        balance(&without_collection, buyer, COLLATERAL)
    );
    let sale = fixed_pool(&state, pool);
    assert_eq!(sale.remaining_inventory, 576);
    assert!(matches!(
        sale.mode,
        lbp_core::PoolMode::Weighted {
            pricing_collateral: 174,
            ..
        }
    ));
    assert_eq!(balance(&state, pool, COLLATERAL), 216);
    assert_eq!(sale.recorded_collateral + sale.accrued_fees, 175);
}

#[test]
fn weighted_clock_failure_and_freeze_preserve_state() {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let buyer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let (state, pool) = weighted_state(buyer);
    let buy = lbp_core::Message::BuyWeightedExactInput {
        membership: None,
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

#[test]
fn weighted_stale_clock_at_inclusion_boundary_preserves_state() {
    let (state, _factory, pool, _launch, _escrow, key, _actors, _salt) = weighted_factory_env();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));

    // Build a transaction with pricing time inside window but test inclusion time outside
    let buy = lbp_core::Message::BuyWeightedExactInput {
        membership: None,
        amount_in: 10,
        min_amount_out: 1,
    };
    let transaction = fixed_transaction(&state, pool, &key, buy);

    // Inclusion time before start must be rejected
    {
        let timestamp = 149_999u64;
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
        membership: None,
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
    let transaction = fixed_transaction(
        &state,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactInput {
            membership: None,
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
    let checkpoint_selector = event_selector(curve_core::events::WeightedCheckpoint::SELECTOR_NAME);
    let checkpoints: Vec<_> = events
        .iter()
        .filter(|event| event.account_id == LBP && event.event.selector == checkpoint_selector)
        .collect();
    assert_eq!(checkpoints.len(), 1);
    assert_eq!(
        checkpoints[0].event.data,
        borsh::to_vec(&cp_after_buy).unwrap()
    );
    // Another buy at the same pricing second does not emit a duplicate event.
    let transaction = fixed_transaction(
        &state,
        pool,
        &key,
        lbp_core::Message::BuyWeightedExactOutput {
            membership: None,
            amount_out: 1,
            max_amount_in: 100,
        },
    );
    let events = state
        .transition_from_public_transaction(&transaction, 1, 150_000)
        .unwrap();
    assert!(
        events
            .iter()
            .all(|event| event.account_id != LBP || event.event.selector != checkpoint_selector)
    );
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
    poke_execute_at(&mut state, pool, 150_000).unwrap();
    let (time, _) = checkpoint_of(&state, pool);
    assert_eq!(time, 150);
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
    let sale = fixed_pool(&state, pool);
    assert_eq!(
        sale.settlement_progress,
        lbp_core::SettlementProgress::ManualComplete
    );
    // Observations remain available after settlement and use the real clock.
    tick_clock(&mut state, 301_000);
    let tx = poke_transaction(&state, pool);
    let events = state
        .transition_from_public_transaction(&tx, 3, 301_000)
        .unwrap();
    assert_eq!(checkpoint_of(&state, pool).0, 301);
    assert_eq!(events.len(), 1);
}
