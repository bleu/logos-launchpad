use integer_math::{ArithmeticError, Rounding, mul_div};

#[test]
fn rounding_matches_direct_products_for_bounded_inputs() {
    for value in 0u128..=32 {
        for multiplier in 0u128..=32 {
            for divisor in 1u128..=32 {
                let product = value * multiplier;
                assert_eq!(
                    mul_div(value, multiplier, divisor, Rounding::Floor),
                    Ok(product / divisor)
                );
                assert_eq!(
                    mul_div(value, multiplier, divisor, Rounding::Ceil),
                    Ok(product.div_ceil(divisor))
                );
            }
        }
    }
}

#[test]
fn zero_divisor_is_rejected_even_with_zero_factors() {
    for rounding in [Rounding::Floor, Rounding::Ceil] {
        for (value, multiplier) in [(0, 0), (0, u128::MAX), (u128::MAX, 0)] {
            assert_eq!(
                mul_div(value, multiplier, 0, rounding),
                Err(ArithmeticError::DivisionByZero)
            );
        }
    }
}

#[test]
fn full_width_values_do_not_require_a_full_width_product() {
    for rounding in [Rounding::Floor, Rounding::Ceil] {
        assert_eq!(mul_div(u128::MAX, 1, 1, rounding), Ok(u128::MAX));
        assert_eq!(
            mul_div(u128::MAX, u128::MAX, u128::MAX, rounding),
            Ok(u128::MAX)
        );
        assert_eq!(mul_div(u128::MAX, 0, 1, rounding), Ok(0));
        assert_eq!(mul_div(0, u128::MAX, 1, rounding), Ok(0));
    }
    assert_eq!(mul_div(u128::MAX, 1, 2, Rounding::Floor), Ok(u128::MAX / 2));
    assert_eq!(
        mul_div(u128::MAX, 1, 2, Rounding::Ceil),
        Ok(u128::MAX / 2 + 1)
    );
}

#[test]
fn intermediate_overflow_is_rejected_even_when_the_result_fits() {
    for rounding in [Rounding::Floor, Rounding::Ceil] {
        assert_eq!(
            mul_div(u128::MAX - 1, u128::MAX, u128::MAX, rounding),
            Err(ArithmeticError::Overflow)
        );
    }
}

#[test]
fn overflowing_products_sums_and_rounding_are_rejected() {
    for rounding in [Rounding::Floor, Rounding::Ceil] {
        assert_eq!(
            mul_div(u128::MAX, 2, 1, rounding),
            Err(ArithmeticError::Overflow)
        );
        assert_eq!(
            mul_div((u128::MAX / 3) * 2 + 1, 3, 2, rounding),
            Err(ArithmeticError::Overflow)
        );
    }

    // The exact result is MAX + 3/5, so only rounding up overflows.
    let value = u128::MAX - u128::MAX / 6;
    assert_eq!(mul_div(value, 6, 5, Rounding::Floor), Ok(u128::MAX));
    assert_eq!(
        mul_div(value, 6, 5, Rounding::Ceil),
        Err(ArithmeticError::Overflow)
    );
}
