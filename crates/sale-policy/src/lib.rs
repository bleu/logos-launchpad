//! Pure sale rules. Account authority, pricing state and token custody stay with callers.

#![deny(clippy::arithmetic_side_effects)]

mod controls;
pub mod events;
mod settlement;
pub use controls::{PauseError, PausePolicy, TradeControl};
pub use settlement::{
    GraduationPolicy, SettlementAmounts, SettlementError, SettlementPayouts, SettlementProgress,
    SettlementRoute,
};
mod terms;
pub use terms::{
    EndingDecision, EndingError, EndingPolicy, EndingReason, InvalidTradeMode, TimeBounds,
    TradeMode,
};

use integer_math::{Rounding, mul_div};

const BASIS_POINTS: u128 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeeError {
    RateAboveDenominator,
    InputConsumedByFees,
    Arithmetic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeeRate(u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollateralSplit {
    pub gross: u128,
    pub net: u128,
    pub fee: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SellFeeRate(u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SellCollateralSplit {
    pub gross: u128,
    pub net: u128,
    pub protocol_fee: u128,
    pub retained_fee: u128,
}

impl SellFeeRate {
    pub fn new(bps: u16) -> Result<Self, FeeError> {
        if u128::from(bps) >= BASIS_POINTS {
            return Err(FeeError::RateAboveDenominator);
        }
        Ok(Self(bps))
    }

    /// Both fees round up independently from gross sell collateral.
    pub fn split(self, gross: u128, namespace_bps: u16) -> Result<SellCollateralSplit, FeeError> {
        let combined = u128::from(self.0)
            .checked_add(u128::from(namespace_bps))
            .ok_or(FeeError::Arithmetic)?;
        if combined >= BASIS_POINTS {
            return Err(FeeError::RateAboveDenominator);
        }
        let protocol_fee = FeeRate::new(namespace_bps)?.split(gross)?.fee;
        let retained_fee = FeeRate::new(self.0)?.split(gross)?.fee;
        let net = gross
            .checked_sub(protocol_fee)
            .and_then(|net| net.checked_sub(retained_fee))
            .filter(|net| *net > 0)
            .ok_or(FeeError::InputConsumedByFees)?;
        Ok(SellCollateralSplit {
            gross,
            net,
            protocol_fee,
            retained_fee,
        })
    }
}

impl FeeRate {
    pub fn new(bps: u16) -> Result<Self, FeeError> {
        if u128::from(bps) > BASIS_POINTS {
            return Err(FeeError::RateAboveDenominator);
        }
        Ok(Self(bps))
    }

    /// Splits gross collateral into its net amount and namespace fee.
    pub fn split(self, gross: u128) -> Result<CollateralSplit, FeeError> {
        let fee = mul_div(gross, u128::from(self.0), BASIS_POINTS, Rounding::Ceil)
            .map_err(|_| FeeError::Arithmetic)?;
        let net = gross.checked_sub(fee).ok_or(FeeError::Arithmetic)?;
        Ok(CollateralSplit { gross, net, fee })
    }

    /// Smallest gross payment whose net covers the requested collateral.
    pub fn gross_up(self, net: u128) -> Result<CollateralSplit, FeeError> {
        let divisor = BASIS_POINTS
            .checked_sub(u128::from(self.0))
            .ok_or(FeeError::Arithmetic)?;
        if divisor == 0 {
            return Err(FeeError::InputConsumedByFees);
        }
        let gross = mul_div(net, BASIS_POINTS, divisor, Rounding::Ceil)
            .map_err(|_| FeeError::Arithmetic)?;
        self.split(gross)
    }
}

pub mod block_allocation;
