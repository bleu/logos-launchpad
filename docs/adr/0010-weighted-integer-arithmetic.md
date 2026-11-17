# ADR 0010: Weighted integer arithmetic

Status: Accepted (reviewed PoC baseline; unaudited)

## Decision

Keep balances and amounts as raw `u128`, with `i128`/`u128` kernel arithmetic. Fixed-point values hold the power base in `(0,1)` and weight exponent in approximately `[0.0101, 99]`, not scaled balances.

Each swap path has one outer `128 x 128 -> 256` result multiply: payout for exact input or payment for exact output. The shifted result must fit `u128`; otherwise halt rather than wrap. The widening helper also supports the power argument and public fixed-point multiply helpers. Raw pricing paths use no 256-bit division.

The checked-quote layer uses bounded wide products and division for accuracy checks and exact rational references. Its results must fit `u128`. It does not change the raw kernel or add a general big-integer dependency.

## Reason and limits

A balance of `1e27` at scale `2^52` exceeds `i128`. Local widening avoids a whole-kernel 256-bit representation. Keep the [overflow envelope](../../packages/weighted-math/docs/overflow-proof.md) and fit checks. [Recorded guest costs](../../packages/weighted-math/docs/zkvm-cycles.md) do not measure checked-quote cost or establish production performance. The kernel is unaudited.

## Sources

- [Overflow envelope](../../packages/weighted-math/docs/overflow-proof.md)
- [Historical guest cycle measurements](../../packages/weighted-math/docs/zkvm-cycles.md)
- [0002-u128-balances.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0002-u128-balances.md)
- [0008-integer-width.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0008-integer-width.md)
