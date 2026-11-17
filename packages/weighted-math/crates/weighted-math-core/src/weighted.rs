//! Weighted-pool swap math (Balancer-style) built on [`crate::pow`].
//!
//! Balances, amounts, and weights are raw `u128` — the LEZ native token
//! unit. `Fixed` is used only for the base ratio and the weight-ratio
//! exponent (ADR 0010).
//!
//! Every rounding favours the pool: base up, exponent down, `1 - power`
//! padded down, final payout multiply floored — the payout never exceeds
//! the true value. `calc_in_given_out` is inverted into the same
//! `base ∈ (0,1)` domain (ADR 0013).
//!
//! Envelope (asserted; violations panic, never wrap): reserves >= 1 wei,
//! total deposit < 2^128, weights in `[1, 2^64]` with ratio in
//! `[1/99, 99]`. The per-step overflow ledger is `docs/overflow-proof.md`.

use crate::fixed::{Fixed, ONE, SCALE};
use crate::{pow, wide};

// The 1/99 weight ratio must stay a nonzero exponent: 2^SCALE >= 99.
#[allow(clippy::assertions_on_constants)]
const _: () = assert!(
    SCALE >= 7,
    "SCALE must be >= 7 so the 1/99 exponent stays nonzero"
);

/// Amount of `token_out` received for a given `amount_in` of `token_in`.
///
/// `out = balance_out * (1 - (balance_in / (balance_in + amount_in))^(weight_in / weight_out))`
///
/// All quantities are raw `u128` wei / raw weight units. The result is
/// rounded down (pool-favouring); a trade too small to move the quantized
/// ratio pays zero.
///
/// ```
/// use weighted_math_core::calc_out_given_in;
///
/// // Sell 1000 wei of token A into a 99/1 pool.
/// let out = calc_out_given_in(
///     1_000_000_000, // balance_in  (raw u128 wei)
///     99,            // weight_in
///     500_000_000,   // balance_out
///     1,             // weight_out
///     1_000,         // amount_in
/// );
/// assert!(out > 0 && out < 500_000_000);
/// ```
///
/// # Panics
///
/// On envelope violations: empty reserve, total deposit past `u128`, or
/// weights outside `[1, 2^64]` / ratio outside `[1/99, 99]`.
pub fn calc_out_given_in(
    balance_in: u128,
    weight_in: u128,
    balance_out: u128,
    weight_out: u128,
    amount_in: u128,
) -> u128 {
    assert!(balance_in >= 1 && balance_out >= 1, "empty reserve");
    let total = balance_in
        .checked_add(amount_in)
        .expect("total deposit must stay below 2^128");
    check_weights(weight_in, weight_out);

    // Exponent rounds down, base rounds up: both overstate the power.
    let exponent = Fixed(((weight_in << SCALE) / weight_out) as i128);
    let base = ratio_up(balance_in, total);
    if base.0 >= ONE.0 {
        return 0;
    }

    // 1 - power at the internal 62-bit scale, rounded down (see pow.rs).
    let omp62 = pow::one_minus_pow_62(base, exponent);
    debug_assert!((0..=ONE_62).contains(&omp62));

    // tokens_out = floor(balance_out * omp62 / 2^62): the widened multiply.
    wide::mul_shr(balance_out, omp62 as u128, pow::LN_SCALE)
}

/// Amount of `token_in` required to receive a given `amount_out` of
/// `token_out`.
///
/// `in = balance_in * ((balance_out / (balance_out - amount_out))^(weight_out / weight_in) - 1)`
///
/// Rounded up: the pool never undercharges. The base-above-one form is
/// inverted into the kernel's `(0,1)` domain via
/// `(b/(b-a))^y - 1 = (1-p)/p`, flipping every rounding direction once
/// (ADR 0013).
///
/// ```
/// use weighted_math_core::calc_in_given_out;
///
/// // Price an exact-out purchase of 1000 wei of token B.
/// let amount_in = calc_in_given_out(
///     1_000_000_000, // balance_in
///     99,            // weight_in
///     500_000_000,   // balance_out
///     1,             // weight_out
///     1_000,         // amount_out
/// );
/// assert!(amount_in > 0);
/// ```
///
/// # Panics
///
/// As [`calc_out_given_in`], plus: `amount_out` above 30% of the reserve,
/// or a payment past `u128` (ADR 0013).
pub fn calc_in_given_out(
    balance_in: u128,
    weight_in: u128,
    balance_out: u128,
    weight_out: u128,
    amount_out: u128,
) -> u128 {
    assert!(balance_in >= 1 && balance_out >= 1, "empty reserve");
    check_weights(weight_in, weight_out);
    let cap = max_exact_output(balance_out);
    assert!(
        amount_out <= cap,
        "amount_out envelope: <= 30% of the reserve"
    );
    if amount_out == 0 {
        return 0;
    }

    // Exponent rounds up, base' rounds down: both understate p.
    let exponent = Fixed(((weight_out << SCALE).div_ceil(weight_in)) as i128);
    let base = ratio_down(balance_out - amount_out, balance_out);
    let p62 = pow::pow_62_down(base, exponent);
    // Positive power is required for inversion; the cap is not an accuracy bound.
    assert!(p62 > 0, "power underflows the internal scale");
    debug_assert!(p62 <= ONE_62);

    // (1 - p)/p at LN_SCALE, rounded up: numerator <= 2^124 fits u128.
    let num = ((ONE_62 - p62) as u128) << pow::LN_SCALE;
    let r62 = num.div_ceil(p62 as u128);
    debug_assert!(r62 <= 1 << 124);

    // ceil(balance_in · r62 / 2^62); the fit assert is the payment envelope.
    wide::mul_shr_up(balance_in, r62, pow::LN_SCALE)
}

pub(crate) const fn max_exact_output(balance_out: u128) -> u128 {
    // floor(0.3 * balance_out) without overflowing 3 * balance_out.
    balance_out / 10 * 3 + balance_out % 10 * 3 / 10
}

const ONE_62: i128 = 1 << pow::LN_SCALE;

fn check_weights(weight_in: u128, weight_out: u128) {
    assert!(weight_in >= 1 && weight_out >= 1, "zero weight");
    assert!(
        weight_in <= 1 << 64 && weight_out <= 1 << 64,
        "weight envelope: <= 2^64"
    );
    assert!(
        weight_in <= 99 * weight_out && weight_out <= 99 * weight_in,
        "weight ratio envelope: within [1/99, 99]"
    );
}

/// `num / den` as a `Fixed`, rounded up, for `num <= den` — one division.
///
/// Wide operands are pre-shifted with upward bias. At SCALE 52, the added
/// absolute ratio error is below 2^-72 before the final division rounding.
pub(crate) fn ratio_up(num: u128, den: u128) -> Fixed {
    debug_assert!(num <= den && den > 0);
    let den_bits = 128 - den.leading_zeros();
    let excess = den_bits.saturating_sub(126 - SCALE);
    let (n, d) = if excess > 0 {
        ((num >> excess) + 1, den >> excess)
    } else {
        (num, den)
    };
    // n <= 2^(126-SCALE), so n << SCALE <= 2^126 and stays in u128.
    let q = (n << SCALE).div_ceil(d);
    debug_assert!(q >> 127 == 0, "ratio fits the signed representation");
    Fixed(q as i128)
}

/// `num / den` as a `Fixed`, rounded down, for `num < den` — the mirror
/// of [`ratio_up`].
pub(crate) fn ratio_down(num: u128, den: u128) -> Fixed {
    debug_assert!(num < den);
    let den_bits = 128 - den.leading_zeros();
    let excess = den_bits.saturating_sub(126 - SCALE);
    let (n, d) = if excess > 0 {
        (num >> excess, (den >> excess) + 1)
    } else {
        (num, den)
    };
    let q = (n << SCALE) / d;
    debug_assert!(q < ONE.0 as u128, "num < den keeps the ratio below one");
    Fixed(q as i128)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[test]
    fn exact_output_cap_matches_bounded_direct_oracle() {
        const ZERO_CAP: u128 = max_exact_output(0);
        assert_eq!(ZERO_CAP, 0);
        for balance in 0..=100_000 {
            assert_eq!(max_exact_output(balance), balance * 3 / 10);
        }
        let residues = [0, 0, 0, 0, 1, 1, 1, 2, 2, 2];
        for (residue, extra) in residues.into_iter().enumerate() {
            assert_eq!(max_exact_output(100 + residue as u128), 30 + extra);
        }
        // MAX = 10 * 34028236692093846346337460743176821145 + 5.
        assert_eq!(
            max_exact_output(u128::MAX),
            102_084_710_076_281_539_039_012_382_229_530_463_436
        );
    }

    #[test]
    fn exact_output_above_cap_keeps_raw_panic() {
        for balance in (1..=100).chain([u128::MAX]) {
            let panic = std::panic::catch_unwind(|| {
                calc_in_given_out(1, 1, balance, 1, max_exact_output(balance) + 1)
            })
            .unwrap_err();
            assert_eq!(
                panic.downcast_ref::<&str>(),
                Some(&"amount_out envelope: <= 30% of the reserve")
            );
        }
    }

    #[test]
    #[should_panic(expected = "empty reserve")]
    fn empty_reserve_precedes_raw_cap_check() {
        calc_in_given_out(0, 0, 1, 0, 1);
    }

    #[test]
    #[should_panic(expected = "zero weight")]
    fn invalid_weight_precedes_raw_cap_check() {
        calc_in_given_out(1, 0, 1, 0, 1);
    }
}
