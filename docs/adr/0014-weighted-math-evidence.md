# ADR 0014: Weighted math evidence

Status: Accepted (reviewed PoC baseline; unaudited)

Date: 2026-09-29

## Context

The reviewed PoC harness defines `BOUND = quant_floor(SCALE) + BUDGET`. Directional wrapper checks require error in `[0, BOUND]`: upward results stay at or above truth, and downward results stay at or below truth. The oracle fixture form is `floor(value * 2^128)`. Source 0001 reports that captured Balancer outputs passed its mpmath comparator before the PoC kernel existed. This is historical comparator evidence, not a new test result.

The invariant referee uses exact big-integer arithmetic for small integer weights. It compares `(b_in + in)^w_in * (b_out - out)^w_out >= b_in^w_in * b_out^w_out`. Equality is allowed for zero-fee math.

## Decision

Keep mpmath fixtures as the accuracy reference and Balancer as a secondary comparator, not a bit-identical target. Retain directional gates, the independent exact invariant referee, property tests and the bounded fuzz target. Keep host/guest parity as a separate check of deterministic execution. Preserve these tools with the copied kernel.

## Consequences

These records and the reviewed guest-parity evidence are historical evidence, not tests rerun here. Neither kernel parity nor a passing harness proves production LBP integration, production proving readiness or an audit. Record ignored tests and fuzz runs separately.

## Sources

- [Test harness architecture](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0001-test-harness-architecture.md)
- [Invariant referee](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0007-invariant-referee.md)
