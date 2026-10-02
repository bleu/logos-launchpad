# ADR 0009: Checked curve pricing and reserve solvency

Status: Accepted as the reviewed PoC baseline

Date: 2026-09-29

## Decision

Retain checked `u128` constant-product arithmetic with pool-favouring rounding. Each initial virtual reserve must be positive and below `2^64`, so their product fits `u128`. Save that product as immutable `k`; fees do not increase it.

Virtual reserves set prices but cannot back withdrawals. Cap output by the matching real reserve and keep accrued fees outside pricing reserves. Check runtime arithmetic for overflow; creation bounds alone do not prove every trade safe.

This supersedes the earlier dual-input-fee/monotonic-product interpretation. The current virtual-reserve product stays at least `k`, but can decrease because later quotes use fixed `k`. See the [pool implementation](../../crates/pool/src/lib.rs), [solvency tests](../../crates/pool/tests/proptest_invariants.rs) and [evidence limits](../product/curve-contract.md#acceptance-evidence).

## Sources

- [Curve PoC bounds ADR](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0004-u128-bounds-for-the-curve-arithmetic.md)
- [Curve PoC revised fee/invariant ADR](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0005-dual-input-fees-and-monotonic-reserve-product.md)
