use crate::{
    EndingPolicy, ExactInputSwap, ExactOutputSwap, Pool, PoolCreateParams, PoolLifecycle,
    SwapError, TokenSide,
};

#[test]
fn exact_input_swaps_token0_for_token1_through_the_public_pool_api() {
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
        .swap_exact_input(ExactInputSwap {
            token_in: TokenSide::Token0,
            amount_in: 250,
            min_amount_out: 20,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 1,
        })
        .expect("swap succeeds");

    assert_eq!(outcome.amount_in, 250);
    assert_eq!(outcome.amount_out, 20);
    assert_eq!(outcome.protocol_fee, 0);
    assert_eq!(pool.virtual_reserve0, 1250);
    assert_eq!(pool.virtual_reserve1, 80);
    assert_eq!(pool.real_reserve0, 1050);
    assert_eq!(pool.real_reserve1, 80);
}

#[test]
fn exact_output_swaps_token1_for_token0_with_the_fee_inside_max_input() {
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
            amount_out: 200,
            max_amount_in: 28,
            pool_fee_bps: 0,
            protocol_fee_bps: 1_000,
            now: 1,
        })
        .expect("swap succeeds");

    assert_eq!(outcome.amount_in, 28);
    assert_eq!(outcome.amount_out, 200);
    assert_eq!(outcome.protocol_fee, 3);
    assert_eq!(pool.virtual_reserve0, 800);
    assert_eq!(pool.virtual_reserve1, 125);
    assert_eq!(pool.real_reserve0, 600);
    assert_eq!(pool.real_reserve1, 125);
}

#[test]
fn later_quotes_continue_to_use_the_creation_time_k() {
    let mut pool = Pool::create(PoolCreateParams {
        token0_amount: 1_000,
        token1_amount: 1_000,
        virtual_reserve0: 1_000,
        virtual_reserve1: 1_000,
        ending_policy: EndingPolicy::Manual,
        sell_fee_bps: 0,
        close_timestamp: None,
        close_on_depletion: None,
    })
    .expect("valid pool");

    pool.swap_exact_input(ExactInputSwap {
        token_in: TokenSide::Token0,
        amount_in: 100,
        min_amount_out: 0,
        pool_fee_bps: 0,
        protocol_fee_bps: 0,
        now: 1,
    })
    .expect("first swap succeeds");
    // The first integer-rounded swap leaves the live reserve product at
    // 1,001,000. RFP-015 fixes the pricing invariant at creation-time k,
    // so the next 100-unit input pays 76 rather than the 75 a live-product
    // quote would produce.
    let outcome = pool
        .swap_exact_input(ExactInputSwap {
            token_in: TokenSide::Token0,
            amount_in: 100,
            min_amount_out: 0,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 1,
        })
        .expect("second swap succeeds");

    assert_eq!(pool.k, 1_000_000);
    assert_eq!(outcome.amount_out, 76);
}

#[test]
fn exact_output_rejects_a_zero_cost_quote_without_mutating_the_pool() {
    let mut pool = Pool::create(PoolCreateParams {
        token0_amount: 1,
        token1_amount: 74_513,
        virtual_reserve0: 1,
        virtual_reserve1: 1_709,
        ending_policy: EndingPolicy::Manual,
        sell_fee_bps: 0,
        close_timestamp: None,
        close_on_depletion: None,
    })
    .expect("valid pool");
    pool.swap_exact_input(ExactInputSwap {
        token_in: TokenSide::Token0,
        amount_in: u64::MAX.into(),
        min_amount_out: 0,
        pool_fee_bps: 0,
        protocol_fee_bps: 0,
        now: 1,
    })
    .expect("first swap succeeds");
    let before = pool.clone();

    assert_eq!(
        pool.swap_exact_output(ExactOutputSwap {
            token_in: TokenSide::Token1,
            amount_out: 1,
            max_amount_in: 0,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 1
        }),
        Err(SwapError::InputTooSmall)
    );
    assert_eq!(pool, before);
}

#[test]
fn exact_output_is_the_smallest_zero_fee_input_that_reaches_the_requested_amount() {
    for amount_out in [1, 100, 200, 799] {
        let mut quoted = Pool::create(PoolCreateParams {
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
        let outcome = quoted
            .swap_exact_output(ExactOutputSwap {
                token_in: TokenSide::Token1,
                amount_out,
                max_amount_in: u128::MAX,
                pool_fee_bps: 0,
                protocol_fee_bps: 0,
                now: 1,
            })
            .expect("exact-output quote succeeds");

        let mut at_quote = Pool::create(PoolCreateParams {
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
        let produced = at_quote
            .swap_exact_input(ExactInputSwap {
                token_in: TokenSide::Token1,
                amount_in: outcome.amount_in,
                min_amount_out: 0,
                pool_fee_bps: 0,
                protocol_fee_bps: 0,
                now: 1,
            })
            .expect("the quoted input is executable");
        assert!(produced.amount_out >= amount_out);

        let mut below_quote = Pool::create(PoolCreateParams {
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
        if outcome.amount_in > 1 {
            let previous = below_quote
                .swap_exact_input(ExactInputSwap {
                    token_in: TokenSide::Token1,
                    amount_in: outcome.amount_in - 1,
                    min_amount_out: 0,
                    pool_fee_bps: 0,
                    protocol_fee_bps: 0,
                    now: 1,
                })
                .expect("one less input remains a positive trade");
            assert!(previous.amount_out < amount_out);
        }
    }
}

#[test]
fn a_swap_cannot_exceed_the_selected_real_output_reserve() {
    let mut pool = Pool::create(PoolCreateParams {
        token0_amount: 800,
        token1_amount: 10,
        virtual_reserve0: 1000,
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
            token_in: TokenSide::Token0,
            amount_in: 250,
            min_amount_out: 0,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 1
        }),
        Err(SwapError::InsufficientRealOutputReserve)
    );
    assert_eq!(pool, before);
    assert_eq!(
        pool.lifecycle,
        PoolLifecycle::Open,
        "unconfigured depletion must not close the pool"
    );
}

#[test]
fn exact_input_rejects_a_nonzero_trade_that_rounds_to_zero_output() {
    let mut pool = Pool::create(PoolCreateParams {
        token0_amount: 800,
        token1_amount: 100,
        virtual_reserve0: 1_000,
        virtual_reserve1: 1_000,
        ending_policy: EndingPolicy::Manual,
        sell_fee_bps: 0,
        close_timestamp: None,
        close_on_depletion: None,
    })
    .expect("valid pool");
    let before = pool.clone();

    assert_eq!(
        pool.swap_exact_input(ExactInputSwap {
            token_in: TokenSide::Token0,
            amount_in: 1,
            min_amount_out: 0,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 1
        }),
        Err(SwapError::OutputTooSmall)
    );
    assert_eq!(pool, before, "a rejected dust trade must be atomic");
}
