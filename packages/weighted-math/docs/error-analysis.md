# Weighted-math error bounds

Status: hand-derived bounds and finite test evidence for the unaudited kernel at public `SCALE = 52` and internal `LN_SCALE = 62`. This is not a machine-checked proof or an audit. A compiled scale sweep does not establish the same accuracy at another scale. Current measured results are in [qualification evidence](qualification.md).

## Units and rounding

Let `D = 2^62`, `H = 2^52`, and one internal unit be `1/D`. Nearest coefficient rounding loses at most half a unit. Integer division truncates toward zero and loses less than one destination unit. Signed arithmetic right shifts floor, including negative products, and lose less than one destination unit. These operations must not all be assigned half-unit errors.

Use rational upper bounds `T = 43/250 = 0.172`, `r = T^2`, and `s_max = 347/500 = 0.694`. The implemented logarithm has 13 atanh coefficients; the exponential has 20 Taylor coefficients. Neither term count is inferred from a fixture.

The stored atanh coefficients approximate `D/(2i+1)` to at most half a unit. The exponential coefficients approximate `(-1)^i * D/i!` to at most half a unit. Their rounding bounds can be checked with exact fractions. The atanh constant coefficient and the first three exponential coefficients are exact.

The rounded logarithm constant has the same half-unit bound. A rational certificate uses `L_low = 2 * sum(i=0..49, 1/((2i+1)*3^(2i+1)))` and remainder `R = 2/(101*3^101*(1-1/9))`. Exact fraction comparisons give `LN2_62 - 1/2 < D*L_low <= D*ln(2) <= D*(L_low+R) < LN2_62 + 1/2`.

For the normalization constant `A = SQRT2_HALF_62`, the integer inequalities `(2A-1)^2 < 2D^2 < (2A+1)^2` certify nearest rounding of `D/sqrt(2)`. Its actual integer window gives both the exact transformed ratio and the truncated ratio magnitude below `T`.

## Logarithm

Scale conversion and repeated doubling are exact. Write `x = m * 2^-k`, with `0 <= k <= 52` and `A/D <= m < 2A/D`. The transformed ratio is `t = (m-1)/(m+1)`. The atanh polynomial is `P(u) = sum(i=0..12, u^i/(2i+1))`, with `u = t^2`.

| Error source | Bound in internal units |
| --- | --- |
| Division for `t`, with derivative of `2*atanh(t)` | `2/(1-r)` |
| Floor of `t^2`, using `P'(u) <= 1/(3*(1-r)^2)` | `2*T/(3*(1-r)^2)` |
| Coefficient rounding and Horner floors | `2*T*(1+r/2)/(1-r)` |
| Final signed product floor | `1` |
| Omitted odd terms from degree 27 | `2*D*T^27/(27*(1-r))` |

The constant coefficient is exact. Coefficient error starts at `u`, while the twelve Horner-floor errors start with weight one. Multiplication by `2*t` gives the third row. The operation `(t*p)>>61` floors the scaled `2*t*p` once; its output error is less than one unit.

The exact rational sum is below 3.544, hence below 3.6. Reconstruction with rounded ln(2) adds at most `k/2` units. Thus `abs(delta_ln) < (3.6+k/2)/D`. This full-unit accounting replaces the earlier half-unit truncation claim.

## Exponential

The reciprocal-log constant selects an initial range index. The correction loops then produce exact integer quotient and remainder for stored `L = LN2_62`: `z = j*L + s`, with `0 <= s < L`. Therefore the reciprocal estimate adds no final numerical error.

Since `s/D < s_max`, the nineteen Horner floors contribute less than `1/(1-s_max)` units. A conservative coefficient allowance is `s_max/(2*(1-s_max))`, since the constant coefficient is exact. Their sum is below 4.402 units. The degree-19 alternating Taylor remainder is at most `D*s_max^20/20! < 0.001274` units.

For `0 <= j <= 62`, the final right shift adds less than one unit. Replacing ln(2) by the rounded constant contributes less than `(j/2)*exp(j/(2D))/2^j < 0.251` units. For `j > 62`, the zero-return branch has error below one internal unit. A six-unit bound covers both branches. Underflow cases are included, not discarded from the argument.

## Power argument and damping

Let `b` be the represented exponent, `0 < b <= 99`, and `a = -ln(x)`. Flooring the widened argument product adds less than one unit. Argument error is bounded by `(b*(3.6+k/2)+1)/D`, below `3000/D`. Its exponential amplification is below 1.001, using `exp(v) <= 1/(1-v)` for this small nonnegative `v`.

For logarithm normalization `k = 0`, `x^b <= 1`. Power error is below `1.001*(99*3.6+1)+6 < 364` units.

For `k >= 1`, `a > 0.69*k-0.35 >= 0.34*k`. This uses ln(2)>0.69 and `m<1.416<exp(0.35)`, each checkable with rational series. Since `b*exp(-a*b) <= 1/(e*a) < 1/a`, power error is below `1.001*((3.6+k/2)/(0.34*k)+1)+6 <= 1.001*(4.1/0.34+1)+6 < 20` units.

Use `400/D` as a conservative unpadded power bound. The existing 512-unit swap pad covers it. The total one-sided kernel-plus-pad error is below `K = 912/D`. Clamping at zero cannot make a downward-padded quantity overstate its ideal value or increase this loss bound.

Public nearest rounding adds half a scale-52 unit. The resulting error is below `400/1024 + 1/2 = 0.890625` public units, so the two-unit directional wrappers have enough margin at scale 52. This reasoning does not establish their direction at every allowed compile-time scale. The old statement that `2^-53.5` is below half a scale-56 unit was false; it is about 5.66 such units. Also, `2^-53` is not twice `2^-53.5`.

## Formation error

Weight-ratio floor or ceiling adds less than `delta_b = 1/H`. If an amount ratio needs a pre-shift, write its original denominator as `d*h + remainder`, where `h = 2^excess`. At scale 52, the shifted denominator has `d >= 2^73`. Upward formation changes the ratio to `(n+1)/d`, an increase below `2/d <= 2^-72`; downward formation changes it to `n/(d+1)`, a decrease below `2/(d+1) < 2^-72`. Final division adds less than `1/H`.

Use `delta_x = 1/H + 1/2^72` as a conservative absolute base error. The old claim of a uniform relative pre-shift error was false for small numerators. Without a pre-shift, `1/H` alone suffices, but the generic guards below use the larger bound.

## Trade-relative policy

The internally selected target is at most one basis point (`epsilon = 1/10000`) of extra arithmetic loss against the independently rounded ideal trade. One basis point is one fee-rate step. This target keeps approximation loss below that step; it is a product choice, not a tolerance that mathematics can derive. Slippage, protocol fees and the natural raw-unit floor/ceiling remain separate. Do not widen the target to make dust or ill-conditioned trades pass.

The following rational guards are conservative. The test harness evaluates them with `BigUint`, without saturation, floating point or skipped cases. The checked quote interface uses the interval checks below. The original harness guards remain independent qualification checks.

### Exact input

Let `B = balance_in`, `R = balance_out`, `A = amount_in`, `x = B/(B+A)`, and `b = weight_in/weight_out`. Formation increases the base and decreases the exponent, both increasing power. On that rectangle, `abs(d(x^b)/dx) <= b/x` and `abs(d(x^b)/db) <= -ln(x) <= (1-x)/x`. Therefore:

```text
E_out = (b*delta_x + (1-x)*delta_b)/x + K
Delta_out = ceil(R*E_out)
```

The returned payout `Q` does not exceed the ideal floored payout. Its additional loss is at most `Delta_out`, since `floor(v+w)-floor(v) <= ceil(w)`. Reject `Q = 0`. Requiring `Delta_out*10000 <= Q` guarantees the target against the ideal floored trade, with some conservative rejection.

### Exact output

Let `B = balance_in`, `x = (balance_out-amount_out)/balance_out`, `b = weight_out/weight_in`, `ell = x-delta_x > 0`, and `b_hi = min(99,b+delta_b)`. Formation decreases base and increases exponent, both decreasing power. A conservative bound is:

```text
E_in = (b_hi*delta_x + (1-ell)*delta_b)/ell + K
```

If `q` is the actual downward-padded power and `p` is ideal power, then `0 < q <= p <= min(1,q+E_in)`. The unrounded relative payment excess is bounded by `E_in/[q*(1-q-E_in)]` only when `q+E_in < 1`. Omitting `E_in` from that denominator is not valid.

The public-output guard avoids private power access. Let `C` be returned payment. Because `C >= B*(1/q-1)`, `q >= q_lo = B/(B+C)`. Absolute unrounded excess is at most `B*E_in/[q_lo*(q_lo+E_in)]`. Inner ratio ceiling adds less than `B/D`; comparing outer ceiling against the ideal ceiling needs no separate extra unit:

```text
Delta_in = ceil(B*E_in/[q_lo*(q_lo+E_in)] + B/D)
```

Require `C > Delta_in` and `Delta_in*10000 <= C-Delta_in`. This guarantees the target against ideal ceiled payment. The absolute bound stays safe if `q_lo+E_in >= 1`, but is usually too loose to accept the trade. Check `B+C` fit separately: economic accuracy does not imply representability.

### Equal weights and coarse units

Equal weights have exact rational references:

```text
ideal_out = floor(balance_out*amount_in/(balance_in+amount_in))
ideal_in = ceil(balance_in*amount_out/(balance_out-amount_out))
```

Use these to tighten the guard when a generic one-unit ceiling rejects an otherwise accurate coarse-unit trade. The harness compares those exact references with the actual kernel output. It does not require zero approximation error: for equal reserves and a deposit equal to the reserve, the existing pad can make payout one or more raw units below the exact half-reserve reference. The checked interface uses bounded wide division for these references.

## Checked quote interface

`try_calc_out_given_in` and `try_calc_in_given_out` return `QuoteError`. They check reserves, weights, amounts and result fit. The exact-output cap remains 30%. Exact input has no additional ratio cap; the total deposit must fit. Only public scale 52 is qualified. The raw functions keep their existing results and panic contract.

The checks use `S = 2^96` for error bounds only. This does not change either kernel scale. `delta_x`, `delta_b` and `K` have exact representations at this scale. Round a base lower bound down and an exponent upper bound up. Round each positive error term and division up. For exact output, subtract `delta_x` from the base lower bound and add `delta_b` to the exponent upper bound, capped at 99. Thus the computed bounds are at least `E_out` and `E_in` above.

Use the actual padded internal power to tighten the final integer check. Let `m` be the exact-input padded complement, in real units, and `q` the exact-output padded power. The existing error argument gives:

```text
ideal complement <= min(1, m + E_out)
ideal power      <= min(1, q + E_in)
```

For exact input, calculate `ideal_upper = floor(balance_out * min(1, m + E_out))`. Set `loss = ideal_upper - payout`. Accept only when the payout is positive and `loss * 10000 <= payout`. For exact output, set `p_upper = min(1, q + E_in)` and calculate `ideal_lower = ceil(balance_in * (1 - p_upper) / p_upper)`. Set `loss = payment - ideal_lower`. Accept only when `ideal_lower > 0` and `loss * 10000 <= ideal_lower`. These bounds compare rounded ideal amounts. The payment comparison includes both raw-kernel ceilings. Equal weights use their exact rational references instead.

Bound calculations use full 128-by-128 products and bounded wide division. They return an error if a required result cannot fit. Comparisons use division by 10000 to avoid an overflowing error-times-10000 product. No floating point, saturation of an error bound, or panic recovery is used. The existing caps and validated formed inputs establish the power kernel's domain. A zero padded power and an unrepresentable rounded payment return defined errors.

The sale handler uses these quotes. It separately checks current vault coverage, full recorded token inventory, pricing collateral, proceeds, fees and post-buy vault capacity. It saves the new records only after these checks pass. These host checks do not establish guest execution or cross-program rollback.

## Practical range and rejected corner

Qualification does not impose one decimal count or an artificial small-pool cap. The fixture grid covers each combination of 0, 6, 8, 18 and 24 decimals, billion-whole-token reserves up to `10^33` raw units, and buys of 1, 100 and 10000 whole units. Other fixtures cover raw `u128` edges, dust, ratio endpoints and unequal weights near `2^64`. Each evaluated case gets a direction check and an error-bound check, even if it fails the accuracy policy.

For six-decimal collateral reserve `10^15` and 18-decimal launch reserve `10^27`, a `10^6` collateral buy at equal weights is about one collateral token buying one launch token from a billion-token pool. The generic exact-input loss bound is `419803293389` raw launch units, about `0.000000419804` launch token. The corresponding one-token exact-output bound is one raw collateral unit. Both satisfy the one-basis-point target; the equal-weight references can tighten them further.

Some coarse-unit and smaller buys remain dust or cannot be certified by the generic guard. They are rejected and counted. The existing 30% exact-output cap is a domain limit, not an accuracy guarantee. At exponent 99 and 30% drain, ideal power is `0.7^99`, about 2130.913 internal units. Subtracting the nominal 512-unit pad alone produces about 31.6% payment excess. The accuracy guard rejects this corner.

## Panic prevention and evidence limits

The raw kernel intentionally panics on invalid or unrepresentable inputs. An already-returned quote passing these guards cannot prove that the original call was safe. Callers of a raw function need preflight for intermediates, positive padded power and rounded payment. The checked interface supplies these checks. Sale handlers must also check post-buy reserve/vault additions and fees before committing anything. A failed trade must not save a checkpoint or move tokens.

For callers that retain the raw exact-output function, this alternative preflight applies at scale 52. Validate reserves, weights, output cap and formed inputs first. Public `pow_down(formed_base, formed_exponent)` is below the exact formed power. Let `u` be its nonnegative scale-52 representation, `q_min = u*1024-912`, and reject if `q_min <= 0`. The actual internal padded power is at least `q_min`. With checked arithmetic, compute:

```text
r_upper = ceil((D-q_min)*D/q_min)
payment_upper = ceil(balance_in*r_upper/D)
```

The ratio numerator fits below `2^124`; its final reserve multiplication is widened. Check the ceiling itself and reject if either `balance_in+payment_upper` or `vault_collateral+payment_upper` cannot fit `u128`. Check fee/counter bounds as well. This is a conservative way to establish that the raw exact-output call will not reach its fit or zero-power panic. It can reject more inputs than the checked interface. The checked interface calculates the actual internal power and checks the payment directly instead. This alternative does not establish custody safety.

Host fixtures verify finite examples. The hand-derived power and formation bounds support the policy but still require mathematical review. Guest parity verifies the same integer outputs on the selected guest toolchain. A real standalone proof verifies that guest computation; it does not implement or prove weighted sale custody, clock integration, stable errors or full LEZ compatibility.
