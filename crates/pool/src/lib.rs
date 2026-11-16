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
use sale_policy::{FeeError, FeeRate};

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
}

impl std::fmt::Display for CreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
/// The pool retains that product as virtual reserves move after swaps.
pub const VIRTUAL_RESERVE_BOUND: u128 = 1 << 64;

/// A bounded constant-product pool with ordered token roles.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Pool {
    pub virtual_reserve0: u128,
    pub virtual_reserve1: u128,
    pub k: u128,
    pub real_reserve0: u128,
    pub real_reserve1: u128,
    /// Uncollected token1 fees, excluded from real and virtual reserves.
    pub fees_accrued: u128,
    /// Cumulative token1 fees paid to treasury.
    pub fees_collected: u128,
    pub ending_policy: EndingPolicy,
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
    /// Collateral output before a sell-side protocol fee.
    pub raw_amount_out: u128,
    /// Amount received by the trader after a possible sell-side protocol fee.
    pub amount_out: u128,
    /// Fee accrued in the collateral vault for later collection.
    pub protocol_fee: u128,
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
            Self::PoolFeeUnsupported => "retained pool fees are not supported by this launchpad",
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
    /// Creates an ordered pool and fixes `k` for its lifetime.
    pub fn create(params: PoolCreateParams) -> Result<Self, CreateError> {
        let PoolCreateParams {
            token0_amount,
            token1_amount,
            virtual_reserve0,
            virtual_reserve1,
            ending_policy,
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
        validate_policies(ending_policy, close_timestamp, close_on_depletion)?;
        let k = virtual_reserve0
            .checked_mul(virtual_reserve1)
            .expect("both virtual reserves are below 2^64, so the product fits u128");
        Ok(Self {
            virtual_reserve0,
            virtual_reserve1,
            k,
            real_reserve0: token0_amount,
            real_reserve1: token1_amount,
            fees_accrued: 0,
            fees_collected: 0,
            ending_policy,
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
        let (recipient_amount_out, protocol_fee, protocol_fee_side) = if is_buy {
            (amount_out, protocol_fee, FeeSide::Input)
        } else {
            let split = fee_rate.split(amount_out).map_err(map_fee_error)?;
            if split.net == 0 {
                return Err(SwapError::OutputTooSmall);
            }
            (split.net, split.fee, FeeSide::Output)
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
            amount_out: recipient_amount_out,
            protocol_fee,
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
mod tests {
    use super::*;

    #[test]
    fn create_pool_records_both_real_reserves_and_optional_close_time() {
        let pool = Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 25,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Scheduled,
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
    fn exact_input_swaps_token0_for_token1_through_the_public_pool_api() {
        let mut pool = Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 100,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Manual,
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
    fn final_exact_input_swap_closes_a_pool_configured_to_close_on_token0_depletion() {
        let mut pool = Pool::create(PoolCreateParams {
            token0_amount: 80,
            token1_amount: 0,
            virtual_reserve0: 100,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Depletion,
            close_timestamp: None,
            close_on_depletion: Some(TokenSide::Token0),
        })
        .expect("valid pool");

        let outcome = pool
            .swap_exact_input(ExactInputSwap {
                token_in: TokenSide::Token1,
                amount_in: 400,
                min_amount_out: 80,
                pool_fee_bps: 0,
                protocol_fee_bps: 0,
                now: 1,
            })
            .expect("final swap succeeds");

        assert_eq!(outcome.amount_out, 80);
        assert_eq!(pool.real_reserve0, 0);
        assert_eq!(pool.effective_lifecycle(1), PoolLifecycle::Closed);
        assert_eq!(
            pool.swap_exact_input(ExactInputSwap {
                token_in: TokenSide::Token1,
                amount_in: 1,
                min_amount_out: 0,
                pool_fee_bps: 0,
                protocol_fee_bps: 0,
                now: 1
            }),
            Err(SwapError::Closed)
        );
    }

    #[test]
    fn final_exact_output_swap_closes_a_pool_configured_to_close_on_token0_depletion() {
        let mut pool = Pool::create(PoolCreateParams {
            token0_amount: 80,
            token1_amount: 0,
            virtual_reserve0: 100,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Depletion,
            close_timestamp: None,
            close_on_depletion: Some(TokenSide::Token0),
        })
        .expect("valid pool");

        pool.swap_exact_output(ExactOutputSwap {
            token_in: TokenSide::Token1,
            amount_out: 80,
            max_amount_in: u128::MAX,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 1,
        })
        .expect("final swap succeeds");

        assert_eq!(pool.real_reserve0, 0);
        assert_eq!(pool.effective_lifecycle(1), PoolLifecycle::Closed);
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
                close_timestamp: None,
                close_on_depletion: Some(TokenSide::Token0)
            }),
            Err(CreateError::DepletionReserveZero)
        );
    }

    #[test]
    fn protocol_fee_is_collateral_input_on_buy_and_collateral_output_on_sell() {
        let mut buy_pool = Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 100,
            virtual_reserve0: 1_000,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Manual,
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
    fn exact_output_swaps_token1_for_token0_with_the_fee_inside_max_input() {
        let mut pool = Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 100,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Manual,
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
            (TokenSide::Token0, 250, 10_000, SwapError::OutputTooSmall),
        ] {
            let mut pool = Pool::create(PoolCreateParams {
                token0_amount: 800,
                token1_amount: 100,
                virtual_reserve0: 1_000,
                virtual_reserve1: 100,
                ending_policy: EndingPolicy::Manual,
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
    fn exact_output_is_the_smallest_zero_fee_input_that_reaches_the_requested_amount() {
        for amount_out in [1, 100, 200, 799] {
            let mut quoted = Pool::create(PoolCreateParams {
                token0_amount: 800,
                token1_amount: 100,
                virtual_reserve0: 1_000,
                virtual_reserve1: 100,
                ending_policy: EndingPolicy::Manual,
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
    fn expiry_allows_full_withdrawal_without_a_close_transaction_and_retires_the_pool() {
        let mut pool = Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 25,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Scheduled,
            close_timestamp: Some(42),
            close_on_depletion: None,
        })
        .expect("valid pool");
        assert_eq!(pool.withdraw_reserves(42), Ok((800, 25)));
        assert_eq!(pool.real_reserve0, 0);
        assert_eq!(pool.real_reserve1, 0);
        assert_eq!(pool.lifecycle, PoolLifecycle::Withdrawn);
        assert_eq!(
            pool.withdraw_reserves(43),
            Err(WithdrawError::AlreadyWithdrawn)
        );
    }

    #[test]
    fn swaps_are_rejected_at_the_close_timestamp_without_mutating_reserves() {
        let mut pool = Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 100,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Scheduled,
            close_timestamp: Some(42),
            close_on_depletion: None,
        })
        .expect("valid pool");
        let before = pool.clone();
        assert_eq!(
            pool.swap_exact_input(ExactInputSwap {
                token_in: TokenSide::Token0,
                amount_in: 250,
                min_amount_out: 20,
                pool_fee_bps: 0,
                protocol_fee_bps: 0,
                now: 42
            }),
            Err(SwapError::Closed)
        );
        assert_eq!(pool, before);
    }

    #[test]
    fn a_swap_cannot_exceed_the_selected_real_output_reserve() {
        let mut pool = Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 10,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Manual,
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

    #[test]
    fn manual_close_allows_full_withdrawal_for_a_pool_without_expiry() {
        let mut pool = Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 25,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Manual,
            close_timestamp: None,
            close_on_depletion: None,
        })
        .expect("valid pool");
        assert_eq!(pool.withdraw_reserves(1), Err(WithdrawError::StillOpen));
        assert_eq!(pool.close_pool(1), Ok(()));
        assert_eq!(pool.withdraw_reserves(1), Ok((800, 25)));
        assert_eq!(pool.lifecycle, PoolLifecycle::Withdrawn);
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
                close_timestamp: None,
                close_on_depletion: None
            }),
            Err(CreateError::VirtualReserveZero)
        );
    }

    #[test]
    fn a_pool_survives_a_borsh_round_trip() {
        let pool = Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 200,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Scheduled,
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
            close_timestamp: Some(42),
            close_on_depletion: None,
        })
        .expect("valid pool");
        let expected: [u8; 124] = [
            0xe8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0xa0, 0x86, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc8, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x01, 0x01, 0x2a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        assert_eq!(borsh::to_vec(&pool).expect("pool serialises"), expected);
    }
    fn policy_params() -> PoolCreateParams {
        PoolCreateParams {
            token0_amount: 80,
            token1_amount: 100,
            virtual_reserve0: 100,
            virtual_reserve1: 100,
            ending_policy: EndingPolicy::Manual,
            close_timestamp: None,
            close_on_depletion: None,
        }
    }

    #[test]
    fn direct_curve_policy_matrix_preserves_all_existing_endings() {
        for ending in [
            EndingPolicy::Manual,
            EndingPolicy::Scheduled,
            EndingPolicy::Depletion,
            EndingPolicy::Either,
        ] {
            for end in [None, Some(42)] {
                for side in [None, Some(TokenSide::Token0), Some(TokenSide::Token1)] {
                    let params = PoolCreateParams {
                        ending_policy: ending,
                        close_timestamp: end,
                        close_on_depletion: side,
                        ..policy_params()
                    };
                    let valid = match ending {
                        EndingPolicy::Manual => end.is_none() && side.is_none(),
                        EndingPolicy::Scheduled => end.is_some() && side.is_none(),
                        EndingPolicy::Depletion => end.is_none() && side.is_some(),
                        EndingPolicy::Either => end.is_some() && side.is_some(),
                    };
                    let result = Pool::create(params);
                    if valid {
                        let mut pool = result.unwrap();
                        assert_eq!(pool.validate_policy(), Ok(()));
                        assert_eq!(pool.close_pool(1), Ok(()));
                        assert_eq!(pool.withdraw_reserves(1), Ok((80, 100)));
                    } else {
                        assert_eq!(result, Err(CreateError::InvalidEndingPolicy));
                    }
                }
            }
        }
    }

    #[test]
    fn malformed_saved_policy_rejects_all_mutations_before_other_errors() {
        let mut pool = Pool::create(policy_params()).unwrap();
        pool.close_on_depletion = Some(TokenSide::Token0);
        assert_invalid_saved_policy(pool);
    }

    fn assert_invalid_saved_policy(mut pool: Pool) {
        let before = pool.clone();
        assert_eq!(
            pool.swap_exact_input(ExactInputSwap {
                token_in: TokenSide::Token0,
                amount_in: 0,
                min_amount_out: 0,
                pool_fee_bps: 1,
                protocol_fee_bps: 10_001,
                now: 1,
            }),
            Err(SwapError::InvalidPolicy)
        );
        assert_eq!(
            pool.swap_exact_output(ExactOutputSwap {
                token_in: TokenSide::Token1,
                amount_out: 0,
                max_amount_in: 0,
                pool_fee_bps: 1,
                protocol_fee_bps: 10_001,
                now: 1,
            }),
            Err(SwapError::InvalidPolicy)
        );
        assert_eq!(pool.collect_fees(), Err(SwapError::InvalidPolicy));
        assert_eq!(pool.close_pool(1), Err(CloseError::InvalidPolicy));
        assert_eq!(
            pool.withdraw_reserves(42),
            Err(WithdrawError::InvalidPolicy)
        );
        assert_eq!(pool, before);
    }

    #[test]
    fn selected_reserve_and_expiry_are_independent_ending_facts() {
        for side in [TokenSide::Token0, TokenSide::Token1] {
            let mut pool = Pool::create(PoolCreateParams {
                ending_policy: EndingPolicy::Either,
                close_on_depletion: Some(side),
                close_timestamp: Some(42),
                ..policy_params()
            })
            .unwrap();
            assert_eq!(pool.effective_lifecycle(41), PoolLifecycle::Open);
            assert_eq!(pool.effective_lifecycle(42), PoolLifecycle::Closed);
            assert_eq!(
                pool.ending_decision(),
                Ok(EndingDecision::ScheduledEnd { timestamp: 42 })
            );
            match side {
                TokenSide::Token0 => pool.real_reserve0 = 0,
                TokenSide::Token1 => pool.real_reserve1 = 0,
            }
            assert_eq!(pool.ending_decision(), Ok(EndingDecision::Depleted));
            assert_eq!(pool.trade_bounds(), Err(SwapError::Closed));
            assert_eq!(pool.effective_lifecycle(1), PoolLifecycle::Closed);
        }
    }

    #[test]
    fn cleared_reserves_never_supply_depletion_evidence_for_stored_closure() {
        let mut pool = Pool::create(PoolCreateParams {
            ending_policy: EndingPolicy::Either,
            close_on_depletion: Some(TokenSide::Token0),
            close_timestamp: Some(42),
            ..policy_params()
        })
        .unwrap();
        pool.close_pool(1).unwrap();
        assert_eq!(pool.ending_decision(), Ok(EndingDecision::Manual));
        pool.withdraw_reserves(1).unwrap();
        assert_eq!(pool.ending_decision(), Ok(EndingDecision::Manual));
        assert_eq!(pool.effective_lifecycle(1), PoolLifecycle::Withdrawn);
    }

    #[test]
    fn scheduled_sellout_stays_open_and_rejects_early_withdrawal() {
        let mut pool = Pool::create(PoolCreateParams {
            ending_policy: EndingPolicy::Scheduled,
            close_timestamp: Some(42),
            ..policy_params()
        })
        .unwrap();
        pool.swap_exact_output(ExactOutputSwap {
            token_in: TokenSide::Token1,
            amount_out: 80,
            max_amount_in: 400,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 1,
        })
        .unwrap();
        assert_eq!(pool.real_reserve0, 0);
        assert_eq!(pool.lifecycle, PoolLifecycle::Open);
        assert_eq!(pool.effective_lifecycle(41), PoolLifecycle::Open);
        assert_eq!(pool.withdraw_reserves(41), Err(WithdrawError::StillOpen));
        assert_eq!(
            pool.ending_decision(),
            Ok(EndingDecision::ScheduledEnd { timestamp: 42 })
        );
        assert_eq!(
            pool.swap_exact_output(ExactOutputSwap {
                token_in: TokenSide::Token1,
                amount_out: 1,
                max_amount_in: 100,
                pool_fee_bps: 0,
                protocol_fee_bps: 0,
                now: 1,
            }),
            Err(SwapError::InsufficientRealOutputReserve)
        );
        pool.swap_exact_input(ExactInputSwap {
            token_in: TokenSide::Token0,
            amount_in: 1,
            min_amount_out: 1,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 41,
        })
        .unwrap();
        assert_eq!(pool.real_reserve0, 1);
        assert_eq!(pool.lifecycle, PoolLifecycle::Open);
        assert!(pool.withdraw_reserves(42).is_ok());
    }

    #[test]
    fn selected_depletion_cannot_reopen_when_inventory_returns() {
        for (ending, end) in [
            (EndingPolicy::Depletion, None),
            (EndingPolicy::Either, Some(42)),
        ] {
            let mut pool = Pool::create(PoolCreateParams {
                ending_policy: ending,
                close_timestamp: end,
                close_on_depletion: Some(TokenSide::Token0),
                ..policy_params()
            })
            .unwrap();
            pool.swap_exact_output(ExactOutputSwap {
                token_in: TokenSide::Token1,
                amount_out: 80,
                max_amount_in: 400,
                pool_fee_bps: 0,
                protocol_fee_bps: 0,
                now: 1,
            })
            .unwrap();
            assert_eq!(pool.lifecycle, PoolLifecycle::Closed);
            pool.real_reserve0 = 1;
            assert_eq!(pool.effective_lifecycle(1), PoolLifecycle::Closed);
            assert_eq!(
                pool.swap_exact_input(ExactInputSwap {
                    token_in: TokenSide::Token0,
                    amount_in: 1,
                    min_amount_out: 0,
                    pool_fee_bps: 0,
                    protocol_fee_bps: 0,
                    now: 1,
                }),
                Err(SwapError::Closed)
            );
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
}
