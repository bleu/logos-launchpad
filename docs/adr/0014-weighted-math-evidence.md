# ADR 0014: Weighted math evidence

Status: Accepted (reviewed PoC baseline; unaudited)

## Decision

Keep high-precision mpmath fixtures as the accuracy reference. Directional checks require error in `[0, BOUND]`, with `BOUND = quant_floor(SCALE) + BUDGET`. A wrong-side result is a fund-safety failure regardless of size. A right-side result outside the bound is a magnitude failure.

Keep fixture checks, property tests, bounded fuzzing and host/guest parity separate. Fixtures retain decimal truth and `floor(value * 2^128)` so Rust can rescale them. Balancer captures check the comparator; they are not the accuracy reference or a bit-identical target.

Use harness-only big integers for the invariant referee, independent of kernel arithmetic. Check `(b_in + in)^w_in * (b_out - out)^w_out >= b_in^w_in * b_out^w_out`; equality is valid for zero-fee math. Integer referee weights in `1..=99` make exact powers practical but cover less than the kernel's weight range. `f64` cannot detect all one-unit losses.

## Reason and limits

Independent calculations reduce the chance of repeating kernel errors. Parity proves deterministic execution, not numerical truth or sale integration. [Qualification](../../packages/weighted-math/docs/qualification.md) records scale-52 rational guards, rejection counts, expanded fixtures and one real standalone proof. That proof does not establish full weighted-sale or LEZ compatibility. Report new checks, ignored tests and fuzz runs separately. The kernel is unaudited.

## Sources

- [Oracle fixtures and regeneration](../../packages/weighted-math/crates/harness/oracle/README.md)
- [Balancer comparison and source notices](../../packages/weighted-math/crates/harness/balancer-ref/README.md)
- [Guest-parity setup](../../packages/weighted-math/zkvm/README.md)
- [Overflow envelope](../../packages/weighted-math/docs/overflow-proof.md)
- [Test harness architecture](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0001-test-harness-architecture.md)
- [Invariant referee](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0007-invariant-referee.md)
