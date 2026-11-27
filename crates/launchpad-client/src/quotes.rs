use curve_core::Config;

use crate::lbp::validate_lbp_pool_binding;
use crate::{ClientError, LbpQuoteRequest, LbpSale, LbpTradeQuote, Result};

/// Quotes use the live namespace rate and freeze state. Weighted pricing time is not inclusion proof.
/// Capped buys need `observed_block` from [`load_clock_block_id`](crate::load_clock_block_id). The allowance check holds only
/// for inclusion in the next block and does not reserve allocation.
pub fn quote_lbp_trade(
    sale: LbpSale,
    pool_account: &lbp_core::PoolAccount,
    config: &Config,
    request: LbpQuoteRequest,
    pricing_time: u64,
    observed_block: Option<u64>,
) -> Result<LbpTradeQuote> {
    if config.namespace != sale.namespace {
        return Err(ClientError::PoolSelectionMismatch);
    }
    validate_lbp_pool_binding(sale, pool_account)?;
    if config.emergency.is_frozen {
        return Err(ClientError::NamespaceFrozen);
    }
    let quote = lbp_core::quote_trade(pool_account, request, config.protocol_fee_bps, pricing_time)
        .map_err(ClientError::QuoteLbpTrade)?;
    if !request.is_sell() && pool_account.block_allocation.limit.is_some() {
        let block = observed_block.ok_or(ClientError::BlockIdRequired)?;
        if pool_account
            .block_allocation
            .remaining(block)
            .is_some_and(|remaining| quote.amount_out > remaining)
        {
            return Err(ClientError::QuoteLbpTrade(
                lbp_core::Error::BlockAllocationExceeded,
            ));
        }
    }
    Ok(quote)
}

/// Quotes an exact-output purchase against a snapshot of the current pool and fee config.
/// The participant supplies the cap in the purchase message.
pub fn quote_buy(
    pool_account: &curve_core::PoolAccount,
    config: &Config,
    amount_out: u128,
    now: u64,
    observed_block: Option<u64>,
) -> Result<pool::SwapOutcome> {
    if config.emergency.is_frozen {
        return Err(ClientError::NamespaceFrozen);
    }
    if !pool_account.funded {
        return Err(ClientError::PoolFundingPending);
    }
    if pool_account.trade_control.is_paused() {
        return Err(ClientError::PoolPaused);
    }
    if pool_account
        .allowed_input_token
        .is_some_and(|side| side != pool::TokenSide::Token1)
    {
        return Err(ClientError::InputTokenRestricted);
    }
    pool_account
        .pool
        .validate_policy()
        .map_err(|_| ClientError::QuoteBuy(pool::SwapError::InvalidPolicy))?;
    let mut pool = pool_account.pool.clone();
    let quote = pool
        .swap_exact_output(pool::ExactOutputSwap {
            token_in: pool::TokenSide::Token1,
            amount_out,
            max_amount_in: u128::MAX,
            pool_fee_bps: 0,
            protocol_fee_bps: config.protocol_fee_bps,
            now,
        })
        .map_err(ClientError::QuoteBuy)?;
    check_curve_allocation(pool_account, observed_block, quote.amount_out)?;
    Ok(quote)
}

/// Quotes an exact-input sale against a snapshot of the current pool and fee config.
/// The participant supplies the floor in the sale message.
pub fn quote_sell(
    pool_account: &curve_core::PoolAccount,
    config: &Config,
    amount_in: u128,
    now: u64,
) -> Result<pool::SwapOutcome> {
    if config.emergency.is_frozen {
        return Err(ClientError::NamespaceFrozen);
    }
    if !pool_account.funded {
        return Err(ClientError::PoolFundingPending);
    }
    if pool_account.trade_control.is_paused() {
        return Err(ClientError::PoolPaused);
    }
    if pool_account
        .allowed_input_token
        .is_some_and(|side| side != pool::TokenSide::Token0)
    {
        return Err(ClientError::InputTokenRestricted);
    }
    pool_account
        .pool
        .validate_policy()
        .map_err(|_| ClientError::QuoteSell(pool::SwapError::InvalidPolicy))?;
    let mut pool = pool_account.pool.clone();
    pool.swap_exact_input(pool::ExactInputSwap {
        token_in: pool::TokenSide::Token0,
        amount_in,
        min_amount_out: 0,
        pool_fee_bps: 0,
        protocol_fee_bps: config.protocol_fee_bps,
        now,
    })
    .map_err(ClientError::QuoteSell)
}

/// Quotes an exact-input purchase spending collateral against a pool snapshot.
pub fn quote_buy_with_collateral(
    pool_account: &curve_core::PoolAccount,
    config: &Config,
    collateral_in: u128,
    now: u64,
    observed_block: Option<u64>,
) -> Result<pool::SwapOutcome> {
    if config.emergency.is_frozen {
        return Err(ClientError::NamespaceFrozen);
    }
    if !pool_account.funded {
        return Err(ClientError::PoolFundingPending);
    }
    if pool_account.trade_control.is_paused() {
        return Err(ClientError::PoolPaused);
    }
    if pool_account
        .allowed_input_token
        .is_some_and(|side| side != pool::TokenSide::Token1)
    {
        return Err(ClientError::InputTokenRestricted);
    }
    pool_account
        .pool
        .validate_policy()
        .map_err(|_| ClientError::QuoteBuyWithCollateral(pool::SwapError::InvalidPolicy))?;
    let mut pool = pool_account.pool.clone();
    let quote = pool
        .swap_exact_input(pool::ExactInputSwap {
            token_in: pool::TokenSide::Token1,
            amount_in: collateral_in,
            min_amount_out: 0,
            pool_fee_bps: 0,
            protocol_fee_bps: config.protocol_fee_bps,
            now,
        })
        .map_err(ClientError::QuoteBuyWithCollateral)?;
    check_curve_allocation(pool_account, observed_block, quote.amount_out)?;
    Ok(quote)
}

fn check_curve_allocation(
    state: &curve_core::PoolAccount,
    observed_block: Option<u64>,
    amount_out: u128,
) -> Result<()> {
    if state.block_allocation.limit.is_some() {
        let block = observed_block.ok_or(ClientError::BlockIdRequired)?;
        if state
            .block_allocation
            .remaining(block)
            .is_some_and(|remaining| amount_out > remaining)
        {
            return Err(ClientError::CurveBlockAllocationExceeded);
        }
    }
    Ok(())
}
