# ADR 0009: Checked curve pricing and reserve solvency

Status: Accepted as the reviewed PoC baseline

Date: 2026-09-29

## Decision

Retain checked `u128` constant-product arithmetic with pool-favouring rounding. Each initial virtual reserve must be positive and below `2^64`, so their product fits `u128`. Save that product as immutable `initial_k` and initialize current `k` to it. The retained-sell-fee amendment below replaces the fixed-`k` rule.

Virtual reserves set prices but cannot back withdrawals. Cap output by the matching real reserve and keep accrued fees outside pricing reserves. Check runtime arithmetic for overflow; creation bounds alone do not prove every trade safe.

Quotes use current `k`. A retained collateral amount `r` increases it by `k_after = k_before + virtual_token_after * r`, with checked arithmetic. Namespace fees remain separate and do not increase it. With zero retained sell-fee rate, `k == initial_k`. The virtual-reserve product stays at least current `k`; rounding can make that product decrease between trades. A state snapshot cannot prove the history of retained fees or `k` updates. See the [pool implementation](../../crates/pool/src/lib.rs), [solvency tests](../../crates/pool/tests/proptest_invariants.rs) and [evidence limits](../product/curve-contract.md#acceptance-evidence).

## Sources

- [Curve PoC bounds ADR](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0004-u128-bounds-for-the-curve-arithmetic.md)
- [Curve PoC revised fee/invariant ADR](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0005-dual-input-fees-and-monotonic-reserve-product.md)
