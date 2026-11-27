mod creation;
mod encoding;
mod fees;
mod lifecycle;
mod swaps;

use crate::{EndingPolicy, PoolCreateParams};

fn policy_params() -> PoolCreateParams {
    PoolCreateParams {
        token0_amount: 80,
        token1_amount: 100,
        virtual_reserve0: 100,
        virtual_reserve1: 100,
        ending_policy: EndingPolicy::Manual,
        sell_fee_bps: 0,
        close_timestamp: None,
        close_on_depletion: None,
    }
}
