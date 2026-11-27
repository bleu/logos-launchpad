use lee::AccountId;

use crate::ClientError;
use crate::curve::{validate_factory_pool, validate_factory_state};

use super::fixtures::{fixed_creation, matching_factory_pool, sale, valid_factory_state};

#[test]
fn factory_state_accepts_matching_derived_ids() {
    let sale = sale();
    assert!(validate_factory_state(sale, &valid_factory_state(sale)).is_ok());
}

#[test]
fn factory_state_rejects_a_mismatched_derived_token_definition_id() {
    let sale = sale();
    let mut state = valid_factory_state(sale);
    state.token_definition_id = AccountId::new([91; 32]);
    assert!(matches!(
        validate_factory_state(sale, &state),
        Err(ClientError::FactorySaleMismatch)
    ));
}

#[test]
fn factory_state_rejects_a_mismatched_derived_creator_escrow_id() {
    let sale = sale();
    let mut state = valid_factory_state(sale);
    state.creator_escrow_id = AccountId::new([92; 32]);
    assert!(matches!(
        validate_factory_state(sale, &state),
        Err(ClientError::FactorySaleMismatch)
    ));
}

#[test]
fn factory_state_rejects_a_mismatched_derived_pool_id() {
    let sale = sale();
    let mut state = valid_factory_state(sale);
    state.pool_id = AccountId::new([93; 32]);
    assert!(matches!(
        validate_factory_state(sale, &state),
        Err(ClientError::FactorySaleMismatch)
    ));
}

#[test]
fn factory_state_rejects_a_mismatched_creator_role() {
    let mut state = valid_factory_state(sale());
    state.creator_role_id = AccountId::new([94; 32]);
    assert!(matches!(
        validate_factory_state(sale(), &state),
        Err(ClientError::FactorySaleMismatch)
    ));
}

#[test]
fn factory_state_rejects_invalid_ending_windows() {
    use curve_core::EndingPolicy::{Depletion, Either, Manual, Scheduled};
    for (ending, end) in [
        (Manual, None),
        (Manual, Some(100)),
        (Scheduled, None),
        (Either, None),
        (Depletion, Some(100)),
    ] {
        let mut state = valid_factory_state(sale());
        state.ending_policy = ending;
        state.end_timestamp = end;
        assert!(matches!(
            validate_factory_state(sale(), &state),
            Err(ClientError::FactorySaleMismatch)
        ));
    }
}

#[test]
fn loaded_factory_pool_checks_all_immutable_policy_and_pricing_terms() {
    let state = valid_factory_state(sale());
    let pool = matching_factory_pool(&state);
    validate_factory_pool(sale(), &state, &pool).unwrap();
    for change in 1..10 {
        let mut changed = pool.clone();
        match change {
            1 => changed.pool.ending_policy = curve_core::EndingPolicy::Manual,
            2 => changed.pool.close_timestamp = Some(100),
            3 => changed.pool.close_on_depletion = Some(pool::TokenSide::Token1),
            4 => changed.allowed_input_token = Some(pool::TokenSide::Token1),
            5 => changed.trade_control = curve_core::TradeControl::Running,
            6 => changed.pool.initial_k += 1,
            7 => changed.owner_program = None,
            8 => changed.token0_definition_id = AccountId::new([99; 32]),
            9 => changed.buy_policy = fixed_creation().buy_policy,
            _ => unreachable!(),
        }
        assert!(
            matches!(
                validate_factory_pool(sale(), &state, &changed),
                Err(ClientError::FactorySaleMismatch)
            ),
            "change {change}"
        );
    }
    let mut paused_state = state.clone();
    paused_state.pause_policy = curve_core::PausePolicy::AuthorityControlled;
    let mut paused_pool = pool.clone();
    paused_pool.trade_control = curve_core::TradeControl::Paused;
    validate_factory_pool(sale(), &paused_state, &paused_pool).unwrap();
    let mut traded_pool = pool;
    traded_pool.pool.virtual_reserve0 -= 1;
    traded_pool.pool.virtual_reserve1 += 1;
    traded_pool.pool.real_reserve0 -= 1;
    traded_pool.pool.real_reserve1 += 1;
    validate_factory_pool(sale(), &state, &traded_pool).unwrap();
}

#[test]
fn loaded_factory_pool_matches_each_selected_ending() {
    use curve_core::EndingPolicy::{Depletion, Either, Scheduled};
    for (ending, end) in [
        (Depletion, None),
        (Scheduled, Some(100)),
        (Either, Some(100)),
    ] {
        let mut state = valid_factory_state(sale());
        state.ending_policy = ending;
        state.end_timestamp = end;
        let pool = matching_factory_pool(&state);
        validate_factory_pool(sale(), &state, &pool).unwrap();
        for side in [
            None,
            Some(pool::TokenSide::Token0),
            Some(pool::TokenSide::Token1),
        ] {
            let mut changed = pool.clone();
            changed.pool.close_on_depletion = side;
            assert_eq!(
                validate_factory_pool(sale(), &state, &changed).is_ok(),
                side == pool.pool.close_on_depletion
            );
        }
        let mut changed = pool;
        changed.pool.ending_policy = if ending == Either { Scheduled } else { Either };
        assert!(matches!(
            validate_factory_pool(sale(), &state, &changed),
            Err(ClientError::FactorySaleMismatch)
        ));
    }
}

#[test]
fn factory_pool_binds_initial_k_and_accepts_current_k_after_retained_sell() {
    let mut state = valid_factory_state(sale());
    state.sell_fee_bps = 1_000;
    let mut pool = matching_factory_pool(&state);
    let initial = pool.pool.initial_k;
    let quote = pool
        .pool
        .swap_exact_input(pool::ExactInputSwap {
            token_in: pool::TokenSide::Token0,
            amount_in: 500,
            min_amount_out: 0,
            pool_fee_bps: 0,
            protocol_fee_bps: 500,
            now: 1,
        })
        .unwrap();
    assert!(quote.retained_fee > 0);
    assert!(pool.pool.k > initial);
    assert_eq!(pool.pool.initial_k, initial);
    validate_factory_pool(sale(), &state, &pool).unwrap();
    let mut changed = pool.clone();
    changed.pool.initial_k += 1;
    assert!(matches!(
        validate_factory_pool(sale(), &state, &changed),
        Err(ClientError::FactorySaleMismatch)
    ));
    changed = pool.clone();
    changed.pool.sell_fee_bps = 999;
    assert!(matches!(
        validate_factory_pool(sale(), &state, &changed),
        Err(ClientError::FactorySaleMismatch)
    ));
}
