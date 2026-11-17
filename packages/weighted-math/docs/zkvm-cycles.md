# Historical weighted-math guest cycles

Measured on 2026-07-14 with the RISC0 3.0.5 executor, guest Rust 1.94.1 and `SCALE = 52`. The guest used a release build with `overflow-checks = true`. No proof was generated. These are historical standalone-kernel measurements for RFP-016 P3, not current LEZ compute units or full-sale costs. Use the [guest guide](../zkvm/README.md) for current commands and [qualification](qualification.md) for later evidence.

The original command, run from `packages/weighted-math`, was `cd zkvm && cargo run --release`. Use the guest guide for the current locked command and tool selection.

## Host/guest parity

The kernel compiled unchanged for the RV32IM guest. All 344 evaluations matched host outputs: 87 power cases through each of `pow`, `pow_up` and `pow_down`, 11 `ln`, 12 `exp`, 12 `expm1`, 29 `calc_out_given_in` and 19 `calc_in_given_out`. Parity checks integer agreement. Independent oracle checks in `crates/harness` assess numerical accuracy; parity does not replace them. The executor exits with an error on a mismatch.

Repeated runs produced byte-identical cycle output.

## Per-call cycles

These are RISC0 user cycles measured with `env::cycle_count()` deltas, minus the 98-cycle measurement baseline. They do not establish a LEZ fee price. Costs vary with input: normalization shifts, exponential underflow and the zero-payout guard.

| op | cases | net median | min | max | u128 divisions |
| --- | ---: | ---: | ---: | ---: | ---: |
| `pow` | 87 | 8,010 | 3,059 | 9,214 | 1 |
| `pow_up` / `pow_down` | 87 each | 8,043 | 3,092 | 9,247 | 1 |
| `ln` | 11 | 4,608 | 2,722 | 5,023 | 1 |
| `exp` | 12 | 4,139 | 57 | 4,140 | 0 |
| `expm1` | 12 | 4,169 | 56 | 4,170 | 0 |
| `calc_out_given_in` | 29 | 11,522 | 721 | 11,950 | 3 |
| `calc_in_given_out` | 19 | 14,195 | 10,974 | 14,913 | 4 |

The one-shot session measurements include startup and I/O for the batch-median case. Measured session overhead was about 4,080 cycles above the call itself.

| program | user cycles | padded segment cycles | segments |
| --- | ---: | ---: | ---: |
| one `pow` | 12,191 | 65,536 (2^16 minimum) | 1 |
| one exact-input trade | 15,702 | 65,536 (2^16 minimum) | 1 |
| one exact-output trade | 18,375 | 65,536 (2^16 minimum) | 1 |

The historical buy session used 24% of `2^16`; the kernel call used about 1.1% of a default `2^20` segment. The original batch estimate was about 90 kernel buys per default `2^20` segment: `(1,048,576 - 4,082) / 11,620`. This estimate excludes sale state, custody and platform costs.

## Division costs

On RV32IM, `u128` division uses a `compiler_builtins` software loop. The measured cost depends on operand shape, shown as numerator bits / denominator bits.

| shape | net cycles |
| --- | ---: |
| 124 / 64 | 2,623 |
| 127 / 75 | 2,500 |
| 127 / 65 | 2,392 |
| 91 / 61 | 1,471 |
| 117 / 64 | 351 |
| 51 / 31 | 491 |

Costs vary by almost 8x. Wide-quotient divisions at the measured kernel magnitudes cost about 2.4-2.6k cycles each.

The reviewed code uses these division counts:

- `pow` and its wrappers: one division for `t = (m-1)/(m+1)` in `ln_inner`, about 2.5k of 8.0k cycles (31%).
- `exp` / `expm1`: none. Range reduction multiplies by a precomputed `1/ln2`.
- `calc_out_given_in`: three, for the exponent, `ratio_up` and `pow`; about half the 11.5k-cycle call.
- `calc_in_given_out`: four, for the exponent, `ratio_down`, `pow` and `(1-p)/p` inversion.

The 20-term exponential Horner loop cost about 4.1k cycles, or about 210 cycles per 128-bit multiply-shift-add step. [ADR 0012](../../../docs/adr/0012-weighted-series.md) selects the term count for error bounds. A cost change needs a new accuracy review.

The historical Balancer-style comparison estimated about 60k cycles for a couple dozen divisions at 2.5k each, roughly five times the measured exact-input path. This is an estimate, not a measured Balancer guest result. See [ADR 0010](../../../docs/adr/0010-weighted-integer-arithmetic.md).

## Limits

Executor user cycles are not proof time. Padded segment cycles describe proof workload. The wrapper uses `env::read_slice` and commits raw words; it does not model LEZ serialization or state access. Circuit and toolchain changes can change costs. Run the harness again before using these numbers for a new build.
