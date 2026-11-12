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
    let floor = whole
        .checked_add(
            numerator
                .checked_div(divisor)
                .ok_or(ArithmeticError::Overflow)?,
        )
        .ok_or(ArithmeticError::Overflow)?;
    let remainder = numerator
        .checked_rem(divisor)
        .ok_or(ArithmeticError::Overflow)?;
    floor
        .checked_add(u128::from(rounding == Rounding::Ceil && remainder != 0))
        .ok_or(ArithmeticError::Overflow)
}
