# ADR 0012: Weighted series

Status: Accepted (reviewed PoC baseline; unaudited)

Date: 2026-09-29

## Context

The reviewed PoC normalizes the logarithm input to `m` in `[sqrt(2)/2, sqrt(2))` and forms `t = (m - 1) / (m + 1)`, where `|t| <= 0.1716`. It uses the odd atanh series `ln(m) = 2t(1 + t^2/3 + t^4/5 + ...)`. Its historical bounds specify 13 logarithm terms at scale `2^62` with a tail below `2^-67`, and 20 exponential terms with a tail below `2^-66`.

The recorded truncation tails are already below Horner rounding noise. The PoC therefore chose simple series constants over a minimax fit. Precomputed constants also avoid costly per-term divisions. On RV32IM, the `u128` division runs as a software loop.

## Decision

Keep the atanh logarithm and alternating Taylor exponential series at internal scale `2^62`. Reduce the exponential argument by `ln(2)` and evaluate `exp(-s)` for `s` in `[0, ln(2))`. Use Horner evaluation with precomputed reciprocal constants for the logarithm and signed constants `(-1)^i round(2^62 / i!)` for the exponential. These constants can be checked directly without a polynomial-fitting tool.

One power uses one integer division, to form `t`. Range reduction uses a precomputed reciprocal of `ln(2)`, and the polynomial terms do not divide. The series therefore avoid the repeated divisions used by the compared Balancer-shaped method.

## Consequences

Any change to a term count, constant, or evaluation order needs new accuracy and cycle evidence. The retained cycle table contains historical guest measurements and the series source records the historical truncation bounds. This unaudited decision does not make a performance or correctness claim for a production weighted sale.

## Sources

- [Historical guest cycle measurements](../../packages/weighted-math/docs/zkvm-cycles.md)
- [Error analysis](../../packages/weighted-math/docs/error-analysis.md)
- [0004-series-choice.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0004-series-choice.md)
