# ADR 0010: Weighted integer arithmetic

Status: Accepted (reviewed PoC baseline; unaudited)

Date: 2026-09-29; checked-quote amendment: 2026-10-29

## Context

The reviewed PoC record rejected fixed-point balances at the interface. A reserve of `1e27` wei at `2^52` scale is about `4.5e42`, which exceeds the `i128` maximum of about `1.7e38`. Balances and amounts therefore stay as raw `u128`; fixed-point values are limited to `base = balance_in / (balance_in + amount_in)` in `(0,1)` and a weight exponent in approximately `[0.0101, 99]`.

The historical guest measurements identify `u128` division as a software operation on RV32IM. Promoting the whole kernel to 256-bit integers would add emulated-width cost and require new bounds for wider intermediate values. The reviewed baseline instead keeps the proof focused on raw amounts and specific widened products.

## Decision

Keep `i128`/`u128` arithmetic in the kernel. Each swap path has one outer `128 x 128 -> 256` result multiply: the payout for exact input and the payment for exact output. The shifted result must fit `u128`; otherwise the operation halts rather than wraps.

The widening helper also supports the power argument product and public fixed-point multiply helpers for uniformity and scale-sweep headroom. Those uses do not promote the kernel to general 256-bit arithmetic, and the raw pricing paths use no 256-bit division.

The checked-quote layer is an exception to the earlier rule against 256-bit division. It uses bounded wide products and division to check accuracy and exact rational references. Its results must fit `u128`. It has no general big-integer dependency and does not change the raw pricing kernel. This amendment supersedes only the division restriction for these checks. Guest costs remain unmeasured.

## Consequences

The kernel has no general big-integer dependency or whole-kernel 256-bit representation. The retained overflow envelope states the input bounds and fit checks for this baseline. The cycle data is historical measurement evidence only. This unaudited decision does not establish behavior for a production weighted sale or another program.

## Sources

- [Overflow envelope](../../packages/weighted-math/docs/overflow-proof.md)
- [Historical guest cycle measurements](../../packages/weighted-math/docs/zkvm-cycles.md)
- [0002-u128-balances.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0002-u128-balances.md)
- [0008-integer-width.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0008-integer-width.md)
