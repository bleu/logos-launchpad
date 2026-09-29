# ADR 0013: Weighted exact output

Status: Accepted (reviewed PoC baseline; unaudited)

Date: 2026-09-29

## Context

The reviewed PoC record keeps exact-output pricing in the base interval `(0,1)`. Its inversion is `(b / (b - a))^y - 1 = (1 - p) / p`, where `p = ((b - a) / b)^y`. It records a downward-padded internal power, a ceiling division for `(1 - p) / p`, and an upward-rounded final widened multiply. The source bounds the division numerator by `2^124` and states that it needs no widening.

## Decision

Record the PoC limit `amount_out <= 30%` of the reserve. With exponent at most 99, the cited bound is `p >= 0.7^99 ~= 2^-50.6`, which the source describes as four times the internal pad floor. The cap prevents the denominator in `(1 - p) / p` from collapsing. Payments that do not fit `u128` are outside the stated representability envelope and halt rather than wrap.

## Consequences

The 30% limit is a documented precondition, not a general price-quality guarantee. The cited source also reports direction-only treatment at the 30% and exponent-99 corner because its sensitivity exceeds `u128`. This ADR records that historical baseline without new verification or a claim about program semantics.

## Sources

- [0006-in-given-out-inversion.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0006-in-given-out-inversion.md)
