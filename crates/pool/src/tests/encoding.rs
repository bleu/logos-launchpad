use crate::{EndingPolicy, Pool, PoolCreateParams};
use borsh::BorshDeserialize;

#[test]
fn a_pool_survives_a_borsh_round_trip() {
    let pool = Pool::create(PoolCreateParams {
        token0_amount: 800,
        token1_amount: 200,
        virtual_reserve0: 1000,
        virtual_reserve1: 100,
        ending_policy: EndingPolicy::Scheduled,
        sell_fee_bps: 0,
        close_timestamp: Some(42),
        close_on_depletion: None,
    })
    .expect("valid pool");
    let bytes = borsh::to_vec(&pool).expect("pool serialises");
    assert_eq!(Pool::try_from_slice(&bytes).expect("bytes parse"), pool);
}

#[test]
fn pool_borsh_layout_places_ending_tag_before_the_deadline() {
    let pool = Pool::create(PoolCreateParams {
        token0_amount: 800,
        token1_amount: 200,
        virtual_reserve0: 1000,
        virtual_reserve1: 100,
        ending_policy: EndingPolicy::Scheduled,
        sell_fee_bps: 0,
        close_timestamp: Some(42),
        close_on_depletion: None,
    })
    .expect("valid pool");
    let mut expected = Vec::new();
    for value in [1000_u128, 100, 100_000, 100_000, 800, 200, 0, 0] {
        expected.extend_from_slice(&value.to_le_bytes());
    }
    expected.push(1); // Scheduled.
    expected.extend_from_slice(&0_u16.to_le_bytes());
    expected.push(1);
    expected.extend_from_slice(&42_u64.to_le_bytes());
    expected.extend_from_slice(&[0, 0]);

    assert_eq!(borsh::to_vec(&pool).expect("pool serialises"), expected);
}
