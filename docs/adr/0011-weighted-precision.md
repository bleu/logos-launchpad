# ADR 0011: Weighted precision

Status: Accepted (reviewed PoC baseline; unaudited)

Date: 2026-09-29

## Context

The reviewed PoC source separates public precision from the logarithm and exponential pipeline. It reports `SCALE = 52` and `LN_SCALE = 62`. The historical sweep tested scales 44, 48, 52, 56, and 60. It reports green suites through 52, an economic-gate failure at 56, and pow-gate plus arithmetic-envelope failures at 60. At scale 52, the reported directional `pow_up` and `pow_down` error was 2 ulps against a 4-ulp bound, where the bound is one quantization ulp plus `BUDGET_ULPS = 3`.

## Decision

Keep the documented baseline at public scale `2^52` with internal scale `2^62`. The cited cancellation record retains `1 - power` at internal scale for the final widened multiply. It specifies an exact integer subtraction, `2^62 - exp_result`, and a pool-safety pad of `2^-53`; it specifies no conditional `expm1` boundary.

## Consequences

These sweep results and error values are historical measurements from the reviewed PoC records, not new verification. The record gives no audit conclusion and makes no claim about another program's precision or execution path.

## Sources

- [0003-fixed-point-scale.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0003-fixed-point-scale.md)
- [0005-expm1-boundary.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0005-expm1-boundary.md)
