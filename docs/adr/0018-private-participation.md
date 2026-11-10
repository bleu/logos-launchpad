# ADR 0018: One transaction for private trades

Status: Product and recovery rules accepted. Public allowlist admission has signed development-execution evidence; private-route platform, wallet and proof support still need verification.

Date: 2026-09-29

## Context

The private route uses a public pool. The current curve-buy PoC does not complete the required flow. See the [research note](../research/private-route-transaction-patterns.md).

## Changes

- 2026-09-30: Replaced the three-transaction flow with one transaction.
- 2026-10-01: Added complete private returns, participant-controlled destinations and recovery rules. Allowlist rules are in [ADR 0021](0021-modes-and-open-limits.md).
- 2026-11-09: ADR 0021 extends optional buy allowlists to all pricing engines and replaces the common collateral cap and one-buy allowance with repeat authorized purchases. It required private approved-account support, superseded below; local proving, atomic returns and recovery rules remain.
- 2026-11-10: ADR 0021 replaces private approved-account enrollment with pre-approved public trading-account batches. The wallet prefers an unused approved address per attempt, without an on-chain one-buy restriction. Private funding and complete private returns remain required; no private account IDs or keys go to the creator.

## Decision

Keep public trades and an SDK-managed private route for bonding-curve buys, allowed bonding-curve sells, and fixed-price and weighted-sale buys. The wallet prefers fresh public accounts; an allowlisted buy selects an unused pre-approved public trader unless the participant deliberately accepts address reuse. Each private attempt uses one transaction to:

1. Move private input and gas funds to those accounts.
2. Trade within the participant's limits.
3. Return received tokens, unused input and unused gas to private accounts controlled by the participant. Actual fees are excluded.

All three steps must finish together or fail together. Do not split them or report success with user funds left public. Return accounts must support the asset and its current private state. Reject accounts controlled by someone else or whose control cannot be checked. Different assets may need different account types.

The SDK prepares quotes, checks limits and destinations, builds the transaction and tracks progress. CLI and QML use the same rules. The wallet provider handles account access, approval, signing and submission. User keys stay outside the shared core, and proving runs in a protected participant-local wallet/proving boundary, without remote proving or remote key custody. LP-0021 support still needs verification; see [ADR 0029](0029-wallet-integration.md).

For allowlisted buys, both funding routes must prove membership and control of the actual approved public trader, bound to the purchase. Participants can precompute public-account batches and send only addresses to the creator. There is no separate approval-only or private checking account. The creator can link batch addresses, and public reuse links purchases; fresh addresses do not guarantee unlinkability. [ADR 0021](0021-modes-and-open-limits.md) owns fixed-list delivery and repeat-buy policy. Removing sale allowance state does not remove signatures, nonces or native private-state nullifiers, and transaction replay must not create another trade.

## Recovery and privacy

- Save progress before calling any operation that can send the transaction.
- A timeout, lost reply or app closure does not prove failure. Keep the attempt pending and check its result. Do not send a replacement while the result is unknown.
- A confirmed trade must not be repeated because wallet updates are delayed.
- After a known rejection, prepare again with current state, a fresh quote and explicit limits. Changed transaction contents need new approval.
- Keep minimal progress on the participant's device. Do not put private-account IDs, keys, balances, membership secrets or proof secrets in core records, logs or events.

Explain public effects before transaction approval. How often to require a separate warning acknowledgement remains undecided.

## Limits and remaining work

Pool changes, trade amounts and timing remain public. These SDK rules do not guarantee privacy for callers who build other transactions. A failed execution may still cost fees; no automatic refund or reversal is promised.

Whole-route execution, complete returns, wallet recovery and all private trade forms still need implementation and tests. Public allowlist admission is implemented for all three engines under [ADR 0021](0021-modes-and-open-limits.md), with signed authority and replay checks; that evidence does not establish private-funded buys. The [full specification](../architecture/private-participation.md) defines the detailed rules and required test cases.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
- [Earlier curve PoC decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0005-private-trade-boundary-and-verification.md)
