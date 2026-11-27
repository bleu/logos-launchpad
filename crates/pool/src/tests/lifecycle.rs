use super::policy_params;
use crate::{
    CloseError, CreateError, EndingDecision, EndingPolicy, ExactInputSwap, ExactOutputSwap, Pool,
    PoolCreateParams, PoolLifecycle, SwapError, TokenSide, WithdrawError,
};

#[test]
fn final_exact_input_swap_closes_a_pool_configured_to_close_on_token0_depletion() {
    let mut pool = Pool::create(PoolCreateParams {
        token0_amount: 80,
        token1_amount: 0,
        virtual_reserve0: 100,
        virtual_reserve1: 100,
        ending_policy: EndingPolicy::Depletion,
        sell_fee_bps: 0,
        close_timestamp: None,
        close_on_depletion: Some(TokenSide::Token0),
    })
    .expect("valid pool");

    let outcome = pool
        .swap_exact_input(ExactInputSwap {
            token_in: TokenSide::Token1,
            amount_in: 400,
            min_amount_out: 80,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 1,
        })
        .expect("final swap succeeds");

    assert_eq!(outcome.amount_out, 80);
    assert_eq!(pool.real_reserve0, 0);
    assert_eq!(pool.effective_lifecycle(1), PoolLifecycle::Closed);
    assert_eq!(
        pool.swap_exact_input(ExactInputSwap {
            token_in: TokenSide::Token1,
            amount_in: 1,
            min_amount_out: 0,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 1
        }),
        Err(SwapError::Closed)
    );
}

#[test]
fn final_exact_output_swap_closes_a_pool_configured_to_close_on_token0_depletion() {
    let mut pool = Pool::create(PoolCreateParams {
        token0_amount: 80,
        token1_amount: 0,
        virtual_reserve0: 100,
        virtual_reserve1: 100,
        ending_policy: EndingPolicy::Depletion,
        sell_fee_bps: 0,
        close_timestamp: None,
        close_on_depletion: Some(TokenSide::Token0),
    })
    .expect("valid pool");

    pool.swap_exact_output(ExactOutputSwap {
        token_in: TokenSide::Token1,
        amount_out: 80,
        max_amount_in: u128::MAX,
        pool_fee_bps: 0,
        protocol_fee_bps: 0,
        now: 1,
    })
    .expect("final swap succeeds");

    assert_eq!(pool.real_reserve0, 0);
    assert_eq!(pool.effective_lifecycle(1), PoolLifecycle::Closed);
}

#[test]
fn expiry_allows_full_withdrawal_without_a_close_transaction_and_retires_the_pool() {
    let mut pool = Pool::create(PoolCreateParams {
        token0_amount: 800,
        token1_amount: 25,
        virtual_reserve0: 1000,
        virtual_reserve1: 100,
        ending_policy: EndingPolicy::Scheduled,
        sell_fee_bps: 0,
        close_timestamp: Some(42),
        close_on_depletion: None,
    })
    .expect("valid pool");
    assert_eq!(pool.withdraw_reserves(42), Ok((800, 25)));
    assert_eq!(pool.real_reserve0, 0);
    assert_eq!(pool.real_reserve1, 0);
    assert_eq!(pool.lifecycle, PoolLifecycle::Withdrawn);
    assert_eq!(
        pool.withdraw_reserves(43),
        Err(WithdrawError::AlreadyWithdrawn)
    );
}

#[test]
fn swaps_are_rejected_at_the_close_timestamp_without_mutating_reserves() {
    let mut pool = Pool::create(PoolCreateParams {
        token0_amount: 800,
        token1_amount: 100,
        virtual_reserve0: 1000,
        virtual_reserve1: 100,
        ending_policy: EndingPolicy::Scheduled,
        sell_fee_bps: 0,
        close_timestamp: Some(42),
        close_on_depletion: None,
    })
    .expect("valid pool");
    let before = pool.clone();
    assert_eq!(
        pool.swap_exact_input(ExactInputSwap {
            token_in: TokenSide::Token0,
            amount_in: 250,
            min_amount_out: 20,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 42
        }),
        Err(SwapError::Closed)
    );
    assert_eq!(pool, before);
}

#[test]
fn manual_close_allows_full_withdrawal_for_a_pool_without_expiry() {
    let mut pool = Pool::create(PoolCreateParams {
        token0_amount: 800,
        token1_amount: 25,
        virtual_reserve0: 1000,
        virtual_reserve1: 100,
        ending_policy: EndingPolicy::Manual,
        sell_fee_bps: 0,
        close_timestamp: None,
        close_on_depletion: None,
    })
    .expect("valid pool");
    assert_eq!(pool.withdraw_reserves(1), Err(WithdrawError::StillOpen));
    assert_eq!(pool.close_pool(1), Ok(()));
    assert_eq!(pool.withdraw_reserves(1), Ok((800, 25)));
    assert_eq!(pool.lifecycle, PoolLifecycle::Withdrawn);
}

#[test]
fn direct_curve_policy_matrix_preserves_all_existing_endings() {
    for ending in [
        EndingPolicy::Manual,
        EndingPolicy::Scheduled,
        EndingPolicy::Depletion,
        EndingPolicy::Either,
    ] {
        for end in [None, Some(42)] {
            for side in [None, Some(TokenSide::Token0), Some(TokenSide::Token1)] {
                let params = PoolCreateParams {
                    ending_policy: ending,
                    close_timestamp: end,
                    close_on_depletion: side,
                    ..policy_params()
                };
                let valid = match ending {
                    EndingPolicy::Manual => end.is_none() && side.is_none(),
                    EndingPolicy::Scheduled => end.is_some() && side.is_none(),
                    EndingPolicy::Depletion => end.is_none() && side.is_some(),
                    EndingPolicy::Either => end.is_some() && side.is_some(),
                };
                let result = Pool::create(params);
                if valid {
                    let mut pool = result.unwrap();
                    assert_eq!(pool.validate_policy(), Ok(()));
                    assert_eq!(pool.close_pool(1), Ok(()));
                    assert_eq!(pool.withdraw_reserves(1), Ok((80, 100)));
                } else {
                    assert_eq!(result, Err(CreateError::InvalidEndingPolicy));
                }
            }
        }
    }
}

#[test]
fn malformed_saved_policy_rejects_all_mutations_before_other_errors() {
    let mut pool = Pool::create(policy_params()).unwrap();
    pool.close_on_depletion = Some(TokenSide::Token0);
    assert_invalid_saved_policy(pool);
}

#[test]
fn immutable_rate_and_initial_invariant_are_validated() {
    for rate in [10_000, u16::MAX] {
        assert_eq!(
            Pool::create(PoolCreateParams {
                sell_fee_bps: rate,
                ..policy_params()
            }),
            Err(CreateError::InvalidSellFee)
        );
    }
    for case in 0..5 {
        let mut pool = Pool::create(policy_params()).unwrap();
        match case {
            0 => pool.sell_fee_bps = 10_000,
            1 => pool.initial_k = 0,
            2 => pool.initial_k = pool.k + 1,
            3 => pool.k += 1,
            _ => pool.virtual_reserve1 -= 1,
        }
        assert_invalid_saved_policy(pool);
    }
}

fn assert_invalid_saved_policy(mut pool: Pool) {
    let before = pool.clone();
    assert_eq!(
        pool.swap_exact_input(ExactInputSwap {
            token_in: TokenSide::Token0,
            amount_in: 0,
            min_amount_out: 0,
            pool_fee_bps: 1,
            protocol_fee_bps: 10_001,
            now: 1,
        }),
        Err(SwapError::InvalidPolicy)
    );
    assert_eq!(
        pool.swap_exact_output(ExactOutputSwap {
            token_in: TokenSide::Token1,
            amount_out: 0,
            max_amount_in: 0,
            pool_fee_bps: 1,
            protocol_fee_bps: 10_001,
            now: 1,
        }),
        Err(SwapError::InvalidPolicy)
    );
    assert_eq!(pool.collect_fees(), Err(SwapError::InvalidPolicy));
    assert_eq!(pool.close_pool(1), Err(CloseError::InvalidPolicy));
    assert_eq!(
        pool.withdraw_reserves(42),
        Err(WithdrawError::InvalidPolicy)
    );
    assert_eq!(pool, before);
}

#[test]
fn selected_reserve_and_expiry_are_independent_ending_facts() {
    for side in [TokenSide::Token0, TokenSide::Token1] {
        let mut pool = Pool::create(PoolCreateParams {
            ending_policy: EndingPolicy::Either,
            sell_fee_bps: 0,
            close_on_depletion: Some(side),
            close_timestamp: Some(42),
            ..policy_params()
        })
        .unwrap();
        assert_eq!(pool.effective_lifecycle(41), PoolLifecycle::Open);
        assert_eq!(pool.effective_lifecycle(42), PoolLifecycle::Closed);
        assert_eq!(
            pool.ending_decision(),
            Ok(EndingDecision::ScheduledEnd { timestamp: 42 })
        );
        match side {
            TokenSide::Token0 => pool.real_reserve0 = 0,
            TokenSide::Token1 => pool.real_reserve1 = 0,
        }
        assert_eq!(pool.ending_decision(), Ok(EndingDecision::Depleted));
        assert_eq!(pool.trade_bounds(), Err(SwapError::Closed));
        assert_eq!(pool.effective_lifecycle(1), PoolLifecycle::Closed);
    }
}

#[test]
fn cleared_reserves_never_supply_depletion_evidence_for_stored_closure() {
    let mut pool = Pool::create(PoolCreateParams {
        ending_policy: EndingPolicy::Either,
        sell_fee_bps: 0,
        close_on_depletion: Some(TokenSide::Token0),
        close_timestamp: Some(42),
        ..policy_params()
    })
    .unwrap();
    pool.close_pool(1).unwrap();
    assert_eq!(pool.ending_decision(), Ok(EndingDecision::Manual));
    pool.withdraw_reserves(1).unwrap();
    assert_eq!(pool.ending_decision(), Ok(EndingDecision::Manual));
    assert_eq!(pool.effective_lifecycle(1), PoolLifecycle::Withdrawn);
}

#[test]
fn scheduled_sellout_stays_open_and_rejects_early_withdrawal() {
    let mut pool = Pool::create(PoolCreateParams {
        ending_policy: EndingPolicy::Scheduled,
        sell_fee_bps: 0,
        close_timestamp: Some(42),
        ..policy_params()
    })
    .unwrap();
    pool.swap_exact_output(ExactOutputSwap {
        token_in: TokenSide::Token1,
        amount_out: 80,
        max_amount_in: 400,
        pool_fee_bps: 0,
        protocol_fee_bps: 0,
        now: 1,
    })
    .unwrap();
    assert_eq!(pool.real_reserve0, 0);
    assert_eq!(pool.lifecycle, PoolLifecycle::Open);
    assert_eq!(pool.effective_lifecycle(41), PoolLifecycle::Open);
    assert_eq!(pool.withdraw_reserves(41), Err(WithdrawError::StillOpen));
    assert_eq!(
        pool.ending_decision(),
        Ok(EndingDecision::ScheduledEnd { timestamp: 42 })
    );
    assert_eq!(
        pool.swap_exact_output(ExactOutputSwap {
            token_in: TokenSide::Token1,
            amount_out: 1,
            max_amount_in: 100,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 1,
        }),
        Err(SwapError::InsufficientRealOutputReserve)
    );
    pool.swap_exact_input(ExactInputSwap {
        token_in: TokenSide::Token0,
        amount_in: 1,
        min_amount_out: 1,
        pool_fee_bps: 0,
        protocol_fee_bps: 0,
        now: 41,
    })
    .unwrap();
    assert_eq!(pool.real_reserve0, 1);
    assert_eq!(pool.lifecycle, PoolLifecycle::Open);
    assert!(pool.withdraw_reserves(42).is_ok());
}

#[test]
fn selected_depletion_cannot_reopen_when_inventory_returns() {
    for (ending, end) in [
        (EndingPolicy::Depletion, None),
        (EndingPolicy::Either, Some(42)),
    ] {
        let mut pool = Pool::create(PoolCreateParams {
            ending_policy: ending,
            close_timestamp: end,
            close_on_depletion: Some(TokenSide::Token0),
            ..policy_params()
        })
        .unwrap();
        pool.swap_exact_output(ExactOutputSwap {
            token_in: TokenSide::Token1,
            amount_out: 80,
            max_amount_in: 400,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 1,
        })
        .unwrap();
        assert_eq!(pool.lifecycle, PoolLifecycle::Closed);
        pool.real_reserve0 = 1;
        assert_eq!(pool.effective_lifecycle(1), PoolLifecycle::Closed);
        assert_eq!(
            pool.swap_exact_input(ExactInputSwap {
                token_in: TokenSide::Token0,
                amount_in: 1,
                min_amount_out: 0,
                pool_fee_bps: 0,
                protocol_fee_bps: 0,
                now: 1,
            }),
            Err(SwapError::Closed)
        );
    }
}
