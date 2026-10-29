//! Bounded wide arithmetic for quote checks, outside the raw pricing kernel.

use crate::{QuoteError, wide};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Uint {
    hi: u128,
    lo: u128,
}

impl From<u128> for Uint {
    fn from(lo: u128) -> Self {
        Self { hi: 0, lo }
    }
}

impl Uint {
    pub(crate) fn product(a: u128, b: u128) -> Self {
        let (hi, lo) = wide::mul_wide(a, b);
        Self { hi, lo }
    }

    fn bits(self) -> u32 {
        if self.hi == 0 {
            128 - self.lo.leading_zeros()
        } else {
            256 - self.hi.leading_zeros()
        }
    }

    // The caller keeps the shifted divisor within the numerator's bit length.
    fn shl(self, shift: u32) -> Self {
        if shift == 0 {
            self
        } else if shift < 128 {
            Self {
                hi: (self.hi << shift) | (self.lo >> (128 - shift)),
                lo: self.lo << shift,
            }
        } else {
            Self {
                hi: self.lo << (shift - 128),
                lo: 0,
            }
        }
    }

    // Only called when self >= rhs.
    fn sub(self, rhs: Self) -> Self {
        let (lo, borrow) = self.lo.overflowing_sub(rhs.lo);
        Self {
            hi: self
                .hi
                .wrapping_sub(rhs.hi)
                .wrapping_sub(u128::from(borrow)),
            lo,
        }
    }

    /// Divide a bounded wide numerator by a positive wide denominator.
    /// Reject both a floor and a rounded-up result that cannot fit u128.
    pub(crate) fn div(self, denominator: Self, ceil: bool) -> Result<u128, QuoteError> {
        if denominator == Self::default() {
            return Err(QuoteError::Arithmetic);
        }
        // Accuracy and kernel scales are powers of two. Keep these divisions
        // as checked shifts rather than running the general division loop.
        if denominator.hi == 0 && denominator.lo.is_power_of_two() {
            let shift = denominator.lo.trailing_zeros();
            let floor = if shift == 0 {
                if self.hi != 0 {
                    return Err(QuoteError::Arithmetic);
                }
                self.lo
            } else {
                if self.hi >> shift != 0 {
                    return Err(QuoteError::Arithmetic);
                }
                (self.hi << (128 - shift)) | (self.lo >> shift)
            };
            let remainder = self.lo & (denominator.lo - 1);
            return floor
                .checked_add(u128::from(ceil && remainder != 0))
                .ok_or(QuoteError::Arithmetic);
        }
        let mut remainder = self;
        let mut quotient = 0u128;
        let highest = self.bits().saturating_sub(denominator.bits());
        for shift in (0..=highest).rev() {
            let divisor = denominator.shl(shift);
            if remainder >= divisor {
                if shift >= 128 {
                    return Err(QuoteError::Arithmetic);
                }
                remainder = remainder.sub(divisor);
                quotient |= 1u128 << shift;
            }
        }
        quotient
            .checked_add(u128::from(ceil && remainder != Self::default()))
            .ok_or(QuoteError::Arithmetic)
    }
}

pub(crate) fn mul_div(a: u128, b: u128, denominator: u128, ceil: bool) -> Result<u128, QuoteError> {
    Uint::product(a, b).div(denominator.into(), ceil)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_division_checks_full_width_results_and_rounding() {
        let max = u128::MAX;
        assert_eq!(mul_div(max, max, max, false), Ok(max));
        assert_eq!(
            mul_div(max, max, max - 1, false),
            Err(QuoteError::Arithmetic)
        );
        // floor((MAX-1)^2/(MAX-2)) is MAX; its ceiling cannot fit.
        assert_eq!(mul_div(max - 1, max - 1, max - 2, false), Ok(max));
        assert_eq!(
            mul_div(max - 1, max - 1, max - 2, true),
            Err(QuoteError::Arithmetic)
        );
        let wide = Uint::product(max, max);
        assert_eq!(wide.div(wide, true), Ok(1));
        assert_eq!(Uint::from(1).div(wide, false), Ok(0));
        assert_eq!(Uint::from(1).div(wide, true), Ok(1));
        assert_eq!(mul_div(1, 1, 0, true), Err(QuoteError::Arithmetic));
        for shift in 0..128 {
            assert_eq!(mul_div(max, 1 << shift, 1 << shift, true), Ok(max));
        }
        assert_eq!(
            mul_div((1 << 90) - 1, (1 << 90) + 1, 1 << 52, false),
            Ok(max)
        );
        assert_eq!(
            mul_div((1 << 90) - 1, (1 << 90) + 1, 1 << 52, true),
            Err(QuoteError::Arithmetic)
        );
    }

    #[test]
    fn bounded_wide_operations_match_native_arithmetic() {
        let mut seed = 0x7394_4261_58ab_cdefu128;
        for _ in 0..1000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let a = seed & u128::from(u64::MAX);
            let b = seed >> 64;
            let d = (a ^ b).max(1);
            assert_eq!(mul_div(a, b, d, false), Ok(a * b / d));
            assert_eq!(mul_div(a, b, d, true), Ok((a * b).div_ceil(d)));
        }
    }
}
