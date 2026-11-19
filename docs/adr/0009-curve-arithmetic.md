# ADR 0009: Checked curve pricing and reserve solvency

Status: Accepted as the reviewed PoC baseline

## Decision

Use checked `u128` constant-product arithmetic with pool-favouring rounding. Each initial virtual reserve must be positive and below `2^64`. Save their product as immutable `initial_k`; initialize current `k` to it.

Virtual reserves set prices but cannot back withdrawals. Cap output by the matching real reserve and exclude accrued fees from pricing reserves. Check arithmetic at runtime; creation bounds do not prove every trade safe.

Factory curve sales require `sell_fee_bps == 0` at creation and on saved-state validation. Their `k` must equal `initial_k` through trading and settlement. This preserves the proposal's fixed pricing invariant and avoids an additional exit charge without an established product need. Namespace fees remain separate from reserves.

Direct curve pools remain internal primitives that can use retained fees. Quotes use current `k`. Retained sell collateral `r` increases it by `k_after = k_before + virtual_token_after * r`, with checked arithmetic. Namespace fees do not increase `k`. At zero retained sell-fee rate, `k == initial_k`. The virtual-reserve product stays at least current `k`; rounding can make that product decrease between trades.

## Reason and limits

Separate real backing and pricing state prevent virtual liquidity from funding withdrawals. A state snapshot cannot prove the history of retained fees or `k` updates. See the [implementation](../../crates/pool/src/lib.rs), [invariant tests](../../crates/pool/tests/proptest_invariants.rs) and [evidence limits](../product/curve-contract.md#acceptance-evidence).

## Sources

- [Curve PoC bounds ADR](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0004-u128-bounds-for-the-curve-arithmetic.md)
- [Curve PoC revised fee/invariant ADR](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0005-dual-input-fees-and-monotonic-reserve-product.md)
