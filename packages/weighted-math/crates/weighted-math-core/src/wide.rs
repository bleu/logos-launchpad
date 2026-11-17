//! Widening 128-bit multiply primitives.
//!
//! A full `128x128 -> 256` product is cheap on RV32IM (`u128` is already
//! multi-limb). The kernel never divides at 256 bits; these helpers only
//! multiply, add, and shift.

/// Full `128x128 -> 256`-bit product as `(hi, lo)`.
pub(crate) const fn mul_wide(a: u128, b: u128) -> (u128, u128) {
    const MASK: u128 = u64::MAX as u128;
    let (a_hi, a_lo) = (a >> 64, a & MASK);
    let (b_hi, b_lo) = (b >> 64, b & MASK);
    // Each partial product is at most (2^64 - 1)^2 < 2^128: no overflow.
    let ll = a_lo * b_lo;
    let lh = a_lo * b_hi;
    let hl = a_hi * b_lo;
    let hh = a_hi * b_hi;
    let (mid, carry_mid) = lh.overflowing_add(hl);
    let (lo, carry_lo) = ll.overflowing_add(mid << 64);
    let hi = hh + (mid >> 64) + ((carry_mid as u128) << 64) + carry_lo as u128;
    (hi, lo)
}

/// `floor(a * b / 2^shift)` for `0 < shift < 128`. Panics if the result
/// does not fit `u128`; each call site's fit argument is in
/// `docs/overflow-proof.md`.
pub(crate) fn mul_shr(a: u128, b: u128, shift: u32) -> u128 {
    shifted_product(a, b, shift).0
}

fn shifted_product(a: u128, b: u128, shift: u32) -> (u128, u128) {
    debug_assert!(shift > 0 && shift < 128);
    let (hi, lo) = mul_wide(a, b);
    assert!(hi >> shift == 0, "widened product exceeds u128 after shift");
    ((hi << (128 - shift)) | (lo >> shift), lo)
}

/// Ceiling variant of [`mul_shr`], same fit requirement.
pub(crate) fn mul_shr_up(a: u128, b: u128, shift: u32) -> u128 {
    let (floor, lo) = shifted_product(a, b, shift);
    floor
        .checked_add(u128::from(lo & ((1u128 << shift) - 1) != 0))
        .expect("widened product exceeds u128 after rounding")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_native_when_in_range() {
        assert_eq!(mul_wide(0, u128::MAX), (0, 0));
        assert_eq!(mul_wide(3, 5), (0, 15));
        let a = 0x1234_5678_9abc_def0_u128;
        let b = 0xfedc_ba98_7654_3210_u128;
        assert_eq!(mul_wide(a, b), (0, a * b));
    }

    #[test]
    fn full_width_corner() {
        // (2^128 - 1)^2 = 2^256 - 2^129 + 1
        assert_eq!(mul_wide(u128::MAX, u128::MAX), (u128::MAX - 1, 1));
    }

    #[test]
    fn shift_boundaries_preserve_floor_and_ceiling() {
        for shift in [1, 62, 127] {
            let unit = 1u128 << shift;
            for (a, b, floor, ceil) in [
                (0, u128::MAX, 0, 0),
                (3, unit, 3, 3),
                (1, unit + 1, 1, 2),
                (u128::MAX, unit, u128::MAX, u128::MAX),
                // MAX * (2^shift - 1) / 2^shift = MAX - 2^(128-shift) + 2^-shift.
                (
                    u128::MAX,
                    unit - 1,
                    u128::MAX - (1u128 << (128 - shift)),
                    u128::MAX - (1u128 << (128 - shift)) + 1,
                ),
            ] {
                assert_eq!(mul_shr(a, b, shift), floor);
                assert_eq!(mul_shr_up(a, b, shift), ceil);
            }
        }
    }

    #[test]
    #[should_panic(expected = "widened product exceeds u128 after shift")]
    fn floor_rejects_shift_fit_failure() {
        mul_shr(u128::MAX, u128::MAX, 127);
    }

    #[test]
    #[should_panic(expected = "widened product exceeds u128 after shift")]
    fn ceiling_rejects_shift_fit_failure() {
        mul_shr_up(u128::MAX, u128::MAX, 127);
    }

    #[test]
    fn floor_fits_when_only_ceiling_overflows() {
        assert_eq!(mul_shr((1 << 127) + 1, u128::MAX - 1, 127), u128::MAX);
    }

    #[test]
    #[should_panic(expected = "widened product exceeds u128 after rounding")]
    fn ceiling_rejects_rounding_overflow() {
        mul_shr_up((1 << 127) + 1, u128::MAX - 1, 127);
    }

    #[test]
    fn shifted_rounding() {
        // 7 * 5 / 8 = 4.375
        assert_eq!(mul_shr(7, 5, 3), 4);
        assert_eq!(mul_shr_up(7, 5, 3), 5);
        // exact multiples round identically
        assert_eq!(mul_shr(6, 4, 3), 3);
        assert_eq!(mul_shr_up(6, 4, 3), 3);
        // a genuinely 256-bit intermediate
        assert_eq!(mul_shr(u128::MAX, 1 << 62, 62), u128::MAX);
    }
}
