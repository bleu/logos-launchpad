# ADR 0012: Weighted series

Status: Accepted (reviewed PoC baseline; unaudited)

Date: 2026-09-29

## Context

The reviewed PoC record selected series that use one division when computing a power. It normalizes the logarithm input to `m` in `[sqrt(2)/2, sqrt(2))` and uses `t = (m - 1) / (m + 1)`, with `|t| <= 0.1716`. The logarithm is the odd atanh series `ln(m) = 2t(1 + t^2/3 + t^4/5 + ...)`. The record specifies 13 terms at scale `2^62` and a truncation tail below `2^-67`.

## Decision

Record the companion exponential method as range reduction by `ln(2)` followed by an alternating Taylor series on `s` in `[0, ln(2))`. The cited source specifies 20 signed constants `(-1)^i round(2^62 / i!)` and truncation below `2^-66`. Both polynomials use Horner evaluation and precomputed reciprocal constants. The source attributes exactly one hardware division per power to formation of `t`.

## Consequences

The stated term counts, domains, and error bounds are source-recorded PoC bounds. They are not independently checked here, and this ADR does not make a general performance or correctness claim.

## Sources

- [0004-series-choice.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0004-series-choice.md)
