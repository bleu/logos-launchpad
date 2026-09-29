# ADR 0010: Weighted integer arithmetic

Status: Accepted (reviewed PoC baseline; unaudited)

Date: 2026-09-29

## Context

The reviewed PoC record rejected fixed-point balances at the interface. A reserve of `1e27` wei at `2^52` scale is about `4.5e42`, which exceeds the `i128` maximum of about `1.7e38`. The cited records use raw `u128` for balances and amounts. Fixed-point values are limited to `base = balance_in / (balance_in + amount_in)` in `(0,1)` and the weight exponent in approximately `[0.0101, 99]`.

## Decision

Record the PoC baseline as `i128`/`u128` arithmetic with one localized `128 x 128 -> 256` multiply at the outer payout or payment calculation. The source places the result behind a fit assertion. A result outside `u128` halts rather than wraps. This record does not claim that another implementation has these properties.

## Consequences

The scope is deliberately small: no general big-integer dependency and no whole-kernel 256-bit promotion are part of this baseline. Overflow safety still depends on the cited proof and its input envelope. This ADR preserves design provenance only; it is not a library audit or a statement about program behavior.

## Sources

- [0002-u128-balances.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0002-u128-balances.md)
- [0008-integer-width.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0008-integer-width.md)
