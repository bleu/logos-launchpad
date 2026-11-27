//! LBP pool preparation, inventory and seed funding, and activation.
use super::*;

pub(super) fn prepare(
    factory: Actor,
    mut state: LbpLaunchState,
    mint: AccountId,
    values: &[Vec<u8>],
) -> Result<Response, Error> {
    validate_allocated_custody(factory, &state, mint, values)?;
    state.pending = None;
    state.creation_stage = LbpCreationStage::PoolPrepared;
    let prepare = match state.mode {
        LbpSetupMode::FixedPrice => lbp_core::Message::PrepareFixedPrice {
            inputs: lbp_core::FixedPricePoolInputs {
                per_block_token_limit: state.per_block_token_limit,
                buy_policy: state.buy_policy.clone(),
                pause_policy: state.pause_policy,
                ending_policy: state.ending_policy,
                trade_mode: state.trade_mode,
                sell_fee_bps: state.sell_fee_bps,
                graduation_adapter: state.adapter(factory),
                graduation_policy: state.graduation_policy,
                namespace: state.namespace,
                namespace_program_id: state.namespace_program_id,
                factory_program_id: factory.program_account_id,
                factory_seed: *compute_factory_seed(state.namespace, state.launch_salt).as_bytes(),
                token_definition_id: state.token_definition_id,
                collateral_definition_id: state.collateral_definition_id,
                sale_inventory: state.sale_inventory,
                creator_allocation: state.creator_allocation,
                price: state.price,
                start_timestamp: state.start_timestamp,
                end_timestamp: state.end_timestamp,
                creator_allocation_release: state.allocation_release,
            },
        },
        LbpSetupMode::Weighted {
            collateral_seed,
            token_weight_start,
            token_weight_end,
        } => lbp_core::Message::PrepareWeighted {
            inputs: lbp_core::WeightedPoolInputs {
                per_block_token_limit: state.per_block_token_limit,
                buy_policy: state.buy_policy.clone(),
                pause_policy: state.pause_policy,
                ending_policy: state.ending_policy,
                trade_mode: state.trade_mode,
                sell_fee_bps: state.sell_fee_bps,
                graduation_adapter: state.adapter(factory),
                graduation_policy: state.graduation_policy,
                namespace: state.namespace,
                namespace_program_id: state.namespace_program_id,
                factory_program_id: factory.program_account_id,
                factory_seed: *compute_factory_seed(state.namespace, state.launch_salt).as_bytes(),
                token_definition_id: state.token_definition_id,
                collateral_definition_id: state.collateral_definition_id,
                sale_inventory: state.sale_inventory,
                creator_allocation: state.creator_allocation,
                collateral_seed,
                token_weight_start,
                token_weight_end,
                start_timestamp: state.start_timestamp,
                end_timestamp: state.end_timestamp.ok_or(Error::InvalidState)?,
                creator_allocation_release: state.allocation_release,
            },
        },
    };
    state.with_setup_deadline(
        factory,
        state
            .response()
            .send_sync(Actor::new(state.pool_id, state.lbp_program_id), &prepare),
    )
}

pub(super) fn complete(
    factory: Actor,
    mut state: LbpLaunchState,
    mint: AccountId,
    creator: AccountId,
    values: &[Vec<u8>],
) -> Result<Response, Error> {
    validate_allocated_custody(factory, &state, mint, values)?;
    let pool = lbp_core::PoolAccount::decode(&values[4]).map_err(|_| Error::InvalidState)?;
    if pool.graduation_policy != state.graduation_policy
        || pool.graduation_adapter != state.adapter(factory)
        || pool.ending_policy != state.ending_policy
        || pool.trade_mode != state.trade_mode
        || pool.block_allocation.limit != state.per_block_token_limit
        || pool.sell_fee_bps != state.sell_fee_bps
    {
        return Err(Error::InvalidCustody);
    }
    if pool.buy_policy != state.buy_policy
        || pool.trade_control.policy() != state.pause_policy
        || pool.funded
        || pool.settlement_progress != SettlementProgress::Unselected
        || pool.mode != state.mode.pool_mode()
        || pool.remaining_inventory != state.sale_inventory
        || pool.accrued_fees != 0
        || pool.recorded_collateral != 0
        || pool.namespace != state.namespace
        || pool.namespace_program_id != state.namespace_program_id
        || pool.factory_owner != factory.account_id
        || pool.factory_program_id != factory.program_account_id
        || pool.factory_seed != *compute_factory_seed(state.namespace, state.launch_salt).as_bytes()
        || pool.token_definition_id != state.token_definition_id
        || pool.collateral_definition_id != state.collateral_definition_id
        || pool.sale_inventory != state.sale_inventory
        || pool.creator_allocation != state.creator_allocation
        || pool.price != state.price
        || pool.start_timestamp != state.start_timestamp
        || pool.end_timestamp != state.end_timestamp
        || pool.creator_allocation_release != state.allocation_release
    {
        return Err(Error::InvalidState);
    }
    if balance(&values[5], state.pool_id, state.token_definition_id)? != 0 {
        return Err(Error::InvalidCustody);
    }
    let pool_collateral = balance(&values[6], state.pool_id, state.collateral_definition_id)?;
    if let LbpSetupMode::Weighted {
        collateral_seed, ..
    } = state.mode
    {
        if balance(&values[7], creator, state.collateral_definition_id)? < collateral_seed {
            return Err(Error::InvalidCustody);
        }
        pool_collateral
            .checked_add(collateral_seed)
            .ok_or(Error::Overflow)?;
    }
    state.pending = None;
    state.creation_stage = LbpCreationStage::Complete;
    state.creator_allocation_released =
        state.allocation_release == CreatorAllocationRelease::AfterSetup;
    let response = transfer(
        state.response(),
        factory.account_id,
        compute_factory_seed(state.namespace, state.launch_salt),
        state.token_definition_id,
        state.pool_id,
        state.sale_inventory,
    );
    let response = if let LbpSetupMode::Weighted {
        collateral_seed, ..
    } = state.mode
    {
        response.send_sync(
            Actor::new(creator, TOKEN_PROGRAM_ACCOUNT_ID),
            &token::Message::Transfer {
                definition: state.collateral_definition_id,
                recipient: state.pool_id,
                amount: collateral_seed,
            },
        )
    } else {
        response
    };
    let activation = match state.mode {
        LbpSetupMode::FixedPrice => lbp_core::Message::ActivateFixedPrice,
        LbpSetupMode::Weighted { .. } => lbp_core::Message::ActivateWeighted,
    };
    let response = response.send_sync(Actor::new(state.pool_id, state.lbp_program_id), &activation);
    let response = if state.creator_allocation_released {
        state.allocation_event(
            transfer(
                response,
                state.creator_escrow_id,
                compute_escrow_seed(state.namespace, state.launch_salt),
                state.token_definition_id,
                creator,
                state.creator_allocation,
            ),
            factory.account_id,
            creator,
        )
    } else {
        response
    };
    state.with_setup_deadline(factory, response)
}

fn validate_allocated_custody(
    factory: Actor,
    state: &LbpLaunchState,
    mint: AccountId,
    values: &[Vec<u8>],
) -> Result<(), Error> {
    if balance(&values[1], mint, state.token_definition_id)? != state.graduation_reserve
        || balance(&values[2], factory.account_id, state.token_definition_id)?
            != state.sale_inventory
        || balance(
            &values[3],
            state.creator_escrow_id,
            state.token_definition_id,
        )? != state.creator_allocation
    {
        return Err(Error::InvalidCustody);
    }
    Ok(())
}
