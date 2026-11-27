//! The bounded-pool state machine behind the neutral AMM interface.
//!
//! Knows about ordered real and virtual reserves, fees, slippage, expiry, and lifecycle.
//! Actor messages and token custody stay outside this state machine.
//!
//! Launch-specific allocation and vocabulary belong to the factory adapter.
//! Errors remain operation-specific to preserve the guest-facing API. Their hand-written
//! implementations keep error-derive dependencies out of the guest state machine.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![deny(clippy::arithmetic_side_effects)]

use borsh::{BorshDeserialize, BorshSerialize};
pub use sale_policy::{EndingDecision, EndingError, EndingPolicy, TimeBounds};
use sale_policy::{FeeError, FeeRate, SellFeeRate};

/// Why a pool could not be created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateError {
    /// A virtual reserve at or above [`VIRTUAL_RESERVE_BOUND`] would let `k` leave u128.
    VirtualReserveAboveBound,
    /// Zero virtual reserves make `k` zero and cannot price a swap.
    VirtualReserveZero,
    /// A configured depletion side must begin with a positive real reserve.
    DepletionReserveZero,
    InvalidEndingPolicy,
    InvalidSellFee,
    InvalidInvariant,
}

impl std::fmt::Display for CreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSellFee => f.write_str("sell fee must be below 10,000 basis points"),
            Self::InvalidInvariant => f.write_str("the saved pricing invariant is invalid"),
            Self::InvalidEndingPolicy => f.write_str("inconsistent curve ending policy"),
            Self::VirtualReserveAboveBound => {
                write!(
                    f,
                    "a virtual reserve at or above 2^64 would let k leave u128"
                )
            }
            Self::VirtualReserveZero => {
                write!(f, "virtual reserves must be non-zero")
            }
            Self::DepletionReserveZero => {
                write!(
                    f,
                    "the configured depletion-side real reserve must be non-zero"
                )
            }
        }
    }
}

impl std::error::Error for CreateError {}

/// Both initial virtual reserves stay below 2^64 so creation-time `k` fits `u128`.
/// Retained sell collateral increases the pricing invariant after creation.
pub const VIRTUAL_RESERVE_BOUND: u128 = 1 << 64;

/// A bounded constant-product pool with ordered token roles.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Pool {
    pub virtual_reserve0: u128,
    pub virtual_reserve1: u128,
    pub initial_k: u128,
    pub k: u128,
    pub real_reserve0: u128,
    pub real_reserve1: u128,
    /// Uncollected token1 fees, excluded from real and virtual reserves.
    pub fees_accrued: u128,
    /// Cumulative token1 fees paid to treasury.
    pub fees_collected: u128,
    pub ending_policy: EndingPolicy,
    pub sell_fee_bps: u16,
    pub close_timestamp: Option<u64>,
    pub close_on_depletion: Option<TokenSide>,
    pub lifecycle: PoolLifecycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TokenSide {
    Token0,
    Token1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolCreateParams {
    pub token0_amount: u128,
    pub token1_amount: u128,
    pub virtual_reserve0: u128,
    pub virtual_reserve1: u128,
    pub ending_policy: EndingPolicy,
    pub sell_fee_bps: u16,
    pub close_timestamp: Option<u64>,
    pub close_on_depletion: Option<TokenSide>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExactInputSwap {
    pub token_in: TokenSide,
    pub amount_in: u128,
    pub min_amount_out: u128,
    pub pool_fee_bps: u16,
    pub protocol_fee_bps: u16,
    pub now: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExactOutputSwap {
    pub token_in: TokenSide,
    pub amount_out: u128,
    pub max_amount_in: u128,
    pub pool_fee_bps: u16,
    pub protocol_fee_bps: u16,
    pub now: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeeSide {
    Input,
    Output,
}

/// The persisted lifecycle. Timestamp expiry is evaluated by [`Pool::effective_lifecycle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PoolLifecycle {
    Open,
    Closed,
    Withdrawn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwapOutcome {
    /// Gross debit from the trader.
    pub amount_in: u128,
    /// Amount used by the constant-product quote.
    pub effective_amount_in: u128,
    /// Collateral output before sell-side fees.
    pub raw_amount_out: u128,
    /// Amount received by the trader after sell-side fees.
    pub amount_out: u128,
    /// Fee accrued in the collateral vault for later collection.
    pub protocol_fee: u128,
    pub retained_fee: u128,
    /// Buys charge the collateral input; sells charge the collateral output.
    pub protocol_fee_side: FeeSide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapError {
    InvalidPolicy,
    Closed,
    ZeroAmount,
    /// A non-zero input would produce no output after conservative integer rounding.
    /// Rejecting this avoids accepting a value-destructive no-op trade.
    OutputTooSmall,
    /// A requested exact output would require no input after conservative rounding.
    /// Rejecting this prevents a zero-cost transfer.
    InputTooSmall,
    FeeAboveDenominator,
    PoolFeeUnsupported,
    InputConsumedByFees,
    ExactOutputRequiresCollateralInput,
    Arithmetic,
    SlippageExceeded,
    InsufficientRealOutputReserve,
}

impl std::fmt::Display for SwapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidPolicy => "the saved pool policy is invalid",
            Self::Closed => "the pool is closed",
            Self::ZeroAmount => "swap input or requested output must be non-zero",
            Self::OutputTooSmall => "the input is too small to produce one output unit",
            Self::InputTooSmall => "the requested output does not require one input unit",
            Self::FeeAboveDenominator => "protocol fee exceeds 10,000 basis points",
            Self::PoolFeeUnsupported => "per-swap pool fees are not supported",
            Self::InputConsumedByFees => "fees consume the entire input amount",
            Self::ExactOutputRequiresCollateralInput => {
                "exact-output swaps are supported only for collateral-input buys"
            }
            Self::Arithmetic => "the swap would overflow the supported arithmetic range",
            Self::SlippageExceeded => "the swap would exceed its slippage limit",
            Self::InsufficientRealOutputReserve => {
                "the pool lacks the real reserve for this output"
            }
        };
        f.write_str(message)
    }
}

impl std::error::Error for SwapError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WithdrawError {
    InvalidPolicy,
    StillOpen,
    AlreadyWithdrawn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseError {
    InvalidPolicy,
    AlreadyClosed,
}

impl Pool {
    /// Creates an ordered pool and records its initial pricing invariant.
    pub fn create(params: PoolCreateParams) -> Result<Self, CreateError> {
        let PoolCreateParams {
            token0_amount,
            token1_amount,
            virtual_reserve0,
            virtual_reserve1,
            ending_policy,
            sell_fee_bps,
            close_timestamp,
            close_on_depletion,
        } = params;
        if virtual_reserve0 >= VIRTUAL_RESERVE_BOUND || virtual_reserve1 >= VIRTUAL_RESERVE_BOUND {
            return Err(CreateError::VirtualReserveAboveBound);
        }
        if virtual_reserve0 == 0 || virtual_reserve1 == 0 {
            return Err(CreateError::VirtualReserveZero);
        }
        if matches!(close_on_depletion, Some(TokenSide::Token0)) && token0_amount == 0
            || matches!(close_on_depletion, Some(TokenSide::Token1)) && token1_amount == 0
        {
            return Err(CreateError::DepletionReserveZero);
        }
        SellFeeRate::new(sell_fee_bps).map_err(|_| CreateError::InvalidSellFee)?;
        validate_policies(ending_policy, close_timestamp, close_on_depletion)?;
        let k = virtual_reserve0
            .checked_mul(virtual_reserve1)
            .expect("both virtual reserves are below 2^64, so the product fits u128");
        Ok(Self {
            virtual_reserve0,
            virtual_reserve1,
            initial_k: k,
            k,
            real_reserve0: token0_amount,
            real_reserve1: token1_amount,
            fees_accrued: 0,
            fees_collected: 0,
            ending_policy,
            sell_fee_bps,
            close_timestamp,
            close_on_depletion,
            lifecycle: PoolLifecycle::Open,
        })
    }

    pub fn swap_exact_input(&mut self, request: ExactInputSwap) -> Result<SwapOutcome, SwapError> {
        let ExactInputSwap {
            token_in,
            amount_in,
            min_amount_out,
            pool_fee_bps,
            protocol_fee_bps,
            now,
        } = request;
        self.ensure_swappable(now)?;
        if amount_in == 0 {
            return Err(SwapError::ZeroAmount);
        }
        if pool_fee_bps != 0 {
            return Err(SwapError::PoolFeeUnsupported);
        }
        let fee_rate = FeeRate::new(protocol_fee_bps).map_err(map_fee_error)?;
        let is_buy = token_in == TokenSide::Token1;
        let (effective_input, protocol_fee) = if is_buy {
            let split = fee_rate.split(amount_in).map_err(map_fee_error)?;
            (split.net, split.fee)
        } else {
            (amount_in, 0)
        };
        if effective_input == 0 {
            return Err(SwapError::InputConsumedByFees);
        }
        let (virtual_in, virtual_out, real_in, real_out) = self.reserves(token_in);
        let amount_out = curve_math::exact_input_amount_out_floor(
            virtual_in,
            virtual_out,
            self.k,
            effective_input,
        )
        .map_err(|_| SwapError::Arithmetic)?;
        if amount_out == 0 {
            return Err(SwapError::OutputTooSmall);
        }
        let (recipient_amount_out, protocol_fee, retained_fee, protocol_fee_side) = if is_buy {
            (amount_out, protocol_fee, 0, FeeSide::Input)
        } else {
            let split = SellFeeRate::new(self.sell_fee_bps)
                .map_err(map_fee_error)?
                .split(amount_out, protocol_fee_bps)
                .map_err(|error| match error {
                    FeeError::InputConsumedByFees => SwapError::OutputTooSmall,
                    other => map_fee_error(other),
                })?;
            (
                split.net,
                split.protocol_fee,
                split.retained_fee,
                FeeSide::Output,
            )
        };
        if recipient_amount_out < min_amount_out {
            return Err(SwapError::SlippageExceeded);
        }
        if amount_out > real_out {
            return Err(SwapError::InsufficientRealOutputReserve);
        }
        let new_virtual_in = virtual_in
            .checked_add(effective_input)
            .ok_or(SwapError::Arithmetic)?;
        let new_virtual_out = virtual_out
            .checked_sub(amount_out)
            .and_then(|reserve| reserve.checked_add(retained_fee))
            .ok_or(SwapError::Arithmetic)?;
        let new_k = if is_buy {
            self.k
        } else {
            new_virtual_in
                .checked_mul(retained_fee)
                .and_then(|growth| self.k.checked_add(growth))
                .ok_or(SwapError::Arithmetic)?
        };
        let fees_accrued = self
            .fees_accrued
            .checked_add(protocol_fee)
            .ok_or(SwapError::Arithmetic)?;
        self.fees_collected
            .checked_add(fees_accrued)
            .ok_or(SwapError::Arithmetic)?;
        // Check vault accounting before mutating any state.
        let collateral_reserve = if token_in == TokenSide::Token1 {
            real_in.checked_add(effective_input)
        } else {
            real_out
                .checked_sub(amount_out)
                .and_then(|reserve| reserve.checked_add(retained_fee))
        }
        .ok_or(SwapError::Arithmetic)?;
        collateral_reserve
            .checked_add(fees_accrued)
            .ok_or(SwapError::Arithmetic)?;
        self.set_reserves(
            token_in,
            new_virtual_in,
            new_virtual_out,
            real_in
                .checked_add(effective_input)
                .ok_or(SwapError::Arithmetic)?,
            real_out
                .checked_sub(amount_out)
                .and_then(|reserve| reserve.checked_add(retained_fee))
                .ok_or(SwapError::Arithmetic)?,
        );
        self.k = new_k;
        self.fees_accrued = fees_accrued;
        self.close_if_depleted();
        Ok(SwapOutcome {
            amount_in,
            effective_amount_in: effective_input,
            raw_amount_out: amount_out,
            amount_out: recipient_amount_out,
            protocol_fee,
            retained_fee,
            protocol_fee_side,
        })
    }

    pub fn swap_exact_output(
        &mut self,
        request: ExactOutputSwap,
    ) -> Result<SwapOutcome, SwapError> {
        let ExactOutputSwap {
            token_in,
            amount_out,
            max_amount_in,
            pool_fee_bps,
            protocol_fee_bps,
            now,
        } = request;
        self.ensure_swappable(now)?;
        if token_in != TokenSide::Token1 {
            return Err(SwapError::ExactOutputRequiresCollateralInput);
        }
        if pool_fee_bps != 0 {
            return Err(SwapError::PoolFeeUnsupported);
        }
        if amount_out == 0 {
            return Err(SwapError::ZeroAmount);
        }
        let (virtual_in, virtual_out, real_in, real_out) = self.reserves(token_in);
        if amount_out > real_out {
            return Err(SwapError::InsufficientRealOutputReserve);
        }
        let required_effective_input =
            curve_math::exact_output_amount_in_ceil(virtual_in, virtual_out, self.k, amount_out)
                .map_err(|_| SwapError::Arithmetic)?;
        if required_effective_input == 0 {
            return Err(SwapError::InputTooSmall);
        }
        let rate = FeeRate::new(protocol_fee_bps).map_err(map_fee_error)?;
        let split = rate
            .gross_up(required_effective_input)
            .map_err(map_fee_error)?;
        let amount_in = split.gross;
        let protocol_fee = split.fee;
        let effective_input = split.net;
        if amount_in > max_amount_in {
            return Err(SwapError::SlippageExceeded);
        }
        let new_virtual_in = virtual_in
            .checked_add(effective_input)
            .ok_or(SwapError::Arithmetic)?;
        let new_virtual_out = virtual_out
            .checked_sub(amount_out)
            .ok_or(SwapError::Arithmetic)?;
        let fees_accrued = self
            .fees_accrued
            .checked_add(protocol_fee)
            .ok_or(SwapError::Arithmetic)?;
        self.fees_collected
            .checked_add(fees_accrued)
            .ok_or(SwapError::Arithmetic)?;
        // Check vault accounting before mutating any state.
        let collateral_reserve = if token_in == TokenSide::Token1 {
            real_in.checked_add(effective_input)
        } else {
            real_out.checked_sub(amount_out)
        }
        .ok_or(SwapError::Arithmetic)?;
        collateral_reserve
            .checked_add(fees_accrued)
            .ok_or(SwapError::Arithmetic)?;
        self.set_reserves(
            token_in,
            new_virtual_in,
            new_virtual_out,
            real_in
                .checked_add(effective_input)
                .ok_or(SwapError::Arithmetic)?,
            real_out
                .checked_sub(amount_out)
                .ok_or(SwapError::Arithmetic)?,
        );
        self.fees_accrued = fees_accrued;
        self.close_if_depleted();
        Ok(SwapOutcome {
            amount_in,
            effective_amount_in: effective_input,
            raw_amount_out: amount_out,
            amount_out,
            protocol_fee,
            retained_fee: 0,
            protocol_fee_side: FeeSide::Input,
        })
    }

    /// Permissionless and idempotent; neither pricing nor lifecycle changes.
    pub fn collect_fees(&mut self) -> Result<u128, SwapError> {
        self.validate_policy()
            .map_err(|_| SwapError::InvalidPolicy)?;
        let amount = self.fees_accrued;
        let total = self
            .fees_collected
            .checked_add(amount)
            .ok_or(SwapError::Arithmetic)?;
        self.fees_collected = total;
        self.fees_accrued = 0;
        Ok(amount)
    }

    pub fn close_pool(&mut self, now: u64) -> Result<(), CloseError> {
        self.validate_policy()
            .map_err(|_| CloseError::InvalidPolicy)?;
        if self.effective_lifecycle(now) != PoolLifecycle::Open {
            return Err(CloseError::AlreadyClosed);
        }
        self.lifecycle = PoolLifecycle::Closed;
        Ok(())
    }

    pub fn withdraw_reserves(&mut self, now: u64) -> Result<(u128, u128), WithdrawError> {
        self.validate_policy()
            .map_err(|_| WithdrawError::InvalidPolicy)?;
        if self.lifecycle == PoolLifecycle::Withdrawn {
            return Err(WithdrawError::AlreadyWithdrawn);
        }
        if self.effective_lifecycle(now) == PoolLifecycle::Open {
            return Err(WithdrawError::StillOpen);
        }
        let reserves = (self.real_reserve0, self.real_reserve1);
        self.real_reserve0 = 0;
        self.real_reserve1 = 0;
        self.lifecycle = PoolLifecycle::Withdrawn;
        Ok(reserves)
    }

    pub fn validate_policy(&self) -> Result<(), CreateError> {
        SellFeeRate::new(self.sell_fee_bps).map_err(|_| CreateError::InvalidSellFee)?;
        if self.initial_k == 0
            || self.k < self.initial_k
            || self.virtual_reserve0 == 0
            || self.virtual_reserve1 < self.k.div_ceil(self.virtual_reserve0)
            || self.sell_fee_bps == 0 && self.k != self.initial_k
        {
            return Err(CreateError::InvalidInvariant);
        }
        validate_policies(
            self.ending_policy,
            self.close_timestamp,
            self.close_on_depletion,
        )
    }

    fn remaining(&self) -> u128 {
        match self.close_on_depletion {
            Some(TokenSide::Token0) => self.real_reserve0,
            Some(TokenSide::Token1) => self.real_reserve1,
            None => 1,
        }
    }

    pub fn trade_bounds(&self) -> Result<TimeBounds, SwapError> {
        self.validate_policy()
            .map_err(|_| SwapError::InvalidPolicy)?;
        if self.lifecycle != PoolLifecycle::Open {
            return Err(SwapError::Closed);
        }
        self.ending_policy
            .trade_bounds(None, self.close_timestamp, self.remaining())
            .map_err(|error| match error {
                EndingError::Ended => SwapError::Closed,
                _ => SwapError::InvalidPolicy,
            })
    }

    pub fn ending_decision(&self) -> Result<EndingDecision, EndingError> {
        self.validate_policy()
            .map_err(|_| EndingError::InvalidWindow)?;
        // Stored closure stays valid after withdrawal clears the reserves.
        if self.lifecycle != PoolLifecycle::Open {
            return Ok(EndingDecision::Manual);
        }
        self.ending_policy
            .settlement(self.close_timestamp, self.remaining())
    }

    #[must_use]
    pub fn effective_lifecycle(&self, now: u64) -> PoolLifecycle {
        if self.lifecycle != PoolLifecycle::Open {
            return self.lifecycle;
        }
        match self.trade_bounds() {
            Ok(bounds) if bounds.contains(now) => PoolLifecycle::Open,
            _ => PoolLifecycle::Closed,
        }
    }

    fn ensure_swappable(&self, now: u64) -> Result<(), SwapError> {
        if self.trade_bounds()?.contains(now) {
            Ok(())
        } else {
            Err(SwapError::Closed)
        }
    }

    fn close_if_depleted(&mut self) {
        if self.lifecycle == PoolLifecycle::Open
            && self.ending_decision() == Ok(EndingDecision::Depleted)
        {
            self.lifecycle = PoolLifecycle::Closed;
        }
    }

    fn reserves(&self, token_in: TokenSide) -> (u128, u128, u128, u128) {
        match token_in {
            TokenSide::Token0 => (
                self.virtual_reserve0,
                self.virtual_reserve1,
                self.real_reserve0,
                self.real_reserve1,
            ),
            TokenSide::Token1 => (
                self.virtual_reserve1,
                self.virtual_reserve0,
                self.real_reserve1,
                self.real_reserve0,
            ),
        }
    }

    fn set_reserves(
        &mut self,
        token_in: TokenSide,
        virtual_in: u128,
        virtual_out: u128,
        real_in: u128,
        real_out: u128,
    ) {
        match token_in {
            TokenSide::Token0 => {
                self.virtual_reserve0 = virtual_in;
                self.virtual_reserve1 = virtual_out;
                self.real_reserve0 = real_in;
                self.real_reserve1 = real_out;
            }
            TokenSide::Token1 => {
                self.virtual_reserve1 = virtual_in;
                self.virtual_reserve0 = virtual_out;
                self.real_reserve1 = real_in;
                self.real_reserve0 = real_out;
            }
        }
    }
}

fn validate_policies(
    ending: EndingPolicy,
    end: Option<u64>,
    side: Option<TokenSide>,
) -> Result<(), CreateError> {
    ending
        .validate(None, end)
        .map_err(|_| CreateError::InvalidEndingPolicy)?;
    if matches!(ending, EndingPolicy::Depletion | EndingPolicy::Either) != side.is_some() {
        return Err(CreateError::InvalidEndingPolicy);
    }
    Ok(())
}

fn map_fee_error(error: FeeError) -> SwapError {
    match error {
        FeeError::RateAboveDenominator => SwapError::FeeAboveDenominator,
        FeeError::InputConsumedByFees => SwapError::InputConsumedByFees,
        FeeError::Arithmetic => SwapError::Arithmetic,
    }
}

#[cfg(test)]
mod tests;
