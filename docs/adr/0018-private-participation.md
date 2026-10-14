# ADR 0018: One transaction for private trades

Status: Product and recovery rules accepted. Platform, wallet and allowlist proofs still need verification.

Date: 2026-09-29

## Context

The private route uses a public pool. The current curve-buy PoC does not complete the required flow. See the [research note](../research/private-route-transaction-patterns.md).

## Changes

- 2026-09-30: Replaced the three-transaction flow with one transaction.
- 2026-10-01: Added complete private returns, participant-controlled destinations and recovery rules. Fixed-price and weighted-sale allowlist rules are in [ADR 0021](0021-modes-and-open-limits.md).

## Decision

Keep public trades and an SDK-managed private route for bonding-curve buys, allowed bonding-curve sells, and fixed-price and weighted-sale buys. Each private attempt uses fresh public accounts and one transaction to:

1. Move private input and gas funds to those accounts.
2. Trade within the participant's limits.
3. Return received tokens, unused input and unused gas to private accounts controlled by the participant. Actual fees are excluded.

All three steps must finish together or fail together. Do not split them or report success with user funds left public. Return accounts must support the asset and its current private state. Reject accounts controlled by someone else or whose control cannot be checked. Different assets may need different account types.

The SDK prepares quotes, checks limits and destinations, builds the transaction and tracks progress. CLI and QML use the same rules. The wallet provider handles account access, approval, signing and submission. User keys stay outside the shared core. LP-0021 support still needs verification; see [ADR 0029](0029-wallet-integration.md).

## Recovery and privacy

- Save progress before calling any operation that can send the transaction.
- A timeout, lost reply or app closure does not prove failure. Keep the attempt pending and check its result. Do not send a replacement while the result is unknown.
- A confirmed trade must not be repeated because wallet updates are delayed.
- After a known rejection, prepare again with current state, a fresh quote and explicit limits. Changed transaction contents need new approval.
- Keep minimal progress on the participant's device. Do not put private-account IDs, keys, balances, membership secrets or proof secrets in core records, logs or events.

Explain public effects before transaction approval. How often to require a separate warning acknowledgement remains undecided.

## Limits and remaining work

Pool changes, trade amounts and timing remain public. These SDK rules do not guarantee privacy for callers who build other transactions. A failed execution may still cost fees; no automatic refund or reversal is promised.

Whole-route execution, complete returns, wallet recovery, private bonding-curve sells, fixed-price and weighted-sale buys, and allowlist proofs still need implementation and tests. The [full specification](../architecture/private-participation.md) defines the detailed rules and required test cases.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
- [Earlier curve PoC decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0005-private-trade-boundary-and-verification.md)
