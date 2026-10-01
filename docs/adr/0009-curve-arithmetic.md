# ADR 0009: Checked curve pricing and reserve solvency

Status: Accepted as the reviewed PoC baseline

Date: 2026-09-29

## Context

Virtual reserves set the curve price but cannot back token withdrawals. Fee treatment must not change the pricing invariant or make accrued fees available as swap output.

## Decision

Retain checked `u128` constant-product arithmetic with pool-favouring rounding for exact-input and exact-output swaps. The reviewed creation bound requires `0 < virtual_reserve < 2^64` for each reserve, so their creation-time product fits `u128`. Keep that product as immutable `k`; virtual reserves move with swaps, but fees do not grow `k`.

Require swap output to be covered by the matching real reserve. Keep accrued protocol fees outside pricing reserves and creator proceeds. Retain overflow checks and randomized reserve-solvency tests.

## Consequences

The earlier dual-input-fee/monotonic-product interpretation is superseded. Accepted trades keep the mathematical current virtual-reserve product at least `k`; the current product can decrease as later quotes use the fixed creation-time `k`. A creation bound alone is not proof of every production arithmetic path. The [curve contract](../product/curve-contract.md) defines quotient/remainder fee helpers, checked runtime reserve and counter arithmetic, physical custody checks, the solvency argument and the acceptance evidence map. Conservative rounding and real-output guards remain required.

## Sources

- [Curve PoC bounds ADR](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0004-u128-bounds-for-the-curve-arithmetic.md).
- [Curve PoC revised fee/invariant ADR](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0005-dual-input-fees-and-monotonic-reserve-product.md).
