//! Differential tests against the mpmath oracle fixtures (ADR 0014):
//! kernel gates with signed one-sided error bands, plus grader
//! self-validation (Balancer's captured outputs, fixture sanity) proving
//! the machinery independently of the kernel.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use harness::*;
use num_bigint::BigUint;
use weighted_math_core::{
    calc_in_given_out, calc_out_given_in, exp, expm1, ln, pow, pow_down, pow_up,
};

fn assert_band(band: Band, what: &str, context: &dyn std::fmt::Debug) {
    match band {
        Band::Ok { .. } => {}
        Band::WrongSide { by_ulps } => panic!(
            "DIRECTION VIOLATION (fund-leak side): {what} off by {by_ulps} ulps in {context:?}"
        ),
        Band::TooFar {
            err_ulps,
            bound_ulps,
        } => panic!(
            "magnitude violation: {what} err {err_ulps} ulps > bound {bound_ulps} in {context:?}"
        ),
    }
}

// ---------------------------------------------------------------------------
// Grader self-validation: Balancer's captured outputs + fixture sanity
// ---------------------------------------------------------------------------

/// Grades Balancer's captured outputs against the mpmath truth within
/// Balancer's own accuracy (1e-14 relative + 2 wei). Proves the fixture
/// plumbing and band checking on a real implementation of this math.
#[test]
fn grader_validates_balancer_capture() {
    let inputs: Vec<BalancerInput> = load_fixture("balancer_inputs.json");
    let outputs: Vec<Option<String>> = load_fixture("balancer_pow.json");
    assert_eq!(
        inputs.len(),
        outputs.len(),
        "capture out of sync with inputs"
    );

    let mut graded = 0;
    for (input, output) in inputs.iter().zip(&outputs) {
        if input.skip {
            assert!(output.is_none(), "capture has a value for a skipped case");
            continue;
        }
        let actual = parse_u128(output.as_ref().expect("missing captured output"));
        let truth = parse_u128(input.expected18_floor.as_ref().unwrap());
        // Balancer's own error band, not the (stricter) kernel bound.
        let bound = truth / 100_000_000_000_000 + 2;
        let err = actual.abs_diff(truth);
        assert!(
            err <= bound,
            "Balancer capture outside its own band: x18={} y18={} actual={actual} \
             truth={truth} err={err} bound={bound}",
            input.x18,
            input.y18,
        );
        graded += 1;
    }
    assert!(graded > 60, "too few Balancer cases graded: {graded}");
}

/// The compiled-in SCALE must be one the oracle emitted a quantization floor
/// for, and the data must agree with the code's own ulp.
#[test]
fn grader_scale_is_covered_by_sweep_fixture() {
    let scales: Vec<ScaleCase> = load_fixture("scales.json");
    let entry = scales
        .iter()
        .find(|s| s.scale == SCALE)
        .unwrap_or_else(|| panic!("scales.json has no entry for SCALE={SCALE}"));
    assert_eq!(parse_u128(&entry.one), 1u128 << SCALE);
    assert_eq!(parse_u128(&entry.ulp_q128), 1u128 << (128 - SCALE));
    assert_eq!(ONE.0, (1i128) << SCALE);
}

/// Structural sanity of the primary fixture: danger zones present and
/// leading, truths in (0,1), inputs on the dyadic grid.
#[test]
fn grader_pow_fixture_sanity() {
    let cases: Vec<PowCase> = load_fixture("pow.json");
    assert!(cases.len() > 60);
    assert_eq!(
        cases[0].zone, "sale_start",
        "danger zone must lead the file"
    );
    assert!(cases.iter().any(|c| c.zone == "exp_high"));
    assert!(cases.iter().any(|c| c.zone == "small_base"));
    for c in &cases {
        let base = s40_to_fixed(&c.base_s40);
        assert!(base.0 > 0 && base.0 < ONE.0, "base out of (0,1): {c:?}");
        let expo = s40_to_fixed(&c.exponent_s40);
        assert!(
            expo.0 > 0 && expo.0 <= 99 * ONE.0,
            "exponent out of (0,99]: {c:?}"
        );
        let q = parse_u128(&c.pow_exact_q128);
        assert!(q < u128::MAX, "pow truth must be < 1");
    }
}

// ---------------------------------------------------------------------------
// Kernel gates
// ---------------------------------------------------------------------------

/// The primitive gate: pow_up / pow_down at the ulp level, signed one-sided.
#[test]
fn kernel_pow_gate() {
    let cases: Vec<PowCase> = load_fixture("pow.json");
    for c in &cases {
        let base = s40_to_fixed(&c.base_s40);
        let expo = s40_to_fixed(&c.exponent_s40);
        let (t_floor, t_ceil) = q_to_scale_bounds(&c.pow_exact_q128, 128);

        let up = pow_up(base, expo);
        assert_band(
            check_directional(t_floor, t_ceil, up.0, Direction::Up, bound_ulps()),
            "pow_up",
            c,
        );
        let down = pow_down(base, expo);
        assert_band(
            check_directional(t_floor, t_ceil, down.0, Direction::Down, bound_ulps()),
            "pow_down",
            c,
        );
        // Output domain: (0,1]. pow_up of a positive base never hits zero.
        assert!(up.0 > 0 && up.0 <= ONE.0, "pow_up out of (0,1] in {c:?}");
        assert!(
            down.0 >= 0 && down.0 <= up.0,
            "pow_down out of range in {c:?}"
        );
    }
}

/// Diagnostic tier: the internal bricks (ln, exp, expm1), two-sided.
#[test]
fn kernel_ln_exp_diagnostics() {
    let fixture: LnExpFixture = load_fixture("ln_exp.json");
    for c in &fixture.ln {
        let x = s40_to_fixed(&c.x_s40);
        let (t_floor, t_ceil) = q_to_scale_bounds(&c.neg_ln_q116, 116);
        let actual = ln(x);
        assert!(actual.0 <= 0, "ln of x <= 1 must be <= 0 in {c:?}");
        assert_band(
            check_two_sided(t_floor, t_ceil, -actual.0, bound_ulps()),
            "ln",
            c,
        );
    }
    for c in &fixture.exp {
        let x = Fixed(-s40_to_fixed(&c.neg_x_s40).0);
        let (t_floor, t_ceil) = q_to_scale_bounds(&c.exp_q128, 128);
        let actual = exp(x);
        assert_band(
            check_two_sided(t_floor, t_ceil, actual.0, bound_ulps()),
            "exp",
            c,
        );

        let (m_floor, m_ceil) = q_to_scale_bounds(&c.neg_expm1_q128, 128);
        let em1 = expm1(x);
        assert!(em1.0 <= 0, "expm1 of x <= 0 must be <= 0 in {c:?}");
        assert_band(
            check_two_sided(m_floor, m_ceil, -em1.0, bound_ulps()),
            "expm1",
            c,
        );
    }
}

/// The fixed-point wrappers must be exactly correctly rounded: pure
/// integer operations, checked against the true rational at double width.
#[test]
fn kernel_arith_wrappers() {
    let cases: Vec<ArithCase> = load_fixture("arith.json");
    let one = ONE.0 as u128;
    for c in &cases {
        let a = s40_to_fixed(&c.a_s40);
        let b = s40_to_fixed(&c.b_s40);
        let (au, bu) = (a.0 as u128, b.0 as u128);

        // mul: down = floor(a*b / 2^SCALE), up = ceil
        let product = mul_wide(au, bu);
        let down = Fixed::mul_down(a, b).0 as u128;
        let up = Fixed::mul_up(a, b).0 as u128;
        assert!(
            wide_le(shl_wide(down, SCALE), product) && wide_lt(product, shl_wide(down + 1, SCALE)),
            "mul_down not the floor in {c:?}"
        );
        if product == (0, 0) {
            assert_eq!(up, 0, "mul_up of exact zero in {c:?}");
        } else {
            assert!(
                wide_le(product, shl_wide(up, SCALE))
                    && (up == 0 || wide_lt(shl_wide(up - 1, SCALE), product)),
                "mul_up not the ceiling in {c:?}"
            );
        }

        // div: down = floor(a * 2^SCALE / b), up = ceil (skip b == 0)
        if bu != 0 {
            let numerator = shl_wide(au, SCALE);
            let qd = Fixed::div_down(a, b).0 as u128;
            let qu = Fixed::div_up(a, b).0 as u128;
            assert!(
                wide_le(mul_wide(qd, bu), numerator) && wide_lt(numerator, mul_wide(qd + 1, bu)),
                "div_down not the floor in {c:?}"
            );
            if numerator == (0, 0) {
                assert_eq!(qu, 0, "div_up of exact zero in {c:?}");
            } else {
                assert!(
                    wide_le(numerator, mul_wide(qu, bu))
                        && (qu == 0 || wide_lt(mul_wide(qu - 1, bu), numerator)),
                    "div_up not the ceiling in {c:?}"
                );
            }
        }

        // complement: exact ONE - x, saturating at zero.
        let comp = Fixed::complement(a).0 as u128;
        assert_eq!(comp, one.saturating_sub(au), "complement wrong in {c:?}");
    }
}

/// Check direction and the rational excess-payment bound for every fixture.
#[test]
fn kernel_in_given_out_gate() {
    let cases: Vec<InGivenOutCase> = load_fixture("in_given_out.json");
    assert_eq!(
        cases[0].zone, "sale_start",
        "danger zone must lead the file"
    );
    assert_eq!(
        SCALE, 52,
        "trade bounds require separate qualification at other scales"
    );
    let mut counts = QualificationCounts::default();
    for c in &cases {
        let amount_in = calc_in_given_out(
            parse_u128(&c.balance_in),
            parse_u128(&c.weight_in),
            parse_u128(&c.balance_out),
            parse_u128(&c.weight_out),
            parse_u128(&c.amount_out),
        );
        let truth = parse_u128(&c.amount_in_ceil);
        let balance_in = parse_u128(&c.balance_in);

        // Direction: the pool must never charge less than the true price.
        assert!(
            amount_in >= truth,
            "UNDERCHARGE (fund leak): kernel {amount_in} < truth {truth} in {c:?}"
        );

        let bound = payment_error_bound(c, amount_in);
        let overcharge = BigUint::from(amount_in - truth);
        assert!(overcharge <= bound, "payment error exceeds bound in {c:?}");
        let payment = BigUint::from(amount_in);
        let lower = if payment > bound {
            payment - &bound
        } else {
            BigUint::default()
        };
        counts.record(
            parse_u128(&c.amount_out) == 0 || amount_in == 0,
            balance_in.checked_add(amount_in).is_some(),
            lower,
            &bound,
        );
    }
    counts.report("exact-output", cases.len());
}

#[test]
fn mixed_decimal_sales_have_small_buy_oracle_coverage() {
    let outputs: Vec<OutGivenInCase> = load_fixture("out_given_in.json");
    let payments: Vec<InGivenOutCase> = load_fixture("in_given_out.json");
    let mut checked = 0;
    for c in outputs.iter().filter(|c| c.zone == "mixed_decimals") {
        let actual = calc_out_given_in(
            parse_u128(&c.balance_in),
            parse_u128(&c.weight_in),
            parse_u128(&c.balance_out),
            parse_u128(&c.weight_out),
            parse_u128(&c.amount_in),
        );
        assert!(actual <= parse_u128(&c.tokens_out_floor), "{c:?}");
        checked += 1;
    }
    for c in payments.iter().filter(|c| c.zone == "mixed_decimals") {
        let actual = calc_in_given_out(
            parse_u128(&c.balance_in),
            parse_u128(&c.weight_in),
            parse_u128(&c.balance_out),
            parse_u128(&c.weight_out),
            parse_u128(&c.amount_out),
        );
        assert!(actual >= parse_u128(&c.amount_in_ceil), "{c:?}");
        checked += 1;
    }
    assert!(
        checked >= 600,
        "only {checked} mixed-decimal trades checked"
    );
}

#[test]
fn equal_weight_boundaries_have_exact_rational_coverage() {
    let outputs: Vec<OutGivenInCase> = load_fixture("out_given_in.json");
    let payments: Vec<InGivenOutCase> = load_fixture("in_given_out.json");
    let mut checked = 0;
    for c in outputs.iter().filter(|c| c.zone == "equal_weight_boundary") {
        let actual = calc_out_given_in(
            parse_u128(&c.balance_in),
            parse_u128(&c.weight_in),
            parse_u128(&c.balance_out),
            parse_u128(&c.weight_out),
            parse_u128(&c.amount_in),
        );
        assert!(actual <= parse_u128(&c.tokens_out_floor), "{c:?}");
        checked += 1;
    }
    for c in payments
        .iter()
        .filter(|c| c.zone == "equal_weight_boundary")
    {
        let actual = calc_in_given_out(
            parse_u128(&c.balance_in),
            parse_u128(&c.weight_in),
            parse_u128(&c.balance_out),
            parse_u128(&c.weight_out),
            parse_u128(&c.amount_out),
        );
        assert!(actual >= parse_u128(&c.amount_in_ceil), "{c:?}");
        checked += 1;
    }
    assert!(
        checked >= 20,
        "only {checked} exact-rational trades checked"
    );
}

#[test]
fn near_u64_weights_match_oracle_direction() {
    let outputs: Vec<OutGivenInCase> = load_fixture("out_given_in.json");
    let payments: Vec<InGivenOutCase> = load_fixture("in_given_out.json");
    let mut checked = 0;
    for c in outputs.iter().filter(|c| c.zone == "large_weights") {
        let actual = calc_out_given_in(
            parse_u128(&c.balance_in),
            parse_u128(&c.weight_in),
            parse_u128(&c.balance_out),
            parse_u128(&c.weight_out),
            parse_u128(&c.amount_in),
        );
        assert!(actual <= parse_u128(&c.tokens_out_floor), "{c:?}");
        checked += 1;
    }
    for c in payments.iter().filter(|c| c.zone == "large_weights") {
        let actual = calc_in_given_out(
            parse_u128(&c.balance_in),
            parse_u128(&c.weight_in),
            parse_u128(&c.balance_out),
            parse_u128(&c.weight_out),
            parse_u128(&c.amount_out),
        );
        assert!(actual >= parse_u128(&c.amount_in_ceil), "{c:?}");
        checked += 1;
    }
    assert!(checked >= 24, "only {checked} large-weight trades checked");
}

fn big(value: &str) -> BigUint {
    BigUint::parse_bytes(value.as_bytes(), 10).expect("unsigned integer fixture")
}

fn ceil_ratio(num: BigUint, den: &BigUint) -> BigUint {
    (num + den - 1u32) / den
}

// Scale-52 rational guards are derived in docs/error-analysis.md.
fn payout_error_bound(c: &OutGivenInCase, actual: u128) -> BigUint {
    let (bi, bo, ai, wi, wo) = (
        big(&c.balance_in),
        big(&c.balance_out),
        big(&c.amount_in),
        big(&c.weight_in),
        big(&c.weight_out),
    );
    if wi == wo {
        return bo * &ai / (&bi + &ai) - actual;
    }
    let h = BigUint::from(1u32) << 20;
    let num = &wi * (&h + 1u32) * (&bi + &ai)
        + &ai * &h * &wo
        + (BigUint::from(912u32) << 10) * &bi * &wo;
    let den = (BigUint::from(1u32) << 72) * bi * wo;
    ceil_ratio(bo * num, &den)
}

fn payment_error_bound(c: &InGivenOutCase, actual: u128) -> BigUint {
    let (bi, bo, ao, wi, wo) = (
        big(&c.balance_in),
        big(&c.balance_out),
        big(&c.amount_out),
        big(&c.weight_in),
        big(&c.weight_out),
    );
    if wi == wo {
        let truth = ceil_ratio(&bi * &ao, &(&bo - &ao));
        return BigUint::from(actual) - truth;
    }
    let h: BigUint = BigUint::from(1u32) << 52;
    let d = BigUint::from(1u32) << 62;
    let m = BigUint::from(1u32) << 72;
    let dx = (BigUint::from(1u32) << 20) + 1u32;
    let ell_num = (&bo - &ao) * &m - &dx * &bo;
    let b_hi_num = (&wo * &h + &wi).min(99u32 * &wi * &h);
    let form_num = b_hi_num * &dx * &bo + &wi * (&dx * &bo + &ao * &m);
    let form_den = &wi * h * ell_num;
    let e_num = form_num * &d + 912u32 * &form_den;
    let e_den = form_den * &d;
    let total = &bi + actual;
    let price_den = &bi * e_den + &total * &e_num;
    let num = e_num * &total * total * &d + &bi * &price_den;
    ceil_ratio(num, &(price_den * d))
}

#[derive(Default)]
struct QualificationCounts {
    accepted: usize,
    dust: usize,
    accuracy_rejected: usize,
    representation_rejected: usize,
}

impl QualificationCounts {
    fn record(&mut self, dust: bool, fits: bool, lower: BigUint, bound: &BigUint) {
        if dust {
            self.dust += 1;
        } else if !fits {
            self.representation_rejected += 1;
        } else if lower != BigUint::default() && bound * 10_000u32 <= lower {
            self.accepted += 1;
        } else {
            self.accuracy_rejected += 1;
        }
    }

    fn report(&self, operation: &str, total: usize) {
        assert_eq!(
            self.accepted + self.dust + self.accuracy_rejected + self.representation_rejected,
            total
        );
        assert!(self.accepted > 0);
        assert!(self.accuracy_rejected > 0);
        println!(
            "{operation}: total={total} accepted={} dust={} accuracy_rejected={} representation_rejected={}",
            self.accepted, self.dust, self.accuracy_rejected, self.representation_rejected
        );
    }
}

/// Reports the worst pow/payout errors at the compiled SCALE.
#[test]
fn measure_error_margins() {
    let cases: Vec<PowCase> = load_fixture("pow.json");
    // (zone, worst raw offset below truth, above truth, worst up/down err)
    let mut worst_low = 0i128; // raw below t_floor, in ulps
    let mut worst_high = 0i128; // raw above t_ceil, in ulps
    let (mut worst_up, mut worst_down) = (0i128, 0i128);
    for c in &cases {
        let base = s40_to_fixed(&c.base_s40);
        let expo = s40_to_fixed(&c.exponent_s40);
        let (t_floor, t_ceil) = q_to_scale_bounds(&c.pow_exact_q128, 128);
        let raw = pow(base, expo).0;
        worst_low = worst_low.max(t_floor - raw);
        worst_high = worst_high.max(raw - t_ceil);
        worst_up = worst_up.max(pow_up(base, expo).0 - t_ceil);
        worst_down = worst_down.max(t_floor - pow_down(base, expo).0);
    }
    println!(
        "SCALE={SCALE} raw_below={worst_low} raw_above={worst_high} \
         up_err={worst_up} down_err={worst_down} bound={}",
        bound_ulps()
    );

    let swaps: Vec<OutGivenInCase> = load_fixture("out_given_in.json");
    let mut worst_rel_num = 0u128; // shortfall as multiple of balance_out ulps
    for c in &swaps {
        let tokens_out = calc_out_given_in(
            parse_u128(&c.balance_in),
            parse_u128(&c.weight_in),
            parse_u128(&c.balance_out),
            parse_u128(&c.weight_out),
            parse_u128(&c.amount_in),
        );
        let truth = parse_u128(&c.tokens_out_floor);
        assert!(tokens_out <= truth);
        let shortfall = truth - tokens_out;
        let ulp_of_reserve = (parse_u128(&c.balance_out) >> SCALE).max(1);
        worst_rel_num = worst_rel_num.max(shortfall / ulp_of_reserve);
    }
    println!("payout worst shortfall = {worst_rel_num} reserve-ulps (balance_out * 2^-SCALE)");
}

/// Check direction and the rational payout-loss bound for every fixture.
#[test]
fn kernel_out_given_in_gate() {
    let cases: Vec<OutGivenInCase> = load_fixture("out_given_in.json");
    assert_eq!(
        cases[0].zone, "sale_start",
        "danger zone must lead the file"
    );
    assert_eq!(
        SCALE, 52,
        "trade bounds require separate qualification at other scales"
    );
    let mut counts = QualificationCounts::default();
    for c in &cases {
        let tokens_out = calc_out_given_in(
            parse_u128(&c.balance_in),
            parse_u128(&c.weight_in),
            parse_u128(&c.balance_out),
            parse_u128(&c.weight_out),
            parse_u128(&c.amount_in),
        );
        let truth = parse_u128(&c.tokens_out_floor);
        let balance_out = parse_u128(&c.balance_out);

        // Direction: never pay out more than the true floored payout.
        assert!(
            tokens_out <= truth,
            "OVERPAYMENT (fund leak): kernel {tokens_out} > truth {truth} in {c:?}"
        );
        assert!(tokens_out <= balance_out, "payout exceeds reserve in {c:?}");

        let bound = payout_error_bound(c, tokens_out);
        let shortfall = BigUint::from(truth - tokens_out);
        assert!(shortfall <= bound, "payout error exceeds bound in {c:?}");
        counts.record(tokens_out == 0, true, BigUint::from(tokens_out), &bound);
    }
    counts.report("exact-input", cases.len());
}
