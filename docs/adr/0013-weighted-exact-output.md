# ADR 0013: Weighted exact output

Status: Accepted (reviewed PoC baseline; unaudited)

## Decision

Use `(b / (b - a))^y - 1 = (1 - p) / p`, where `p = ((b - a) / b)^y`, for exact output. Keep power bases in `(0,1)`; do not add a base-greater-than-1 path.

Round the base down, exponent up and padded internal power down. Use ceiling division for `(1 - p) / p` and round the final widened multiply up. Each direction overstates payment. Cap output at 30% of the reserve. At exponent 99, `p >= 0.7^99 ~= 2^-50.6` keeps the denominator above the internal pad floor. The division numerator is bounded by `2^124`.

## Reason and limits

The cap protects the power and division domain; it does not guarantee price quality or payment fit. Even within the cap, payment can exceed `u128`; reject that result rather than wrap. The 30% and exponent-99 corner has direction-only evidence because its sensitivity exceeds `u128`. See the [overflow envelope](../../packages/weighted-math/docs/overflow-proof.md). The kernel is unaudited.

## Sources

- [Overflow envelope](../../packages/weighted-math/docs/overflow-proof.md)
- [Oracle fixtures and regeneration](../../packages/weighted-math/crates/harness/oracle/README.md)
- [0006-in-given-out-inversion.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0006-in-given-out-inversion.md)
