use weighted_math_core::{calc_in_given_out, calc_out_given_in};

#[test]
fn equal_weight_quotes_round_in_the_pools_favor() {
    let output = calc_out_given_in(1000, 1, 1000, 1, 300);
    let input = calc_in_given_out(1000, 1, 1000, 1, 300);

    assert_eq!((output, input), (230, 429));
}
