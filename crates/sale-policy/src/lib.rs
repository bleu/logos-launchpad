//! Pure sale rules. Account authority, pricing state and token custody stay with callers.

#![deny(clippy::arithmetic_side_effects)]

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

impl FeeRate {
    pub fn new(bps: u16) -> Result<Self, FeeError> {
        if u128::from(bps) > BASIS_POINTS {
            return Err(FeeError::RateAboveDenominator);
        }
        Ok(Self(bps))
    }

    /// The caller chooses whether net collateral prices the trade or pays the creator.
    pub fn split(self, gross: u128) -> Result<CollateralSplit, FeeError> {
        let fee = mul_div_ceil(gross, u128::from(self.0), BASIS_POINTS)?;
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
        let gross = mul_div_ceil(net, BASIS_POINTS, divisor)?;
        self.split(gross)
    }
}

fn mul_div_ceil(value: u128, multiplier: u128, divisor: u128) -> Result<u128, FeeError> {
    let whole = value
        .checked_div(divisor)
        .and_then(|quotient| quotient.checked_mul(multiplier))
        .ok_or(FeeError::Arithmetic)?;
    let numerator = value
        .checked_rem(divisor)
        .and_then(|remainder| remainder.checked_mul(multiplier))
        .ok_or(FeeError::Arithmetic)?;
    let floor = whole
        .checked_add(numerator.checked_div(divisor).ok_or(FeeError::Arithmetic)?)
        .ok_or(FeeError::Arithmetic)?;
    floor
        .checked_add(u128::from(
            numerator.checked_rem(divisor).ok_or(FeeError::Arithmetic)? != 0,
        ))
        .ok_or(FeeError::Arithmetic)
}
