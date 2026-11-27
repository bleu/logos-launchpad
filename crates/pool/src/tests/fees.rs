use super::policy_params;
use crate::{
    EndingPolicy, ExactInputSwap, ExactOutputSwap, FeeSide, Pool, PoolCreateParams, SwapError,
    TokenSide, VIRTUAL_RESERVE_BOUND,
};
use sale_policy::FeeRate;

#[test]
fn protocol_fee_is_collateral_input_on_buy_and_collateral_output_on_sell() {
    let mut buy_pool = Pool::create(PoolCreateParams {
        token0_amount: 800,
        token1_amount: 100,
        virtual_reserve0: 1_000,
        virtual_reserve1: 100,
        ending_policy: EndingPolicy::Manual,
        sell_fee_bps: 0,
        close_timestamp: None,
        close_on_depletion: None,
    })
    .expect("valid pool");
    let buy = buy_pool
        .swap_exact_input(ExactInputSwap {
            token_in: TokenSide::Token1,
            amount_in: 25,
            min_amount_out: 180,
            pool_fee_bps: 0,
            protocol_fee_bps: 1_000,
            now: 1,
        })
        .expect("buy succeeds");
    assert_eq!((buy.effective_amount_in, buy.protocol_fee), (22, 3));
    assert_eq!((buy.raw_amount_out, buy.amount_out), (180, 180));
    assert_eq!(buy.protocol_fee_side, FeeSide::Input);
    assert_eq!(buy_pool.real_reserve1, 122);

    let mut sell_pool = Pool::create(PoolCreateParams {
        token0_amount: 800,
        token1_amount: 100,
        virtual_reserve0: 1_000,
        virtual_reserve1: 100,
        ending_policy: EndingPolicy::Manual,
        sell_fee_bps: 0,
        close_timestamp: None,
        close_on_depletion: None,
    })
    .expect("valid pool");
    let sell = sell_pool
        .swap_exact_input(ExactInputSwap {
            token_in: TokenSide::Token0,
            amount_in: 250,
            min_amount_out: 18,
            pool_fee_bps: 0,
            protocol_fee_bps: 1_000,
            now: 1,
        })
        .expect("sell succeeds");
    assert_eq!(
        (sell.raw_amount_out, sell.protocol_fee, sell.amount_out),
        (20, 2, 18)
    );
    assert_eq!(sell.protocol_fee_side, FeeSide::Output);
    assert_eq!(
        sell_pool.real_reserve1, 80,
        "the fee leaves the collateral reserve"
    );
}

#[test]
fn exact_input_fee_errors_preserve_direction_and_leave_state_unchanged() {
    for (token_in, amount_in, fee_bps, expected) in [
        (TokenSide::Token1, 0, 10_001, SwapError::ZeroAmount),
        (TokenSide::Token1, 1, 10_001, SwapError::FeeAboveDenominator),
        (TokenSide::Token0, 1, 10_001, SwapError::FeeAboveDenominator),
        (TokenSide::Token1, 1, 1, SwapError::InputConsumedByFees),
        (
            TokenSide::Token1,
            25,
            10_000,
            SwapError::InputConsumedByFees,
        ),
        (TokenSide::Token0, 250, 9_999, SwapError::OutputTooSmall),
        (
            TokenSide::Token0,
            250,
            10_000,
            SwapError::FeeAboveDenominator,
        ),
    ] {
        let mut pool = Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 100,
            virtual_reserve0: 1_000,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Manual,
            sell_fee_bps: 0,
            close_timestamp: None,
            close_on_depletion: None,
        })
        .expect("valid pool");
        let before = pool.clone();
        assert_eq!(
            pool.swap_exact_input(ExactInputSwap {
                token_in,
                amount_in,
                min_amount_out: u128::MAX,
                pool_fee_bps: 0,
                protocol_fee_bps: fee_bps,
                now: 1,
            }),
            Err(expected)
        );
        assert_eq!(pool, before);
    }
}

#[test]
fn exact_output_fee_errors_follow_quote_checks_and_leave_state_unchanged() {
    for (amount_out, fee_bps, expected) in [
        (0, 10_001, SwapError::ZeroAmount),
        (801, 10_001, SwapError::InsufficientRealOutputReserve),
        (100, 10_001, SwapError::FeeAboveDenominator),
        (100, 10_000, SwapError::InputConsumedByFees),
        (100, 1_000, SwapError::SlippageExceeded),
    ] {
        let mut pool = Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 100,
            virtual_reserve0: 1_000,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Manual,
            sell_fee_bps: 0,
            close_timestamp: None,
            close_on_depletion: None,
        })
        .expect("valid pool");
        let before = pool.clone();
        assert_eq!(
            pool.swap_exact_output(ExactOutputSwap {
                token_in: TokenSide::Token1,
                amount_out,
                max_amount_in: 0,
                pool_fee_bps: 0,
                protocol_fee_bps: fee_bps,
                now: 1,
            }),
            Err(expected)
        );
        assert_eq!(pool, before);
    }
}

#[test]
fn exact_output_uses_the_smallest_fee_inclusive_gross_input() {
    for fee_bps in [0, 1, 2_500, 9_999] {
        let mut pool = Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 100,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Manual,
            sell_fee_bps: 0,
            close_timestamp: None,
            close_on_depletion: None,
        })
        .expect("valid pool");
        let outcome = pool
            .swap_exact_output(ExactOutputSwap {
                token_in: TokenSide::Token1,
                amount_out: 100,
                max_amount_in: u128::MAX,
                pool_fee_bps: 0,
                protocol_fee_bps: fee_bps,
                now: 1,
            })
            .expect("swap succeeds");
        let required_net = 12;
        assert_eq!(
            outcome.effective_amount_in,
            outcome.amount_in - outcome.protocol_fee
        );
        assert!(outcome.effective_amount_in >= required_net);
        let previous = outcome.amount_in.checked_sub(1).expect("positive input");
        let previous_fee = previous
            .checked_mul(u128::from(fee_bps))
            .expect("small test input")
            .div_ceil(10_000);
        let previous_effective = previous - previous_fee;
        assert!(previous_effective < required_net);
    }
}

#[test]
fn policy_buy_quotes_match_fee_math_and_curve_rounding() {
    for bps in [0, 1, 1000, 9999] {
        let base = Pool::create(PoolCreateParams {
            token0_amount: 800,
            virtual_reserve0: 1000,
            ..policy_params()
        })
        .unwrap();
        let rate = FeeRate::new(bps).unwrap();
        let split = rate.split(100_000).unwrap();
        let expected =
            curve_math::exact_input_amount_out_floor(100, 1000, base.k, split.net).unwrap();
        let mut pool = base.clone();
        let quoted = pool.swap_exact_input(ExactInputSwap {
            token_in: TokenSide::Token1,
            amount_in: 100_000,
            min_amount_out: 0,
            pool_fee_bps: 0,
            protocol_fee_bps: bps,
            now: 1,
        });
        if expected > base.real_reserve0 {
            assert_eq!(quoted, Err(SwapError::InsufficientRealOutputReserve));
        } else {
            let quoted = quoted.unwrap();
            assert_eq!(
                (
                    quoted.amount_out,
                    quoted.effective_amount_in,
                    quoted.protocol_fee
                ),
                (expected, split.net, split.fee)
            );
        }
        let required = curve_math::exact_output_amount_in_ceil(100, 1000, base.k, 100).unwrap();
        let split = rate.gross_up(required).unwrap();
        let mut pool = base;
        let quoted = pool
            .swap_exact_output(ExactOutputSwap {
                token_in: TokenSide::Token1,
                amount_out: 100,
                max_amount_in: u128::MAX,
                pool_fee_bps: 0,
                protocol_fee_bps: bps,
                now: 1,
            })
            .unwrap();
        assert_eq!(
            (
                quoted.amount_in,
                quoted.effective_amount_in,
                quoted.protocol_fee
            ),
            (split.gross, split.net, split.fee)
        );
    }
}

#[test]
fn retained_sell_fee_preserves_rounding_slack_and_changes_later_quotes() {
    let mut pool = Pool::create(PoolCreateParams {
        virtual_reserve0: 1000,
        virtual_reserve1: 1000,
        token0_amount: 1000,
        token1_amount: 1000,
        sell_fee_bps: 100,
        ..policy_params()
    })
    .unwrap();
    let before = pool.clone();
    let sell = pool
        .swap_exact_input(ExactInputSwap {
            token_in: TokenSide::Token0,
            amount_in: 100,
            min_amount_out: 88,
            pool_fee_bps: 0,
            protocol_fee_bps: 100,
            now: 0,
        })
        .unwrap();
    assert_eq!(
        (
            sell.raw_amount_out,
            sell.amount_out,
            sell.protocol_fee,
            sell.retained_fee
        ),
        (90, 88, 1, 1)
    );
    assert_eq!(
        (pool.real_reserve0, pool.real_reserve1, pool.fees_accrued),
        (1100, 911, 1)
    );
    assert_eq!((pool.initial_k, pool.k), (1_000_000, 1_001_100));
    assert_eq!(pool.virtual_reserve0 * pool.virtual_reserve1 - pool.k, 1000);
    assert_eq!(
        before.real_reserve1,
        pool.real_reserve1 + pool.fees_accrued + sell.amount_out
    );
    let expected = curve_math::exact_input_amount_out_floor(1100, 911, pool.k, 100).unwrap();
    let next = pool
        .swap_exact_input(ExactInputSwap {
            token_in: TokenSide::Token0,
            amount_in: 100,
            min_amount_out: 0,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 0,
        })
        .unwrap();
    assert_eq!(next.raw_amount_out, expected);
    assert_eq!(pool.k, 1_001_100 + 1200 * next.retained_fee);
    assert_eq!(pool.initial_k, 1_000_000);
    assert_eq!(pool.validate_policy(), Ok(()));
    assert_eq!(pool.collect_fees(), Ok(1));
    let proceeds = pool.real_reserve1;
    pool.close_pool(0).unwrap();
    assert_eq!(pool.withdraw_reserves(0).unwrap().1, proceeds);
}

#[test]
fn combined_rate_and_dust_checks_apply_only_to_sells() {
    let pool = Pool::create(PoolCreateParams {
        virtual_reserve0: 1000,
        virtual_reserve1: 1000,
        token0_amount: 1000,
        token1_amount: 1000,
        sell_fee_bps: 5000,
        ..policy_params()
    })
    .unwrap();
    for (amount_in, namespace, expected) in [
        (100, 5000, SwapError::FeeAboveDenominator),
        (3, 1, SwapError::OutputTooSmall),
    ] {
        let mut rejected = pool.clone();
        assert_eq!(
            rejected.swap_exact_input(ExactInputSwap {
                token_in: TokenSide::Token0,
                amount_in,
                min_amount_out: 0,
                pool_fee_bps: 0,
                protocol_fee_bps: namespace,
                now: 0,
            }),
            Err(expected)
        );
        assert_eq!(rejected, pool);
    }
    let mut buy = pool.clone();
    assert_eq!(
        buy.swap_exact_input(ExactInputSwap {
            token_in: TokenSide::Token1,
            amount_in: 100,
            min_amount_out: 0,
            pool_fee_bps: 0,
            protocol_fee_bps: 5000,
            now: 0,
        })
        .unwrap()
        .retained_fee,
        0
    );
    assert_eq!(buy.k, buy.initial_k);
    let mut buy = pool;
    assert_eq!(
        buy.swap_exact_output(ExactOutputSwap {
            token_in: TokenSide::Token1,
            amount_out: 10,
            max_amount_in: 100,
            pool_fee_bps: 0,
            protocol_fee_bps: 5000,
            now: 0,
        })
        .unwrap()
        .retained_fee,
        0
    );
    assert_eq!(buy.k, buy.initial_k);
}

#[test]
fn retained_fee_invariant_overflow_is_atomic() {
    let reserve = VIRTUAL_RESERVE_BOUND - 1;
    let mut pool = Pool::create(PoolCreateParams {
        virtual_reserve0: reserve,
        virtual_reserve1: reserve,
        token0_amount: 1000,
        token1_amount: reserve,
        sell_fee_bps: 1000,
        ..policy_params()
    })
    .unwrap();
    let before = pool.clone();
    assert_eq!(
        pool.swap_exact_input(ExactInputSwap {
            token_in: TokenSide::Token0,
            amount_in: 100,
            min_amount_out: 0,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 0,
        }),
        Err(SwapError::Arithmetic)
    );
    assert_eq!(pool, before);
}
