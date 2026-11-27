use factory_core::{LbpLaunchState, LbpSetupMode, compute_factory_seed, compute_graduation_pda};
use lbp_core::{BlockAllocation, PoolAccount, PoolMode, TradeControl};
use lee_core::account::Actor;

pub fn funded_pool(factory: Actor, launch: &LbpLaunchState) -> PoolAccount {
    let mode = match launch.mode {
        LbpSetupMode::FixedPrice => PoolMode::FixedPrice,
        LbpSetupMode::Weighted {
            collateral_seed,
            token_weight_start,
            token_weight_end,
        } => PoolMode::Weighted {
            collateral_seed,
            token_weight_start,
            token_weight_end,
            pricing_collateral: collateral_seed,
            checkpoint_time: 0,
            checkpoint_token_weight: 0,
        },
    };
    let adapter = launch.graduation_hook.as_ref().map(|hook| {
        Actor::new(
            compute_graduation_pda(hook.program_id, factory.account_id),
            hook.program_id,
        )
    });
    let mut bytes = borsh::to_vec(&(
        BlockAllocation::new(launch.per_block_token_limit).unwrap(),
        &launch.buy_policy,
        TradeControl::new(launch.pause_policy),
        launch.ending_policy,
        launch.trade_mode,
        launch.sell_fee_bps,
        adapter,
        launch.graduation_policy,
        mode,
        (true, false),
        (
            launch.namespace,
            launch.namespace_program_id,
            factory.account_id,
            factory.program_account_id,
            *compute_factory_seed(launch.namespace, launch.launch_salt).as_bytes(),
            launch.token_definition_id,
            launch.collateral_definition_id,
        ),
        (
            launch.sale_inventory,
            launch.sale_inventory,
            launch.creator_allocation,
        ),
        launch.price,
        (
            launch.start_timestamp,
            launch.end_timestamp,
            launch.allocation_release,
        ),
        (
            match mode {
                PoolMode::FixedPrice => 0u128,
                PoolMode::Weighted {
                    collateral_seed, ..
                } => collateral_seed,
            },
            0u128,
            launch.settlement_progress,
        ),
    ))
    .unwrap();
    bytes.push(0);
    PoolAccount::decode(&bytes).unwrap()
}
