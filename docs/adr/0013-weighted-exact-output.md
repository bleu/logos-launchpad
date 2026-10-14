# ADR 0013: Weighted exact output

Status: Accepted (reviewed PoC baseline; unaudited)

Date: 2026-09-29

## Context

The reviewed PoC keeps power inputs in `(0,1)`. The direct exact-output form would require a base greater than 1. Its inversion instead uses `(b / (b - a))^y - 1 = (1 - p) / p`, where `p = ((b - a) / b)^y` and the power base remains in `(0,1)`.

The source records a downward-rounded base, an upward-rounded exponent, a downward-padded internal power, a ceiling division for `(1 - p) / p`, and an upward-rounded final widened multiply. Each direction overstates the payment. The division numerator is bounded by `2^124` in the historical envelope and needs no widening.

## Decision

Use the inverted form for exact-output pricing. Do not add a base-greater-than-1 power path. Cap `amount_out` at 30% of the reserve. At exponent 99, the stated bound `p >= 0.7^99 ~= 2^-50.6` keeps the inversion denominator above the internal pad floor.

The 30% cap and payment representability are separate bounds. The cap protects the power and division domain. Even within that cap, a payment can exceed `u128`; the final fit assertion rejects that result and halts rather than wraps.

## Consequences

The cap is a precondition for this exact-output path, not a general price-quality guarantee. The retained overflow envelope documents the separate result-fit boundary. The 30% and exponent-99 corner has historical direction-only evidence because its sensitivity exceeds `u128`. This unaudited decision does not establish production weighted-sale behavior.

## Sources

- [Overflow envelope](../../packages/weighted-math/docs/overflow-proof.md)
- [Oracle fixtures and regeneration](../../packages/weighted-math/crates/harness/oracle/README.md)
- [0006-in-given-out-inversion.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0006-in-given-out-inversion.md)
