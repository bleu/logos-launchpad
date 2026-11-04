//! Linear weight interpolation over time for LBP weight schedules.
//!
//! Weights are raw `u128` ratios in the same unit as [`crate::calc_out_given_in`]
//! inputs (e.g. `95` and `5` for a 95/5 pool). The complement weight is always
//! computed as `total - interpolated`, so the pair stays in sync with no drift.
//!
//! Domain requirements (asserted; violations panic):
//! - `end_time > start_time` (validated upstream in `PrepareWeighted`)
//! - each weight is at most `u64::MAX`; the weight delta times elapsed time then fits in `u128`.
//!   Weighted sales validate endpoints against their normalized total of `10^18`.

/// Returns the linearly interpolated weight at `now`, clamped to
/// `[start_time, end_time]`.
///
/// The interpolation is:
/// ```text
/// weight(t) = start + (end - start) * (t - start_time) / (end_time - start_time)
/// ```
/// rounded down. For a decrease, subtract the change rounded up.
///
/// # Panics
///
/// Panics if `end_time <= start_time` (zero or negative duration).
/// The caller must bound weights to `u64::MAX` to prevent product overflow.
pub fn interpolate_weight(
    start_weight: u128,
    end_weight: u128,
    start_time: u64,
    end_time: u64,
    now: u64,
) -> u128 {
    assert!(end_time > start_time, "end_time must be > start_time");

    if now <= start_time {
        return start_weight;
    }
    if now >= end_time {
        return end_weight;
    }

    let elapsed = u128::from(now - start_time);
    let duration = u128::from(end_time - start_time);

    if start_weight <= end_weight {
        // weight is increasing: floor keeps us from overshooting
        let delta = (end_weight - start_weight) * elapsed / duration;
        start_weight + delta
    } else {
        // weight is decreasing: ceiling keeps us from undershooting
        let numerator = (start_weight - end_weight) * elapsed;
        let delta = numerator.div_ceil(duration);
        start_weight - delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn before_start_returns_start_weight() {
        assert_eq!(interpolate_weight(95, 50, 1000, 2000, 0), 95);
        assert_eq!(interpolate_weight(95, 50, 1000, 2000, 999), 95);
        assert_eq!(interpolate_weight(95, 50, 1000, 2000, 1000), 95);
    }

    #[test]
    fn after_end_returns_end_weight() {
        assert_eq!(interpolate_weight(95, 50, 1000, 2000, 2000), 50);
        assert_eq!(interpolate_weight(95, 50, 1000, 2000, 2001), 50);
        assert_eq!(interpolate_weight(95, 50, 1000, 2000, u64::MAX), 50);
    }

    #[test]
    fn at_midpoint_is_linear() {
        // 95 → 50 over [1000, 3000], midpoint t=2000
        // elapsed=1000, duration=2000
        // decreasing: delta = ceil((95-50)*1000/2000) = ceil(22.5) = 23 → 95-23 = 72
        let mid = interpolate_weight(95, 50, 1000, 3000, 2000);
        assert_eq!(mid, 72);

        // 5 → 50 over [1000, 3000], midpoint t=2000 (increasing)
        // delta = floor((50-5)*1000/2000) = floor(22.5) = 22 → 5+22 = 27
        let mid_inc = interpolate_weight(5, 50, 1000, 3000, 2000);
        assert_eq!(mid_inc, 27);
    }

    #[test]
    fn complement_sums_to_total_when_derived_not_interpolated() {
        // Correct usage: interpolate only the token weight; derive collateral
        // weight as (total - token_weight). Do NOT interpolate both independently.
        let total = 100u128;
        for now in [1000u64, 1500, 2000, 2500, 3000] {
            let token_w = interpolate_weight(95, 50, 1000, 3000, now);
            let coll_w = total - token_w; // derived, not interpolated separately
            assert_eq!(token_w + coll_w, total);
        }
    }

    #[test]
    fn constant_weight_unchanged_throughout() {
        for now in [0u64, 500, 1000, 1500, 2000] {
            assert_eq!(interpolate_weight(50, 50, 500, 2000, now), 50);
        }
    }

    #[test]
    fn decreasing_weight_never_goes_below_end() {
        for now in 0u64..=3000 {
            let w = interpolate_weight(95, 50, 1000, 3000, now);
            assert!((50..=95).contains(&w), "weight {w} out of range at t={now}");
        }
    }

    #[test]
    fn increasing_weight_never_exceeds_end() {
        for now in 0u64..=3000 {
            let w = interpolate_weight(5, 50, 1000, 3000, now);
            assert!((5..=50).contains(&w), "weight {w} out of range at t={now}");
        }
    }

    #[test]
    #[should_panic(expected = "end_time must be > start_time")]
    fn panics_on_zero_duration() {
        interpolate_weight(95, 50, 1000, 1000, 1000);
    }
}
