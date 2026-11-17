# ADR 0021: Sale types, trade modes, allowlists and open limits

Status: Product rules and public-account batch enrollment accepted internally; public buy admission implemented for all three engines with signed development-execution evidence; wallet and private-route verification open; per-block limit deferral: `To be reviewed by Logos team`.

Date: 2026-09-29; sale mechanics review: 2026-10-02; allowlist amendments: 2026-11-09 and 2026-11-10

## Context

Keep the three pricing engines with shared immutable factory trade modes. On 2026-11-09, the user extended optional allowlists to all three engines and removed the common collateral cap and one-purchase allowance accepted on 2026-10-01. On 2026-11-10, the user chose pre-approved batches of public trading accounts. This replaces the requirement to enroll private approved accounts and the proposed separate private checking account. Private funding and complete private returns remain required. The same day's proof interview accepted public Merkle paths and locking the list at initial sale creation. Public admission now uses the maintained `crates/eligibility-policy` module in all three engines. Signed development tests cover actual-trader authority through native `Execute`, transaction replay rejection and complete public buys; they do not establish private-route or deployment readiness. [ADR 0031](0031-shared-launch-factory.md) records the target separation of pricing from shared pool policies.

## Sale modes

All factory engines require immutable `TradeMode::{BuyOnly, BuyAndSell}`. `BuyOnly` permits exact-input and exact-output buys with collateral and rejects sells. `BuyAndSell` also permits exact-input sells with a minimum net collateral output. Exact-output buys are unchanged; exact-output sells are unsupported. UI defaults remain open. Direct curve pools retain the separate `allowed_input_token` primitive, including token0-only input; it is not a factory trade mode.

[ADR 0017](0017-quotes-and-slippage.md) defines quote checks and provider-time limits. See [implementation evidence](../product/curve-contract.md#acceptance-evidence). [ADR 0025](0025-sale-closing.md) owns closing rules and the pending expiry-duration review.

## Fixed-price and weighted sales

Fixed-price and weighted sales support exact-input buys with a minimum output and exact-output buys with a maximum gross input. Fixed-price exact input spends the full input and floors output; exact output ceils payment. Reject oversized buys in full, without partial fills or rounding-remainder refunds. In `BuyAndSell`, fixed-price exact-input sells floor gross collateral at the saved price, then deduct sell fees under [ADR 0015](0015-fee-accounting.md). Weighted sells use the checked reverse quote and cannot spend the saved seed, accrued fees or surplus. Both check the minimum net collateral output and recorded proceeds backing.

A fixed-price sale may omit its end time. When it has an end time, it must be after the resolved start. A weighted sale requires an end time after its resolved start. Fixed-price start-now and scheduled starts use the inclusion-time bounds in [ADR 0032](0032-fixed-price-lbp-setup.md). Weighted creation requires an explicit scheduled start. The canonical clock must be at or before that start. There is no weighted start-now option while the clock cannot return a trusted time. [ADR 0025](0025-sale-closing.md) defines when each sale closes. [ADR 0026](0026-lbp-clock-and-poke.md) owns weighted validity windows and schedule interpolation.

When pause is enabled, only the current creator authority holder can pause/resume before the selected ending, including before start. Authorized repeated calls succeed without another state change or event. Authority must still be checked. Pause does not stop weighted-sale weight changes or move either sale's end, and resume cannot reopen an ended sale. This settles the previously open fixed-price and weighted-sale pause authority; it does not grant sale control to namespace administrators or select platform freeze gates.

Allowlist admission applies only to buys. A permitted sell can replenish empty inventory under `Scheduled`; selected depletion under `Depletion` or `Either` is final and rejects later sells. Payout cannot create depletion evidence.

Defer the optional limit on tokens allocated in one block. This differs from the RFP-016 requirement and is `To be reviewed by Logos team`. It is not implemented or removed from scope.

## Allowlist rules for all pricing engines

An optional allowlist must be available to bonding-curve, fixed-price and weighted pools. When enabled:

- The sale has a fixed private list of approved regular public LEZ trading accounts. Before the list is locked, participants can create batches of public accounts locally and send only their addresses to the creator. No keys or private funding/return account IDs go to the creator. After initial protected membership-data delivery, buying must not depend on the creator's service.
- Each buy must prove membership and control of the actual approved public trader, bound to the sale and purchase. There is no separate approval-only or private checking account. Membership data alone must not grant authority, and another address from the same wallet is not automatically approved.
- The wallet prefers an unused approved address per attempt. This is a privacy preference, not an on-chain one-buy rule. Approved accounts can make repeated purchases, including independent concurrent purchases across public and private funding routes when native state permits. There is no allowlist spending cap, purchase counter or consumed allowance. Normal balance, inventory, reserve, price-limit, fee, direction, pause and ending rules still apply.
- The full roster is not published, but an approved trading address and its trade effects are public when used. The creator can link addresses submitted together; public address reuse also links purchases. Fresh addresses do not guarantee unlinkability from timing, amounts or other data. Private funding and return account IDs must stay private. No persistent cross-sale participant ID is required.
- Invalid or mismatched authorization must fail. Reusing membership data for a new authorized buy is allowed; copying authorization to a different purchase or replaying a transaction must not create another trade.

All batch addresses must be included before sale creation. Save the final context and root with the initial immutable terms; no staged setup action can replace them. There is no list update or replenishment in this version. Capacity counts approved addresses, not people. If the batch has no unused addresses left, the wallet must report that limit rather than silently reuse an address or choose an unapproved one. A participant can deliberately reuse an approved address after a privacy warning; on-chain rules do not consume its approval.

On-chain programs must enforce eligibility on both routes. Removing the allowance does not remove signatures, nonces, native private-state nullifiers or submission recovery. Private funding, trade and returns must commit together under [ADR 0018](0018-private-participation.md). The allowlist gates buys only; permitted sells use the shared trade mode without membership admission. Account control does not establish human uniqueness or prevent full-key sharing. An approved account may buy all available sale inventory.

## Membership method and evidence

Use a public SHA-256 Merkle path with fixed depth 17, independent random 32-byte salts, randomized leaf positions and separate leaf, padding and branch domains. Capacity is 131,072 addresses. Context contains a version, the actual namespace and a sale-policy ID equal to the canonical pool `AccountId` bytes. That address does not depend on the root and can be computed before tree construction. The stored context/root and actual trader are trusted verifier inputs; they are not accepted from the witness as claims. See the [membership format](../architecture/allowlist-proof.md#membership-format).

Public paths expose the used leaf's salt, index and sibling hashes. Hash comparisons, structure and metadata leakage are accepted; roster-size secrecy is not guaranteed. Protect unused salts and the full roster. Native transaction authorization and replay checks remain necessary. Membership does not grant spending authority or get consumed by a purchase.

On 2026-11-10, the original isolated signed verifier passed at both three and 100,000 addresses under the unchanged 10,000,000-cycle charged gate. The larger fixture used 260,365 cycles; path preparation took about 0.14 ms. After switching the guest adapter to the maintained verifier, the signed gate passed again at 271,708 and 271,709 cycles respectively. The original timings remain historical measurements. These isolated runs are public signed self-execution, not full buys or native private proving. The [design guide](../architecture/allowlist-proof.md#earlier-measurements) records the measurement limits and research source; the [program integration plan](../../plans/allowlist-program-integration.md#public-execution-evidence) records separate complete-public-buy evidence and its limits. Weighted execution uses a mock clock; normal weighted buys remain unsupported without a trusted timestamp provider. The earlier Semaphore experiment exceeded that gate, including its prepared-key variant.

## Open work

The [plain-English allowlist design](../architecture/allowlist-proof.md) explains public-account batch enrollment and the one-transaction private route. The earlier separate checking account and mandatory private eligibility guest are not current requirements. Public policy storage, admission and native authorization are implemented, and staged creation preserves the exact policy. Wallet support, private holdings, complete atomic private returns, security review, real proofs and deployment readiness remain open. See the [program integration plan](../../plans/allowlist-program-integration.md). Native privacy proving for private funding and returns is still required; no speedup for that proof is established. DistributionX remains a research reference. See the [full participation specification](../architecture/private-participation.md).

Signed public-buy development evidence does not establish a complete private route or production readiness. Deferred requirements still need Logos-team review.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
