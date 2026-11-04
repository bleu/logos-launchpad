use harness::{InGivenOutCase, OutGivenInCase, load_fixture, parse_u128};
use num_bigint::BigUint;
use proptest::prelude::*;
use weighted_math_core::{
    QuoteError, calc_in_given_out, calc_out_given_in, try_calc_in_given_out, try_calc_out_given_in,
};

fn big(value: &str) -> BigUint {
    BigUint::parse_bytes(value.as_bytes(), 10).unwrap()
}

#[test]
fn checked_quotes_meet_the_independent_oracle_target() {
    let mut accepted = [0usize; 2];
    let mut rejected = [0usize; 2];
    for c in load_fixture::<Vec<OutGivenInCase>>("out_given_in.json") {
        let (bi, wi, bo, wo, a) = (
            parse_u128(&c.balance_in),
            parse_u128(&c.weight_in),
            parse_u128(&c.balance_out),
            parse_u128(&c.weight_out),
            parse_u128(&c.amount_in),
        );
        match try_calc_out_given_in(bi, wi, bo, wo, a) {
            Ok(actual) => {
                let ideal = big(&c.tokens_out_floor);
                let value = BigUint::from(actual);
                assert!(value <= ideal, "wrong payout direction: {c:?}");
                assert!(
                    (&ideal - &value) * 10_000u32 <= ideal,
                    "payout accuracy: {c:?}"
                );
                assert_eq!(actual, calc_out_given_in(bi, wi, bo, wo, a));
                assert!(actual > 0 && actual <= bo);
                assert!(bi.checked_add(a).is_some());
                accepted[0] += 1;
            }
            Err(error) => {
                assert!(
                    matches!(
                        error,
                        QuoteError::Dust | QuoteError::Accuracy | QuoteError::Arithmetic
                    ) || (a == 0 && error == QuoteError::InvalidAmount),
                    "{error:?}: {c:?}"
                );
                rejected[0] += 1;
            }
        }
    }
    for c in load_fixture::<Vec<InGivenOutCase>>("in_given_out.json") {
        let (bi, wi, bo, wo, a) = (
            parse_u128(&c.balance_in),
            parse_u128(&c.weight_in),
            parse_u128(&c.balance_out),
            parse_u128(&c.weight_out),
            parse_u128(&c.amount_out),
        );
        match try_calc_in_given_out(bi, wi, bo, wo, a) {
            Ok(actual) => {
                let ideal = big(&c.amount_in_ceil);
                let value = BigUint::from(actual);
                assert!(value >= ideal, "wrong payment direction: {c:?}");
                assert!(
                    (&value - &ideal) * 10_000u32 <= ideal,
                    "payment accuracy: {c:?}"
                );
                assert_eq!(actual, calc_in_given_out(bi, wi, bo, wo, a));
                assert!(bi.checked_add(actual).is_some());
                accepted[1] += 1;
            }
            Err(error) => {
                assert!(
                    matches!(error, QuoteError::Accuracy | QuoteError::Arithmetic)
                        || (a == 0 && error == QuoteError::InvalidAmount),
                    "{error:?}: {c:?}"
                );
                rejected[1] += 1;
            }
        }
    }
    for i in 0..2 {
        assert!(accepted[i] > 0 && rejected[i] > 0);
    }
    println!("checked quote fixture counts: accepted={accepted:?}, rejected={rejected:?}");
}

#[test]
fn invalid_inputs_and_unrepresentable_results_return_defined_errors() {
    for (bi, wi, bo, wo, error) in [
        (0, 1, 100, 1, QuoteError::EmptyReserve),
        (100, 1, 0, 1, QuoteError::EmptyReserve),
        (100, 0, 100, 1, QuoteError::InvalidWeights),
        (100, 1, 100, 0, QuoteError::InvalidWeights),
        (100, u128::MAX, 100, u128::MAX, QuoteError::InvalidWeights),
        (100, (1 << 64) + 1, 100, 1 << 64, QuoteError::InvalidWeights),
        (100, 100, 100, 1, QuoteError::InvalidWeights),
        (100, 1, 100, 100, QuoteError::InvalidWeights),
    ] {
        assert_eq!(try_calc_out_given_in(bi, wi, bo, wo, 1), Err(error));
        assert_eq!(try_calc_in_given_out(bi, wi, bo, wo, 1), Err(error));
    }
    assert_eq!(
        try_calc_out_given_in(100, 1, 100, 1, 0),
        Err(QuoteError::InvalidAmount)
    );
    assert_eq!(
        try_calc_in_given_out(100, 1, 100, 1, 0),
        Err(QuoteError::InvalidAmount)
    );
    assert_eq!(
        try_calc_in_given_out(100, 1, 100, 1, 31),
        Err(QuoteError::AmountOutOfBounds)
    );
    assert_eq!(try_calc_in_given_out(100, 1, 100, 1, 30), Ok(43));
    assert_eq!(
        try_calc_out_given_in(u128::MAX, 1, 100, 1, 1),
        Err(QuoteError::Arithmetic)
    );
    assert_eq!(
        try_calc_out_given_in(u128::MAX - 1, 1, 100, 1, 1),
        Err(QuoteError::Dust)
    );
    let b = 1u128 << 100;
    assert_eq!(
        try_calc_in_given_out(b, 1, b, 99, b / 10 * 3),
        Err(QuoteError::Arithmetic)
    );
    assert_eq!(
        try_calc_in_given_out(
            326_937_960_375_019_080_809_068_952_671_530_218_863,
            1,
            10,
            2,
            3
        ),
        Err(QuoteError::Arithmetic)
    );
    // Payment itself fits, but adding it to the pricing reserve does not.
    assert_eq!(
        try_calc_in_given_out(u128::MAX, 1, 100, 1, 1),
        Err(QuoteError::Arithmetic)
    );
    // The 30% cap does not certify the sensitive exponent-99 corner.
    assert_eq!(
        try_calc_in_given_out(100, 1, 1_000_000, 99, 300_000),
        Err(QuoteError::Accuracy)
    );
}

#[test]
fn one_basis_point_boundary_uses_the_rounded_ideal_trade() {
    // Equal-weight references are exact. The pad costs one extra raw unit.
    assert_eq!(
        try_calc_out_given_in(10_001, 1, 20_002, 1, 10_001),
        Ok(10_000)
    );
    assert_eq!(
        try_calc_out_given_in(10_000, 1, 20_000, 1, 10_000),
        Err(QuoteError::Accuracy)
    );
    assert_eq!(
        try_calc_in_given_out(30_000, 1, 40_000, 1, 10_000),
        Ok(10_001)
    );
    assert_eq!(
        try_calc_in_given_out(29_997, 1, 40_000, 1, 10_000),
        Err(QuoteError::Accuracy)
    );
}

proptest! {
    #[test]
    fn accepted_unequal_weight_quotes_meet_the_target_with_an_integer_power_referee(
        bi in 1000u128..=u128::MAX, bo in 1000u128..=u128::MAX,
        wi in 1u32..=99, in_fraction in 1u128..=1000, out_fraction in 1u128..=300,
    ) {
        let wo = 100 - wi;
        let input = bi / 1000 * in_fraction;
        let output = bo / 1000 * out_fraction;
        let b = BigUint::from(bi);
        let r = BigUint::from(bo);
        if let Ok(actual) = try_calc_out_given_in(bi, wi.into(), bo, wo.into(), input) {
            prop_assert_eq!(actual, calc_out_given_in(bi, wi.into(), bo, wo.into(), input));
            // floor(ideal) <= floor(actual*10000/9999) proves the 1 bp target.
            // Compare the continuous ideal with allowed+1 using integer powers.
            let allowed = BigUint::from(actual) * 10_000u32 / 9999u32;
            if allowed < r {
                let remaining = &r - allowed - 1u32;
                prop_assert!(b.pow(wi) * r.pow(wo) > (&b + input).pow(wi) * remaining.pow(wo));
            }
        }
        if let Ok(actual) = try_calc_in_given_out(bi, wi.into(), bo, wo.into(), output) {
            prop_assert_eq!(actual, calc_in_given_out(bi, wi.into(), bo, wo.into(), output));
            // ceil(ideal) >= ceil(actual*10000/10001) proves the 1 bp target.
            let lower = (BigUint::from(actual) * 10_000u32 + 10_000u32) / 10_001u32;
            let threshold = &b + lower - 1u32;
            prop_assert!(r.pow(wo) * b.pow(wi) > (&r - output).pow(wo) * threshold.pow(wi));
        }
    }

    #[test]
    fn arbitrary_inputs_never_panic(bi in any::<u128>(), wi in any::<u128>(), bo in any::<u128>(), wo in any::<u128>(), amount in any::<u128>()) {
        let _ = try_calc_out_given_in(bi, wi, bo, wo, amount);
        let _ = try_calc_in_given_out(bi, wi, bo, wo, amount);
    }

    #[test]
    fn qualified_weights_and_full_width_balances_never_panic(
        bi in 1u128..=u128::MAX, bo in 1u128..=u128::MAX,
        w in 1u128..=99, input in any::<u128>(), fraction in 1u128..=300,
    ) {
        let _ = try_calc_out_given_in(bi, w, bo, 100-w, input);
        let output = bo / 1000 * fraction;
        if let Ok(payment) = try_calc_in_given_out(bi, w, bo, 100-w, output) {
            prop_assert!(bi.checked_add(payment).is_some());
            prop_assert_eq!(payment, calc_in_given_out(bi, w, bo, 100-w, output));
        }
    }

    #[test]
    fn equal_weight_full_width_quotes_match_big_integer_references(
        bi in 1u128..=u128::MAX, bo in 1u128..=u128::MAX, amount in 1u128..=u128::MAX,
    ) {
        let b = BigUint::from(bi);
        let r = BigUint::from(bo);
        let a = BigUint::from(amount);
        if let Ok(actual) = try_calc_out_given_in(bi, 1, bo, 1, amount) {
            let ideal = &r * &a / (&b + &a);
            prop_assert!(BigUint::from(actual) <= ideal);
            prop_assert!((ideal - actual) * 10_000u32 <= BigUint::from(actual));
        }
        let output = bo / 10;
        if let Ok(actual) = try_calc_in_given_out(bi, 1, bo, 1, output) {
            let denominator = &r - output;
            let ideal = (&b * output + &denominator - 1u32) / denominator;
            prop_assert!(BigUint::from(actual) >= ideal);
            prop_assert!((BigUint::from(actual) - &ideal) * 10_000u32 <= ideal);
        }
    }
}
