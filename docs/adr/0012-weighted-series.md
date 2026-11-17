# ADR 0012: Weighted series

Status: Accepted (reviewed PoC baseline; unaudited)

## Decision

Use the atanh logarithm and alternating Taylor exponential at internal scale `2^62`. Normalize the logarithm input to `m` in `[sqrt(2)/2, sqrt(2))`; form `t = (m - 1) / (m + 1)`, with `|t| <= 0.1716`. Evaluate `ln(m) = 2t(1 + t^2/3 + t^4/5 + ...)` with 13 terms.

Reduce the exponential argument by `ln(2)` and evaluate `exp(-s)` for `s` in `[0, ln(2))` with 20 terms. Use Horner evaluation, precomputed reciprocal logarithm constants and signed exponential constants `(-1)^i round(2^62 / i!)`. One power uses one integer division to form `t`; range reduction uses a precomputed reciprocal.

## Reason and limits

Simple constants can be checked without a fitting tool and avoid repeated software divisions on RV32IM. Truncation bounds and rounding limits are in the [error analysis](../../packages/weighted-math/docs/error-analysis.md). Changes to term counts, constants or evaluation order require new accuracy and cycle evidence. [Recorded cycles](../../packages/weighted-math/docs/zkvm-cycles.md) are limited measurements, not a production-performance claim. The kernel is unaudited.

## Sources

- [Historical guest cycle measurements](../../packages/weighted-math/docs/zkvm-cycles.md)
- [Error analysis](../../packages/weighted-math/docs/error-analysis.md)
- [0004-series-choice.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0004-series-choice.md)
