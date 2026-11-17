use integer_math::{ArithmeticError, div_ceil};

#[test]
fn ceiling_division_matches_bounded_direct_results() {
    for numerator in 0u128..=128 {
        for divisor in 1u128..=128 {
            assert_eq!(
                div_ceil(numerator, divisor),
                Ok(numerator.div_ceil(divisor))
            );
        }
    }
}

#[test]
fn ceiling_division_handles_full_width_values_without_adding_to_the_numerator() {
    assert_eq!(div_ceil(u128::MAX, 1), Ok(u128::MAX));
    assert_eq!(div_ceil(u128::MAX, 2), Ok(u128::MAX / 2 + 1));
    assert_eq!(div_ceil(u128::MAX, u128::MAX), Ok(1));
    assert_eq!(div_ceil(u128::MAX - 1, u128::MAX), Ok(1));
    assert_eq!(div_ceil(0, u128::MAX), Ok(0));
}

#[test]
fn ceiling_division_rejects_zero_divisors_including_zero_over_zero() {
    for numerator in [0, 1, u128::MAX] {
        assert_eq!(div_ceil(numerator, 0), Err(ArithmeticError::DivisionByZero));
    }
}
