use crate::{CreateError, EndingPolicy, Pool, PoolCreateParams, PoolLifecycle, TokenSide};

#[test]
fn create_pool_records_both_real_reserves_and_optional_close_time() {
    let pool = Pool::create(PoolCreateParams {
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
    assert_eq!(pool.real_reserve0, 800);
    assert_eq!(pool.real_reserve1, 25);
    assert_eq!(pool.virtual_reserve0, 1000);
    assert_eq!(pool.virtual_reserve1, 100);
    assert_eq!(pool.close_timestamp, Some(42));
    assert_eq!(pool.k, 100_000);
    assert_eq!(pool.lifecycle, PoolLifecycle::Open);
}

#[test]
fn configured_zero_depletion_reserve_is_rejected() {
    assert_eq!(
        Pool::create(PoolCreateParams {
            token0_amount: 0,
            token1_amount: 1,
            virtual_reserve0: 100,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Depletion,
            sell_fee_bps: 0,
            close_timestamp: None,
            close_on_depletion: Some(TokenSide::Token0)
        }),
        Err(CreateError::DepletionReserveZero)
    );
}

#[test]
fn create_bounds_both_virtual_reserves_below_two_to_the_64() {
    assert_eq!(
        Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 0,
            virtual_reserve0: 1 << 64,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Manual,
            sell_fee_bps: 0,
            close_timestamp: None,
            close_on_depletion: None
        }),
        Err(CreateError::VirtualReserveAboveBound)
    );
    assert_eq!(
        Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 0,
            virtual_reserve0: 1000,
            virtual_reserve1: 1 << 64,
            ending_policy: EndingPolicy::Manual,
            sell_fee_bps: 0,
            close_timestamp: None,
            close_on_depletion: None
        }),
        Err(CreateError::VirtualReserveAboveBound)
    );
    assert!(
        Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 0,
            virtual_reserve0: (1 << 64) - 1,
            virtual_reserve1: (1 << 64) - 1,
            ending_policy: EndingPolicy::Manual,
            sell_fee_bps: 0,
            close_timestamp: None,
            close_on_depletion: None
        })
        .is_ok()
    );
}

#[test]
fn create_requires_both_virtual_reserves_to_price_the_pool() {
    assert_eq!(
        Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 200,
            virtual_reserve0: 1000,
            virtual_reserve1: 0,
            ending_policy: EndingPolicy::Manual,
            sell_fee_bps: 0,
            close_timestamp: None,
            close_on_depletion: None
        }),
        Err(CreateError::VirtualReserveZero)
    );
    assert_eq!(
        Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 200,
            virtual_reserve0: 0,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Manual,
            sell_fee_bps: 0,
            close_timestamp: None,
            close_on_depletion: None
        }),
        Err(CreateError::VirtualReserveZero)
    );
}
