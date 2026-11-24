# ADR 0018: One transaction for private trades

Status: Accepted design; private-route platform, wallet and proof support pending.

## Decision

Provide public trades and an SDK-managed private route for exact-input and exact-output buys in bonding-curve, fixed-price and weighted sales. Include exact-input sells in all three sale types when `BuyAndSell` permits them. In one transaction, move private input and gas to public trading accounts, trade within explicit limits, then return received tokens, unused input and unused gas to participant-controlled private accounts. Exclude actual fees from returns.

All steps must succeed or fail together. Do not report success with participant funds left public. Check control and asset/private-state support for each return account. Different assets can require different account types.

The SDK owns quotes, limits, destinations, transaction construction and progress. CLI and QML use that logic. The wallet owns account access, approval, signing and submission. Keys stay outside the shared core. Proving stays in a protected participant-local wallet/proving boundary, without remote proving or key custody. [Wallet support](0029-wallet-integration.md) remains unverified.

Both funding routes must prove membership and control of the actual approved public trader, bound to the purchase. Participants send only public-address batches to the creator, not keys or private funding/return IDs. There is no separate approval-only account. The wallet prefers an unused approved address per attempt; deliberate reuse requires a privacy warning. [ADR 0021](0021-modes-and-open-limits.md) owns fixed-list and repeated-purchase rules. Removing allowances does not remove signatures, nonces or native private-state nullifiers. Replay must not create another trade.

## Recovery and privacy

Save minimal participant-local progress before any operation that can send the transaction. A timeout, lost reply or app closure does not prove failure. Keep the attempt pending and reconcile its result; do not send a replacement while the result is unknown. Do not repeat a confirmed trade because wallet updates lag. After a known rejection, use current state, a fresh quote and explicit limits. Changed transaction contents need new approval.

Do not store private-account IDs, keys, balances, membership secrets or proof secrets in core records, logs or events. Explain public effects before approval. The creator can link batch addresses; reuse links purchases. Fresh addresses do not guarantee unlinkability. The frequency of separate warning acknowledgement remains open.

## Reason and limits

Atomic returns prevent a successful trade from leaving funds public. Pool changes, amounts and timing remain public. These SDK rules do not guarantee privacy for other callers. Failed execution can still cost fees; no automatic refund or reversal is promised.

Whole-route execution, complete returns, wallet recovery and private trade forms still need implementation and tests. Signed public allowlist execution does not prove private-funded buys or production readiness. See the [participation contract](../architecture/private-participation.md) and [public evidence](../architecture/allowlist-proof.md#public-execution-evidence).

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
- [Earlier curve PoC decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0005-private-trade-boundary-and-verification.md)
