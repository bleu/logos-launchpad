# ADR 0014: Weighted math evidence

Status: Accepted (reviewed PoC baseline; unaudited)

Date: 2026-09-29

## Context

The reviewed PoC harness defines `BOUND = quant_floor(SCALE) + BUDGET`. Directional wrapper checks require error in `[0, BOUND]`: upward results stay at or above truth, and downward results stay at or below truth. A wrong-side result is a fund-safety failure regardless of its size; a right-side result outside the bound is a magnitude failure.

The power fixtures store high-precision decimal values and `floor(value * 2^128)` values. Rust rescales the latter at test time, so the fixtures can be reused across scale sweeps. The historical harness record reports that captured Balancer outputs passed the mpmath comparator before the PoC kernel existed. Checking those outputs against Balancer's own tolerance tests the grader with an independent implementation. Balancer is not the kernel's accuracy reference or a bit-identical target.

## Decision

Keep mpmath fixtures as the accuracy reference, directional gates, property tests, the bounded fuzz target, and host/guest parity as separate checks. Retain the committed fixtures and their oracle, Balancer-capture, and guest-parity tools with the copied kernel.

Keep the invariant referee independent from kernel arithmetic by using harness-only big-integer calculations. Reusing the kernel could repeat its errors, while `f64` can miss one-unit losses at these magnitudes. The referee uses integer weights in `1..=99` to make exact powers practical. That test restriction is narrower than the kernel's accepted weight range.

## Consequences

The exact referee checks `(b_in + in)^w_in * (b_out - out)^w_out >= b_in^w_in * b_out^w_out`; equality is valid for zero-fee math. Host/guest parity checks deterministic execution, not numerical truth, LBP integration, production proving readiness, or an audit. The imported comparator and parity results are historical evidence. [Current qualification](../../packages/weighted-math/docs/qualification.md) records scale-52 rational trade guards, explicit rejection counts, expanded mixed-decimal/large-weight fixtures and one real standalone proof. That proof does not establish full LBP or LEZ compatibility. Report new checks, ignored tests and fuzz runs separately.

## Sources

- [Oracle fixtures and regeneration](../../packages/weighted-math/crates/harness/oracle/README.md)
- [Balancer comparison and source notices](../../packages/weighted-math/crates/harness/balancer-ref/README.md)
- [Guest-parity setup](../../packages/weighted-math/zkvm/README.md)
- [Overflow envelope](../../packages/weighted-math/docs/overflow-proof.md)
- [Test harness architecture](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0001-test-harness-architecture.md)
- [Invariant referee](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0007-invariant-referee.md)
