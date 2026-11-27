# Oracle fixtures

CI reads the committed JSON; it does not run Python. Regeneration is deterministic and must be byte-identical. The generator uses mpmath at 220 decimal digits and exact `Fraction` arithmetic for integer exponents. It does not call the Rust kernel to obtain expected results.

## Regenerate

From this directory:

```sh
pip install -r requirements.txt
python3 gen.py
```

Finite high-precision agreement is evidence, not a universal proof of correct rounding.

## Fixtures

- `pow.json`: 87 power cases on the exact `2^-40` dyadic input grid, with 78-digit values and q128 floors. The primitive gate isolates algorithmic error from input quantization.
- `ln_exp.json`: logarithm, exponential and complement diagnostics. Negative logarithms use q116; exponential values use q128.
- `out_given_in.json`: exact raw-amount references rounded down. The 351 executable cases include unequal weights near `2^64`, exact-rational equal weights, mixed decimals, dust and `u128` edges.
- `in_given_out.json`: exact raw-amount references rounded up. The 341 executable cases include the inverse-exponent stress corner and the same wider input coverage.
- `arith.json`: exact dyadic inputs for public fixed-point multiplication, division and complement checks.
- `scales.json`: representation-grid data for historical scale sweeps. It does not qualify the current trade guards at another scale.
- `balancer_inputs.json`: independent truth for the rounded Balancer input grid. Its explicit `skip` fields identify inputs outside that implementation's domain; the corresponding capture is separate from kernel qualification.

The swap fixtures retain first-order sensitivity fields as diagnostics. The current gates use the [scale-52 rational error bounds](../../../docs/error-analysis.md) with BigUint comparisons. They never skip a magnitude check because a sensitivity exceeds `u128`, and they never saturate a tolerance to make it pass. Every executable swap gets a direction and bound check, plus an explicit accepted, dust, accuracy-rejected or representation-rejected classification.

The mixed-decimal grid uses every pair of 0, 6, 8, 18 and 24 decimals. Reserves are one billion whole units of each token, up to `10^33` raw units. Buys are 1, 100 and 10000 whole units. This tests raw-unit conversion and small buys in large pools without assuming both tokens have equal decimals. It does not promise that every grid trade passes the accuracy guard.

Large raw weights use their exact rational ratio in the oracle. They are not passed to the small-integer BigInt invariant referee as huge exponents. The valid near-99 pair uses `(2^64, ceil(2^64/99))`, not the invalid pair with a floored denominator.

Expected-panic cases stay in `tests/overflow_envelope.rs`, outside the executable guest arrays. Dust and accuracy-rejected executable cases still participate in host/guest parity.
