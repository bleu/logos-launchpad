# Overflow envelope

Status: arithmetic argument and regression evidence for the unaudited kernel. The selected public scale is 52; the internal scale is 62. Fit arguments below distinguish inputs that cannot overflow from inputs that are rejected by release-active guards. This is not an audit or a proof of production weighted-sale handler safety.

Release arithmetic can wrap when overflow checks are disabled. Correctness must therefore follow from bounded operands or explicit checked operations. The weighted workspace enables release overflow checks, but the regression suite also runs without them.

## Enforced inputs and results

| Check | Enforcement |
| --- | --- |
| Positive reserves | Both swap functions assert `balance_in >= 1 && balance_out >= 1`. |
| Exact-input total below `2^128` | `balance_in.checked_add(amount_in)`. |
| Weights in `[1, 2^64]`, ratio in `[1/99, 99]` | `check_weights`, before shift or ratio formation. |
| Exact-output amount at most `floor(3 * balance_out / 10)` | Overflow-safe quotient/remainder cap. |
| Power base in `(0, 1)`, exponent in `(0, 99]` | Power domain assertions. |
| Shifted floor and rounded ceiling fit `u128` | Fit assertion and checked carry in `wide.rs`. |
| Public fixed-point result fits nonnegative `i128` | `checked_repr`. |
| Fixed-point division numerator fits | `numerator`. |
| Positive padded power before exact-output inversion | `p62 > 0` assertion. |

These guards panic in the raw kernel. The checked quote interface returns defined errors. It validates the power domain, checks positive padded power and uses bounded wide division for rounded payments. It also checks the post-buy pricing reserve. The sale handler checks vault capacity and accounting separately. A representable payment alone does not establish that `balance_in + payment` fits.

## Widening multiplication and ceiling carry

`mul_wide(a, b)` forms the exact 256-bit product from four 64-bit partial products. Each partial product is below `2^128`. Cross-term and low-word carries are recorded with `overflowing_add` and included in the high word. The high-word sum is nonnegative and its mathematical total is `floor(a*b / 2^128) <= 2^128 - 2`; no partial sum can exceed that total.

For `0 < shift < 128`, `hi >> shift == 0` proves only that the shifted floor fits `u128`. It does not prove that the ceiling fits. The floor can be `u128::MAX` with a nonzero discarded remainder. In that case the ceiling is `2^128`.

`mul_shr_up` now uses `checked_add` for that final carry. A floor that does not fit is rejected by the shift assertion. A ceiling that does not fit is rejected with `widened product exceeds u128 after rounding`. Both guards work with overflow checks on or off.

### Public fixed-point reproduction

At public scale 52, let `a = 2^90 - 1` and `b = 2^90 + 1`. Both are valid nonnegative `Fixed` representations. Their product is `2^180 - 1`. Its scale-52 floor is `2^128 - 1`, with discarded remainder `2^52 - 1`. Its ceiling is `2^128`.

Before the fix, public `Fixed::mul_up` returned zero in unchecked release. The wrapped zero then passed the signed representation check. The regression `fixed_multiplication_rejects_an_unrepresentable_ceiling` first failed with that implementation and passes with checked carry.

### Public swap reproduction

The same defect is reachable through public exact-output pricing at the selected scale. Use `balance_in = 326937960375019080809068952671530218863`, `weight_in = 1`, `balance_out = 10`, `weight_out = 2`, and `amount_out = 3`.

The computed scale-62 payment ratio is `r62 = 4799918100812182583`. The final widened product divided by `2^62` has floor `340282366920938463463374607431768211455` and remainder `2366108921125422809`. Thus the floor fits, but the ceiling does not. With the old helper, the public call returned zero in unchecked release. With checked carry, it panics. The regression `exact_output_rejects_an_unrepresentable_ceiling` records both outcomes.

This establishes raw-kernel reachability, not a valid settled weighted sale trade. The checked interface rejects this payment and its unrepresentable post-buy pricing balance. Sale host tests check rejection before transfers. Seeded guest tests cover arithmetic rejection and rollback.

## Exact-input ledger

Let `S = 52`, `L = 62`, and `U = 2^62`. The arithmetic type is `u128` unless stated otherwise.

| Intermediate | Bound and reason |
| --- | --- |
| `total = balance_in + amount_in` | At most `2^128 - 1`, enforced with checked addition. |
| `99 * weight` | At most `99 * 2^64 < 2^71`. |
| `weight_in << S` | At most `2^116`. |
| Floored exponent | At most `99 * 2^52 < 2^59`; positive under the ratio check; fits `i128`. |
| Formed base | Ratio lemma below; all shifts fit and the signed cast is safe. |
| Complement after padding | In `[0, U]`, from the power range and clamp. |
| Final `balance_out * complement` | Below `2^190`; exact widened multiplication; after shifting 62 the payout is at most `balance_out`. |

If base quantization reaches `ONE` or above, exact-input returns zero before calling power. The checked interface rejects zero-result trades as dust. The sale handler must not collect payment for them.

### Ratio formation

For a large denominator, the helpers choose `excess = bit_length(den) - (126 - S)` when positive. After the shift, the denominator has at most `126 - S` bits and is positive. `ratio_up` uses `(num >> excess) + 1` over `den >> excess`; `ratio_down` uses `num >> excess` over `(den >> excess) + 1`.

The shifted numerator with bias is at most `2^(126-S)`. Shifting it by `S` is at most `2^126`, so it fits `u128`. The quotient also fits the nonnegative `i128` representation. In the downward case the numerator is strictly smaller than the biased denominator, so the quotient is below `ONE`. The pre-shift biases preserve the intended direction; their accuracy cost is separate from these fit arguments.

## Power ledger

| Intermediate | Bound and reason |
| --- | --- |
| Initial `m = x << (62 - S)` | At most `U`; exact scale conversion. |
| Logarithm normalization count | At most `S`; normalized `m` is in the rounded square-root window, below `2^63`. |
| `(m - U) << 62` | Magnitude below `2^123`; fits signed `i128`. |
| `m + U` | Below `2^64`. |
| `t` | Magnitude below `0.172 * U < 2^60`. |
| `t * t` | Below `2^120`. |
| `u` | Below `0.03 * U`. |
| Logarithm Horner accumulator | Below `1.031 * U < 2^63`. |
| Horner products | `p * u < 2^120`; `t * p < 2^122`. |
| `k * LN2_62` and negative logarithm | Below `42 * U < 2^68` even at the compile-time maximum scale 60. |
| `exponent * negative_logarithm` | Below `4158 * 2^(S+62)`; widened for scale-sweep headroom. After shift `S`, below `2^75`; fits the asserted exponential argument envelope. |
| Exponential argument times inverse-log constant | Below `2^107` under `neg_arg < 2^76`. |
| Exponential range-reduction count | Below `2^15`; products and correction steps fit `i128`. |
| Reduced exponential argument | In `[0, LN2_62)` after correction loops. |
| Exponential Horner accumulator | Magnitude below `3.27 * U < 2^64`; products below `2^126`. |
| Final exponential result | In `[0, U]`; for the reduced interval its polynomial approximates a value in `[1/2, 1]`, with the much smaller error accounted for in [error analysis](error-analysis.md). |
| Final right shift | Only performed for `k <= 62`; larger `k` returns zero. |
| Public nearest rounding and pads | Add at most `2^54` to at most `U`; all results fit signed `i128` and are clamped to the output range. |

At `S = 52`, the power argument product itself fits `u128`; widening keeps the same implementation valid at larger compile-time scales. Numerical accuracy is qualified separately for scale 52. Compiling another scale does not establish its accuracy.

## Exact-output ledger

| Intermediate | Bound and reason |
| --- | --- |
| Reserve cap | `b/10*3 + b%10*3/10 = floor(3b/10)`; no `3*b` full-width overflow. |
| Remaining output reserve | At least `ceil(0.7 * balance_out) >= 1`. |
| Shifted weight and ceiling exponent | Numerator at most `2^116`; exponent at most `99 * 2^52`. |
| Formed base | Positive and below `ONE` at the selected scale; ratio lemma above. |
| Padded power | At most `U`; positivity is asserted before inversion. Do not infer economic accuracy from the 30% cap. |
| `(U - p62) << 62` | At most `2^124`. |
| Ceiling division for `r62` | At most `(U - 1) * U < 2^124` when `p62 >= 1`. |
| Final `balance_in * r62` | Below `2^252`; exact widened product. Its shifted floor and rounded ceiling each need a fit guard. |

The final payment fit is an input-dependent guard, not a universal in-domain fit theorem. [Error analysis](error-analysis.md) also addresses payment amplification when padded power is small.

## Public fixed-point wrappers

Multiplication uses the widened floor or checked ceiling, then checks the signed representation. Division checks that `a << S` fits before division; with nonnegative signed operands, any ceiling addition stays below `2^128`. Complement subtracts a nonnegative `i128` representation from `ONE`, which remains within signed range.

Public `exp` and `expm1` saturate inputs at or below -64 before negation or widening. This avoids negating `i128::MIN`, losing bits in a checked shift that only checks the shift amount, or overflowing range-reduction multiplication. It is not a swap-path issue: swap arguments have the smaller bound above.

## Verification

`crates/harness/tests/overflow_envelope.rs` tests deposit boundaries, full-width payout/payment scaling, public power corners, deep negative exponential saturation, unrepresentable payments, and both carry reproductions. These are executable checks of the stated interface behavior, not substitutes for the arithmetic arguments.

From the weighted-math workspace, run:

```sh
cargo +1.94.0 test --workspace --locked
cargo +1.94.0 test --workspace --locked --release --config 'profile.release.overflow-checks=true'
cargo +1.94.0 test --workspace --locked --release --config 'profile.release.overflow-checks=false'
```

The unchecked-release red/green carry results are recorded in the [qualification evidence](qualification.md). Host tests, guest execution, real proving and full platform-handler evidence are different checks.

## Checked quote arithmetic

The checked layer forms each 128-by-128 product with `mul_wide`. A shifted divisor never has more bits than the wide numerator. Division subtracts only when the remainder covers the shifted divisor. A quotient bit at position 128 or above is an error. The final ceiling uses `checked_add`, so a floor of `u128::MAX` with a remainder is also an error. These operations do not depend on release overflow checks.

Error bounds use scale `2^96` and round in the conservative direction. The [error analysis](error-analysis.md#checked-quote-interface) defines the acceptance checks. Invalid inputs, dust, insufficient accuracy, power underflow and unrepresentable results return `QuoteError`. The raw functions and their overflow regression cases retain their previous behaviour. Seeded sale guest tests cover checked-quote integration and rollback. Full-fixture checked-quote host/guest parity and guest costs remain unverified.
