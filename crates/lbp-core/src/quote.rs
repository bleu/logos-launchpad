use crate::{Error, PoolAccount, PoolMode, TradeMode};
use sale_policy::{FeeError, FeeRate, SellFeeRate};
use weighted_math_core::{QuoteError, try_calc_in_given_out, try_calc_out_given_in};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteRequest {
    BuyExactInput {
        collateral_in: u128,
        min_tokens_out: u128,
    },
    BuyExactOutput {
        tokens_out: u128,
        max_collateral_in: u128,
    },
    SellExactInput {
        tokens_in: u128,
        min_collateral_out: u128,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeQuote {
    pub amount_in: u128,
    pub amount_out: u128,
    pub gross_collateral: u128,
    pub net_collateral: u128,
    pub protocol_fee: u128,
    pub retained_fee: u128,
}

impl QuoteRequest {
    pub fn is_sell(self) -> bool {
        matches!(self, Self::SellExactInput { .. })
    }
}

/// Quote a snapshot. The actor still checks authority, live freeze, custody and inclusion bounds.
pub fn quote_trade(
    state: &PoolAccount,
    request: QuoteRequest,
    pricing_time: u64,
) -> Result<TradeQuote, Error> {
    state.validate_policies()?;
    if !state.funded || state.pending.is_some() {
        return Err(Error::PendingFunding);
    }
    if state.trade_control.is_paused() {
        return Err(Error::Paused);
    }
    if state.settlement_progress != crate::SettlementProgress::Unselected {
        return Err(Error::Closed);
    }
    if !state.trade_bounds()?.contains(pricing_time) {
        return Err(Error::Closed);
    }
    let sell = request.is_sell();
    if sell && state.trade_mode != TradeMode::BuyAndSell {
        return Err(Error::TradeDirectionUnsupported);
    }
    if !sell && state.remaining_inventory == 0 {
        return Err(Error::Closed);
    }
    let namespace_bps = state.protocol_fee_bps;
    let rate = FeeRate::new(namespace_bps).map_err(fee_error)?;
    let weights = if let PoolMode::Weighted {
        collateral_seed,
        pricing_collateral,
        ..
    } = state.mode
    {
        if pricing_collateral != state.recorded_collateral || pricing_collateral < collateral_seed {
            return Err(Error::InvalidState);
        }
        Some(crate::weighted_lifecycle::weights_at(state, pricing_time)?)
    } else {
        if state.price.collateral_units == 0 || state.price.token_units == 0 {
            return Err(Error::InvalidTerms);
        }
        None
    };
    let quote = match request {
        QuoteRequest::BuyExactInput {
            collateral_in,
            min_tokens_out,
        } => {
            let collateral = rate.split(collateral_in).map_err(fee_error)?;
            let out = if let Some((coll_w, token_w)) = weights {
                try_calc_out_given_in(
                    state.recorded_collateral,
                    coll_w,
                    state.remaining_inventory,
                    token_w,
                    collateral.net,
                )
                .map_err(math_error)?
            } else {
                state.price.amount_out(collateral.net)?
            };
            if out < min_tokens_out {
                return Err(Error::Slippage);
            }
            TradeQuote {
                amount_in: collateral_in,
                amount_out: out,
                gross_collateral: collateral.gross,
                net_collateral: collateral.net,
                protocol_fee: collateral.fee,
                retained_fee: 0,
            }
        }
        QuoteRequest::BuyExactOutput {
            tokens_out,
            max_collateral_in,
        } => {
            let net = if let Some((coll_w, token_w)) = weights {
                try_calc_in_given_out(
                    state.recorded_collateral,
                    coll_w,
                    state.remaining_inventory,
                    token_w,
                    tokens_out,
                )
                .map_err(math_error)?
            } else {
                state.price.amount_in(tokens_out)?
            };
            let collateral = rate.gross_up(net).map_err(fee_error)?;
            if collateral.gross > max_collateral_in {
                return Err(Error::Slippage);
            }
            TradeQuote {
                amount_in: collateral.gross,
                amount_out: tokens_out,
                gross_collateral: collateral.gross,
                net_collateral: collateral.net,
                protocol_fee: collateral.fee,
                retained_fee: 0,
            }
        }
        QuoteRequest::SellExactInput {
            tokens_in,
            min_collateral_out,
        } => {
            let gross = if let Some((coll_w, token_w)) = weights {
                try_calc_out_given_in(
                    state.remaining_inventory,
                    token_w,
                    state.recorded_collateral,
                    coll_w,
                    tokens_in,
                )
                .map_err(math_error)?
            } else {
                state.price.sell_amount_out(tokens_in)?
            };
            let collateral = SellFeeRate::new(state.sell_fee_bps)
                .map_err(fee_error)?
                .split(gross, namespace_bps)
                .map_err(fee_error)?;
            if collateral.net < min_collateral_out {
                return Err(Error::Slippage);
            }
            TradeQuote {
                amount_in: tokens_in,
                amount_out: collateral.net,
                gross_collateral: collateral.gross,
                net_collateral: collateral.net,
                protocol_fee: collateral.protocol_fee,
                retained_fee: collateral.retained_fee,
            }
        }
    };
    if quote.amount_in == 0 || quote.amount_out == 0 || quote.net_collateral == 0 {
        return Err(Error::InvalidAmount);
    }
    if !sell && quote.amount_out > state.remaining_inventory {
        return Err(Error::InvalidAmount);
    }
    accounting(state, &quote, sell)?;
    Ok(quote)
}

pub(super) fn accounting(
    state: &PoolAccount,
    quote: &TradeQuote,
    sell: bool,
) -> Result<(u128, u128, u128), Error> {
    state
        .recorded_collateral
        .checked_add(state.accrued_fees)
        .ok_or(Error::Arithmetic)?;
    let (recorded, inventory) = if sell {
        let seed = match state.mode {
            PoolMode::FixedPrice => 0,
            PoolMode::Weighted {
                collateral_seed, ..
            } => collateral_seed,
        };
        let budget = state
            .recorded_collateral
            .checked_sub(seed)
            .ok_or(Error::Underfunded)?;
        if quote.gross_collateral > budget {
            return Err(Error::Underfunded);
        }
        let debit = quote
            .gross_collateral
            .checked_sub(quote.retained_fee)
            .ok_or(Error::Arithmetic)?;
        let recorded = state
            .recorded_collateral
            .checked_sub(debit)
            .ok_or(Error::Underfunded)?;
        if recorded < seed {
            return Err(Error::Underfunded);
        }
        (
            recorded,
            state
                .remaining_inventory
                .checked_add(quote.amount_in)
                .ok_or(Error::Arithmetic)?,
        )
    } else {
        (
            state
                .recorded_collateral
                .checked_add(quote.net_collateral)
                .ok_or(Error::Arithmetic)?,
            state
                .remaining_inventory
                .checked_sub(quote.amount_out)
                .ok_or(Error::Arithmetic)?,
        )
    };
    let fees = state
        .accrued_fees
        .checked_add(quote.protocol_fee)
        .ok_or(Error::Arithmetic)?;
    recorded.checked_add(fees).ok_or(Error::Arithmetic)?;
    Ok((recorded, fees, inventory))
}

fn fee_error(error: FeeError) -> Error {
    match error {
        FeeError::RateAboveDenominator => Error::InvalidFee,
        FeeError::InputConsumedByFees => Error::InvalidAmount,
        FeeError::Arithmetic => Error::Arithmetic,
    }
}

fn math_error(error: QuoteError) -> Error {
    match error {
        QuoteError::EmptyReserve => Error::Underfunded,
        QuoteError::InvalidWeights => Error::InvalidTerms,
        QuoteError::InvalidAmount | QuoteError::AmountOutOfBounds | QuoteError::Dust => {
            Error::InvalidAmount
        }
        QuoteError::Arithmetic => Error::Arithmetic,
        QuoteError::PowerUnderflow => Error::PowerUnderflow,
        QuoteError::Accuracy => Error::MathAccuracy,
        QuoteError::UnqualifiedScale => Error::InvalidState,
    }
}
