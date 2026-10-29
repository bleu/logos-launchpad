//! Checked scale-52 quotes and the one-basis-point arithmetic-loss policy.
//! See `docs/error-analysis.md`. Fees and sale policy are not part of this API.

use crate::checked_uint::mul_div;
use crate::weighted::{ratio_down, ratio_up};
use crate::{Fixed, ONE, SCALE, pow};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuoteError {
    EmptyReserve,
    InvalidWeights,
    InvalidAmount,
    AmountOutOfBounds,
    Arithmetic,
    PowerUnderflow,
    Dust,
    Accuracy,
    UnqualifiedScale,
}

impl core::fmt::Display for QuoteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl core::error::Error for QuoteError {}

// These are error-check units only. The pricing kernel keeps scales 52 and 62.
const S: u128 = 1 << 96;
const D: u128 = 1 << 62;
const DX: u128 = (1 << 44) + (1 << 24); // (2^-52 + 2^-72) * S
const DB: u128 = 1 << 44; // 2^-52 * S
const K: u128 = 912 << 34; // (912 / 2^62) * S

fn validate(
    balance_in: u128,
    weight_in: u128,
    balance_out: u128,
    weight_out: u128,
) -> Result<(), QuoteError> {
    if SCALE != 52 {
        return Err(QuoteError::UnqualifiedScale);
    }
    if balance_in == 0 || balance_out == 0 {
        return Err(QuoteError::EmptyReserve);
    }
    if weight_in == 0 || weight_out == 0 || weight_in > 1 << 64 || weight_out > 1 << 64 {
        return Err(QuoteError::InvalidWeights);
    }
    // Both products fit after the individual weight checks.
    if weight_in > 99 * weight_out || weight_out > 99 * weight_in {
        return Err(QuoteError::InvalidWeights);
    }
    Ok(())
}

/// Exact-input quote. Reject invalid, unrepresentable, dust or uncertified trades.
/// Accepted amounts match the raw kernel and add at most one basis point of loss.
pub fn try_calc_out_given_in(
    balance_in: u128,
    weight_in: u128,
    balance_out: u128,
    weight_out: u128,
    amount_in: u128,
) -> Result<u128, QuoteError> {
    validate(balance_in, weight_in, balance_out, weight_out)?;
    if amount_in == 0 {
        return Err(QuoteError::InvalidAmount);
    }
    let total = balance_in
        .checked_add(amount_in)
        .ok_or(QuoteError::Arithmetic)?;
    // All raw-kernel preconditions are now established. Its payout cannot exceed
    // balance_out, so the final widened multiply also fits.
    let base = ratio_up(balance_in, total);
    if base.0 >= ONE.0 {
        return Err(QuoteError::Dust);
    }
    let exponent = Fixed(((weight_in << SCALE) / weight_out) as i128);
    let complement = pow::one_minus_pow_62(base, exponent) as u128;
    let amount_out = mul_div(balance_out, complement, D, false)?;
    if amount_out == 0 {
        return Err(QuoteError::Dust);
    }

    let loss = if weight_in == weight_out {
        let ideal = mul_div(balance_out, amount_in, total, false)?;
        ideal
            .checked_sub(amount_out)
            .ok_or(QuoteError::Arithmetic)?
    } else {
        let x_lo = mul_div(balance_in, S, total, false)?;
        let b_hi = mul_div(weight_in, S, weight_out, true)?;
        let e = formation_error(b_hi, x_lo)?;
        // ideal complement <= padded complement + E_out. Taking the floor
        // before comparing also certifies coarse-unit trades with no extra loss.
        let upper = (complement << 34)
            .checked_add(e)
            .ok_or(QuoteError::Accuracy)?
            .min(S);
        let ideal_upper = mul_div(balance_out, upper, S, false)?;
        ideal_upper
            .checked_sub(amount_out)
            .ok_or(QuoteError::Arithmetic)?
    };
    if loss > amount_out / 10_000 {
        return Err(QuoteError::Accuracy);
    }
    Ok(amount_out)
}

/// Exact-output quote. The 30% output cap is a domain check, not an accuracy
/// guarantee. Reject an unrepresentable payment or post-buy pricing reserve.
pub fn try_calc_in_given_out(
    balance_in: u128,
    weight_in: u128,
    balance_out: u128,
    weight_out: u128,
    amount_out: u128,
) -> Result<u128, QuoteError> {
    validate(balance_in, weight_in, balance_out, weight_out)?;
    if amount_out == 0 {
        return Err(QuoteError::InvalidAmount);
    }
    let cap = balance_out / 10 * 3 + balance_out % 10 * 3 / 10;
    if amount_out > cap {
        return Err(QuoteError::AmountOutOfBounds);
    }

    let base = ratio_down(balance_out - amount_out, balance_out);
    let exponent = Fixed(((weight_out << SCALE).div_ceil(weight_in)) as i128);
    // Reserve, cap and weight checks establish the power kernel's domain.
    let power = pow::pow_62_down(base, exponent);
    let amount_in = payment_from_power(balance_in, power)?;
    balance_in
        .checked_add(amount_in)
        .ok_or(QuoteError::Arithmetic)?;

    let loss = if weight_in == weight_out {
        let ideal = mul_div(balance_in, amount_out, balance_out - amount_out, true)?;
        amount_in.checked_sub(ideal).ok_or(QuoteError::Arithmetic)?
    } else {
        payment_loss_bound(
            balance_in,
            weight_in,
            balance_out,
            weight_out,
            amount_out,
            power as u128,
            amount_in,
        )?
    };
    let lower = amount_in.checked_sub(loss).ok_or(QuoteError::Accuracy)?;
    if lower == 0 || loss > lower / 10_000 {
        return Err(QuoteError::Accuracy);
    }
    Ok(amount_in)
}

fn payment_from_power(balance_in: u128, power: i128) -> Result<u128, QuoteError> {
    if power <= 0 {
        return Err(QuoteError::PowerUnderflow);
    }
    let q = power as u128;
    if q > D {
        return Err(QuoteError::Arithmetic);
    }
    let ratio = ((D - q) * D).div_ceil(q);
    mul_div(balance_in, ratio, D, true)
}

/// Upper bound for (b*delta_x + (1-x)*delta_b)/x + K, in 2^-96 units.
/// b is an upper bound and x is a positive lower bound. Round each step up.
fn formation_error(b_hi: u128, x_lo: u128) -> Result<u128, QuoteError> {
    if x_lo == 0 || x_lo > S {
        return Err(QuoteError::Accuracy);
    }
    let first = mul_div(b_hi, DX, S, true)?;
    let second = mul_div(S - x_lo, DB, S, true)?;
    let numerator = first.checked_add(second).ok_or(QuoteError::Accuracy)?;
    mul_div(numerator, S, x_lo, true)
        .map_err(|_| QuoteError::Accuracy)?
        .checked_add(K)
        .ok_or(QuoteError::Accuracy)
}

fn payment_loss_bound(
    bi: u128,
    wi: u128,
    bo: u128,
    wo: u128,
    ao: u128,
    q62: u128,
    payment: u128,
) -> Result<u128, QuoteError> {
    let x_lo = mul_div(bo - ao, S, bo, false)?;
    let ell = x_lo
        .checked_sub(DX)
        .filter(|v| *v > 0)
        .ok_or(QuoteError::Accuracy)?;
    let b_hi = mul_div(wo, S, wi, true)?
        .checked_add(DB)
        .ok_or(QuoteError::Accuracy)?
        .min(99 * S);
    let e = formation_error(b_hi, ell)?;
    let q = q62 << 34;
    let upper = q.checked_add(e).ok_or(QuoteError::Accuracy)?.min(S);
    // q <= ideal power <= min(1, q+E_in). This gives a lower bound on
    // the ideal ceiled payment. Comparing it with the actual payment also
    // includes the raw kernel's inner ratio ceiling and final payment ceiling.
    let ideal_lower = mul_div(bi, S - upper, upper, true)?;
    payment
        .checked_sub(ideal_lower)
        .ok_or(QuoteError::Arithmetic)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_or_negative_power_is_a_defined_error() {
        for power in [i128::MIN, -1, 0] {
            assert_eq!(
                payment_from_power(100, power),
                Err(QuoteError::PowerUnderflow)
            );
        }
    }
}
