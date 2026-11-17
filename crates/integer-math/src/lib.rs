//! Checked u128 arithmetic without sale rules.

#![no_std]
#![deny(clippy::arithmetic_side_effects)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rounding {
    Floor,
    Ceil,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithmeticError {
    DivisionByZero,
    Overflow,
}

pub fn div_ceil(numerator: u128, divisor: u128) -> Result<u128, ArithmeticError> {
    let floor = numerator
        .checked_div(divisor)
        .ok_or(ArithmeticError::DivisionByZero)?;
    let remainder = numerator
        .checked_rem(divisor)
        .ok_or(ArithmeticError::DivisionByZero)?;
    floor
        .checked_add(u128::from(remainder != 0))
        .ok_or(ArithmeticError::Overflow)
}

/// Intermediate u128 overflow is rejected even when the final result would fit.
pub fn mul_div(
    value: u128,
    multiplier: u128,
    divisor: u128,
    rounding: Rounding,
) -> Result<u128, ArithmeticError> {
    let quotient = value
        .checked_div(divisor)
        .ok_or(ArithmeticError::DivisionByZero)?;
    let whole = quotient
        .checked_mul(multiplier)
        .ok_or(ArithmeticError::Overflow)?;
    let numerator = value
        .checked_rem(divisor)
        .and_then(|remainder| remainder.checked_mul(multiplier))
        .ok_or(ArithmeticError::Overflow)?;
    let fraction = match rounding {
        Rounding::Floor => numerator
            .checked_div(divisor)
            .ok_or(ArithmeticError::DivisionByZero)?,
        Rounding::Ceil => div_ceil(numerator, divisor)?,
    };
    whole.checked_add(fraction).ok_or(ArithmeticError::Overflow)
}
