# ADR 0021: Sale types, trade modes, allowlists and open limits

Status: Accepted internally; public admission implemented; wallet/private execution pending; per-block ceiling deferral: To be reviewed by Logos team.

## Sale modes

All factory engines require immutable `TradeMode::{BuyOnly, BuyAndSell}`. `BuyOnly` permits exact-input and exact-output buys with collateral and rejects sells. `BuyAndSell` also permits exact-input sells with a minimum net collateral output. Exact-output sells are unsupported. UI defaults remain open. Direct curve `allowed_input_token`, including token0-only input, is a separate primitive.

Exact-input fixed-price buys spend the full input and floor output; exact output ceils payment. Reject oversized buys in full, without partial fills or rounding-remainder refunds. Fixed-price sells floor gross collateral at the saved price, then deduct [sell fees](0015-fee-accounting.md). Weighted sells use checked reverse quotes and cannot spend the saved seed, accrued fees or surplus. Both require recorded backing for gross output and check minimum net output. See [quotes](0017-quotes-and-slippage.md).

[Setup](0032-fixed-price-lbp-setup.md) defines fixed-price start-now and scheduled bounds. Weighted sales require an explicit scheduled start, a later schedule end and canonical clock time at or before start. There is no weighted start-now option without trusted clock reads. [ADR 0026](0026-lbp-clock-and-poke.md) defines interpolation. [ADR 0025](0025-sale-closing.md) defines ending and the pending expiry-duration review.

Optional [pause](0031-shared-launch-factory.md#policy-contract) belongs to the current creator, including before start. Authenticate repeated calls; unchanged calls emit no event. Pause does not stop weights or move the end. Resume cannot reopen an ended sale or clear freeze. It grants no sale control to namespace administrators.

Allowlist admission gates buys only. A permitted sell can replenish inventory under `Scheduled`; selected depletion is final and rejects later sells. Settlement burns cannot create depletion evidence.

## Allowlist rules for all pricing engines

Use an optional immutable buy allowlist for all three engines. The fixed private list contains approved regular public LEZ trading accounts. Participants prepare batches locally and send only addresses to the creator. No keys or private funding/return IDs go to the creator. After protected initial membership-data delivery, buying must not depend on the creator's service.

Each buy proves membership and control of the actual approved trader, bound to sale and purchase. Membership data does not grant spending authority; another account in the same wallet is not approved automatically. There is no separate approval-only or private checking account.

The wallet prefers an unused approved address per attempt. On-chain approval permits repeated and independent concurrent purchases across funding routes when native state permits. There is no spending cap, purchase counter or consumed allowance. Normal balance, inventory, reserve, slippage, fee, direction, pause and ending rules still apply. An approved account can buy all available inventory.

Lock all batch addresses, context and root at initial creation. No setup stage can replace them. No list update or replenishment is supported. Capacity counts addresses, not people. When unused addresses run out, the wallet must report the limit rather than silently reuse or select an unapproved address. Deliberate reuse requires a privacy warning.

Do not publish the full roster. Used addresses and trade effects are public. The creator can link batch addresses; public reuse links purchases. Fresh addresses do not guarantee unlinkability from amounts, timing or other data. Keep private funding/return IDs private; require no persistent cross-sale participant ID. Account control does not prove human uniqueness or prevent full-key sharing.

Reject invalid or mismatched authorization. Membership reuse for a new authorized buy is permitted; copying authorization to a different purchase or transaction replay must not create a trade. Programs enforce eligibility on both routes. Signatures, nonces, native private-state nullifiers and submission recovery remain necessary. Private funding, trade and returns must commit together under [ADR 0018](0018-private-participation.md).

## Membership method and evidence

Use public SHA-256 Merkle paths of fixed depth 17, independent random 32-byte salts, randomized leaf positions and separate leaf, padding and branch domains. Capacity is 131,072 addresses. Context includes a version, actual namespace and sale-policy ID equal to canonical pool `AccountId` bytes. That address is independent of the root and can be computed before the tree. Stored context/root and actual trader are trusted verifier inputs, not witness claims. See the [membership format](../architecture/allowlist-proof.md#membership-format).

Public paths reveal the used salt, index and sibling hashes. Hash comparisons, structure and metadata leakage are accepted; roster-size secrecy is not guaranteed. Protect unused salts and the roster. Membership is not consumed and does not replace native authorization.

All engines use `crates/eligibility-policy`. [Signed public execution evidence](../architecture/allowlist-proof.md#public-execution-evidence) covers actual-trader authority, replay rejection and complete public buys. Weighted tests use a mock clock; normal builds require a trusted provider. Isolated verifier measurements do not prove complete buys, native private proving or a private-proof speedup.

## Limits and open work

Defer the optional per-block token allocation ceiling. This RFP-016 requirement is not implemented or removed from scope; its status is `To be reviewed by Logos team`.

Wallet batch preparation, private holdings, complete atomic private returns, security review, real proofs and deployment readiness remain open. DistributionX is a research reference. See the [allowlist design](../architecture/allowlist-proof.md) and [participation contract](../architecture/private-participation.md). Public development execution does not establish a complete private route or external approval.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
