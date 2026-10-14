# ADR 0011: Weighted precision

Status: Accepted (reviewed PoC baseline; unaudited)

Date: 2026-09-29

## Context

The reviewed PoC source separates public precision from the logarithm and exponential pipeline. Its historical sweep tested public scales 44, 48, 52, 56, and 60 with `LN_SCALE = 62`. It recorded green suites through 52, an economic-gate failure at 56, and pow-gate plus arithmetic-envelope failures at 60. The lower passing scales did not reduce the internal pipeline cost.

At scale 52, the recorded directional `pow_up` and `pow_down` error was 2 ulps against a 4-ulp bound: one quantization ulp plus `BUDGET_ULPS = 3`. The retained oracle fixtures let a later scale sweep use the same high-precision values. These are historical measurements, not verification performed for this ADR.

## Decision

Keep public scale `2^52` and internal scale `2^62`. The historical sweep selected 52 as the finest passing public scale that the 62-bit pipeline could support within its stated error and arithmetic envelope.

Keep `1 - power` at internal scale until the final widened multiply. The baseline uses the exact integer subtraction `2^62 - exp_result` and a pool-safety pad of `2^-53`. Rounding the small complement to public scale before the payout multiply would lose useful precision. Keeping it at internal scale avoids a conditional `expm1` branch; the public `expm1` function is not a swap-path branch.

## Consequences

A change to either scale or the cancellation path requires new evidence for the error and overflow bounds. The oracle retains historical sweep inputs. The current [error analysis](../../packages/weighted-math/docs/error-analysis.md) corrects the baseline's truncation and scale claims, and [qualification evidence](../../packages/weighted-math/docs/qualification.md) records new scale-52 checks. This decision remains unaudited and does not claim precision for a production weighted sale or another implementation.

## Sources

- [Error analysis](../../packages/weighted-math/docs/error-analysis.md)
- [Oracle fixtures and regeneration](../../packages/weighted-math/crates/harness/oracle/README.md)
- [0003-fixed-point-scale.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0003-fixed-point-scale.md)
- [0005-expm1-boundary.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0005-expm1-boundary.md)
