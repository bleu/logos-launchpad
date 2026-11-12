# Private participation and recovery

Status: Product and recovery requirements accepted; platform, wallet and proof verification pending.

Date: 2026-10-01; allowlist amendments: 2026-11-09 and 2026-11-10 ([ADR 0021](../adr/0021-modes-and-open-limits.md)).

This is the shared private-flow specification for curve buys, curve sells, fixed-price sale buys and weighted sale buys. It defines required behavior, not implemented production behavior. The selected actor revision's temporary token support has no private holdings. Existing private purchase and authority helpers return explicit unsupported errors. See [ADR 0018](../adr/0018-private-participation.md), [migration limits](../adr/0034-lez-v04-migration.md) and [allowlist policy](../adr/0021-modes-and-open-limits.md).

## Transaction contract

A private route uses one transaction: deshield funding, execute the public trade, then shield received tokens, unused input and unused gas into participant-controlled private accounts. The wallet prefers fresh public trading accounts; an allowlisted buy selects a pre-approved public trader as described below. Actual fees are excluded from the return. Funding, trade and returns must commit together. There is no separately committed trade followed by a re-shield retry, and no separate-transaction fallback.

A route cannot report compliant success while user funds remain public. If a platform account rule or call limit prevents complete returns, record a compatibility blocker. Do not silently retain a public balance or weaken the return rule. Actual transaction fees may still be charged after a rejected execution; the selected platform's fee behavior needs verification. Application-state rollback does not promise a fee refund.

Native gas and custom tokens may require different sources and private destinations. Each source must permit the executing program to spend that balance. The transaction must prove the required account and program authority. A guest's authorization flag alone is insufficient. Funding assets and gas through separate calls inside one transaction is permitted; funding them through separate transactions is not.

For an allowlisted buy, the wallet selects an unused approved public trader from the participant's precomputed batch and reserves it for the attempt. For other private trades, it prepares fresh public accounts. The provider must verify control and usability before approval and retain protected account derivation, membership and attempt associations for recovery. One-address-per-attempt is a privacy preference, not an on-chain restriction. If no unused approved address remains, report that limit; do not silently reuse an address or select an unapproved one. Intentional approved-address reuse needs a privacy warning and normal current-state authorization. Keep an unresolved attempt's accounts reserved and reconcile it before preparing a replacement with another address. Exact provider APIs remain unverified.

All private destinations must be controlled by the participant and compatible with the asset's account rules. The SDK validates private-account type, token definition, current state where required, and control through the provider before approval. Unknown, foreign or incompatible destinations are rejected. A bare account identifier is not proof of control. Exact validation APIs and private-state access remain wallet integration requirements.

## Mechanism rules

Private and public routes use the same mechanism state, custody checks, price rules and protocol fees. Private participation does not bypass sale closing, pause controls, reserve checks or trade limits.

| Operation | Input | Received tokens | Protocol fee rule |
| --- | --- | --- | --- |
| Curve buy | Collateral | Launch tokens | Deducted from collateral input. |
| Curve sell | Launch tokens | Collateral | Deducted from collateral output. The output limit uses the applicable net result. |
| Fixed-price sale buy | Collateral | Launch tokens | Deducted before pricing. Exact output grosses up required net collateral; the input limit caps gross payment ([ADR 0035](../adr/0035-fixed-price-swap-fees.md)). |
| Weighted sale buy | Collateral | Launch tokens | Creator-borne claim on collateral proceeds. Full buyer collateral enters pricing. |

A curve sell is available only when the sale mode permits it. Fixed-price and weighted sale selling is not added by this specification. Fixed-price and weighted sale buys retain their separate price rules. Fixed-price buys stop at the optional end or sellout; weighted sales close at their scheduled end. See [sale closing](../adr/0025-sale-closing.md), [weighted sale clock](../adr/0026-lbp-clock-and-poke.md) and [fee accounting](../adr/0015-fee-accounting.md).

The SDK obtains a fresh quote from current chain state and retains explicit participant-selected bounds. Apply the [quote and slippage decision](../adr/0017-quotes-and-slippage.md) to each supported operation. This specification does not add an on-chain instruction or change exact-input and exact-output meanings.

Return accounting uses the actual trade input and output, including the mechanism's fee treatment. An input cap is not the amount necessarily spent. Do not re-shield the full original cap after part of it entered the pool. Fixed-price input spent includes the protocol fee; do not deduct it again from returns. Future weighted-sale fees remain creator-borne and must not be deducted from participant returns. Native gas funding and actual transaction charges are separate from collateral protocol fees.

Public-state changes can invalidate a private proof even when the displayed price stays within the trade limit. Rebuilding requires fresh state, quote, funding checks and any required proof. Keep explicit limits unless the participant changes them. Never silently lower minimum output, raise maximum input or request an unrestricted trade. Changed transaction contents require renewed wallet approval.

## Allowlist policy

Optional allowlists apply to buys in all three pricing engines under [ADR 0021](../adr/0021-modes-and-open-limits.md). The 2026-11-10 amendment replaces private approved-account enrollment with pre-approved public traders. The [plain-English design](allowlist-proof.md) explains that decision; the earlier checking-account protocol is superseded. The maintained `crates/eligibility-policy` module now enforces public buy admission in all three engines. Signed development tests cover native `Execute` authority and replay rejection; see [public execution evidence](../../plans/allowlist-program-integration.md#public-execution-evidence). Weighted builds use the existing fake clock by default, with the limits in [ADR 0026](../adr/0026-lbp-clock-and-poke.md). Wallet support and complete private execution remain unverified.

- Each sale has a fixed private list of regular public trading accounts. Participants can precompute batches locally and send only public addresses to the creator before sale creation locks the context/root in the initial terms. Staged setup cannot change that policy. No keys or private funding/return account IDs go to the creator. Protected membership delivery happens once; buying must not depend on the creator's service afterward. There is no list update, replenishment or revocation feature in this version.
- Both funding routes must prove membership and control of the actual approved public trader, bound to the sale, purchase and destinations. There is no separate approval-only or private checking account. Membership information alone cannot transfer permission, and another address derived by the same wallet is not automatically approved.
- The wallet prefers an unused approved address per attempt. Approved addresses can still make repeated and independent concurrent buys when normal native state permits. There is no allowlist collateral cap, purchase counter or consumed allowance. Normal balances, inventory, reserves, pricing, fees, trade limits, pause and end rules still apply.
- The full roster is not published, but the trader address and trade effects are public. The membership path also reveals the used salt, index and sibling hashes; structure and roster-size secrecy are not guaranteed. Keep unused salts and enrollment bundles protected. The creator can link addresses submitted together; address reuse links purchases publicly. Fresh addresses do not guarantee unlinkability. Private funding/return IDs and private proof secrets stay protected, and no persistent cross-sale private participant ID is required.
- Membership data may be reused for a new authorized purchase by the same approved trader. Reject invalid proofs, authorization copied to a different trader, sale, purchase or destination, and transaction replay. Replaying the same transaction must not create another trade.

List capacity counts addresses, not people. Exhausting unused batch addresses must be reported; deliberate reuse after a privacy warning remains permitted. This does not weaken required private funding and complete private returns.

Enforce membership and purchase-bound account control on-chain for both routes. SDK checks alone are insufficient. Signatures, nonces and native private-state nullifiers remain required where applicable; no sale-level one-buy nullifier or allowance state is required. DistributionX remains a reference, not the selected method. This policy gates buys only and adds no sells.

Approved-account control does not prove that each account belongs to a different person or prevent full-key sharing. Do not claim human uniqueness or identity checks.

## Responsibilities

| Component | Required responsibility |
| --- | --- |
| Configured-sales app | Show the quote, explicit limits, public effects and progress. Present unresolved outcomes without calling them failures. |
| SDK and shared core | Validate requests and targets, obtain fresh state, build the combined transaction and track its intent and outcome. CLI and QML use the same rules. |
| Wallet provider | Select accounts, approve private balance access, obtain transaction approval, sign, submit and provide outcome and private-state synchronization support. |
| On-chain programs and platform | Enforce custody, mechanism and allowlist rules; validate the complete transaction; commit or reject its application changes. |

User keys stay outside the shared core. Wallet state, private witnesses and proving data need a protected participant-local wallet/proving boundary. Proving must run locally, without a remote prover or remote key custody. The final provider interface remains an integration question. Do not assume that LP-0021 supplies all required methods. See [ADR 0029](../adr/0029-wallet-integration.md).

## Progress states

A trade intent records what the participant wants to do. An attempt is one proposed transaction for that intent. Rebuilding after a known rejection creates a new attempt; it must not erase the original attempt's identity or result.

| State | Meaning and allowed next step |
| --- | --- |
| Draft | No transaction is ready or submitted. The participant may edit or discard the request. |
| Preparing | Validate accounts, read current sale state, quote, construct the calls and prepare the required proof through compatible wallet tooling. A proven pre-submission failure may return to preparation. |
| Awaiting approval | Present the current transaction and disclosures. User rejection cancels this attempt only if submission has not started. Changed contents require a new approval. |
| Submitting | Persist this boundary before invoking an operation that can broadcast. From this point, a missing result cannot be treated as a pre-submission failure. |
| Pending | A submission exists but its final outcome is unresolved. An RPC acknowledgement or transaction hash is not confirmation. Query the original attempt; do not submit a replacement. |
| Outcome unknown | Submission may have happened, but the result or hash is missing. Use the original provider reference and wallet synchronization to reconcile. Keep this state until evidence establishes the outcome. |
| Confirmed | The tested provider/platform confirmation rule establishes success. Never retry this intent as recovery. If private wallet synchronization is incomplete, show that separately and finish synchronization. |
| Known rejected | Authoritative evidence establishes a terminal failure, or proves that submission never occurred and cannot still occur. Refresh state and obtain approval before a new attempt. |
| Cancelled before submission | No operation that can broadcast has started. Discard preparation without claiming that an on-chain transaction was cancelled. |

A crash after persisting Submitting is treated as Outcome unknown unless the provider proves a stronger result. Closing the app, rejecting a later prompt, losing connectivity or waiting longer does not establish that an already started submission was cancelled or rejected.

Never infer rejection from a generic RPC error or a missing explorer result. Confirmation, terminal rejection, finality and identical-message resubmission rules must be verified against the selected platform and provider. Until then, unresolved outcomes remain pending. There is no automatic replacement, refund or reversal guarantee.

## Persisted progress and recovery

Persist the minimum progress needed to recover the original attempt before submission starts. The logical record needs an intent identifier, attempt identifier, progress state, provider recovery reference when available, and the original transaction identifier when available. Bind provider updates to that attempt so a late callback cannot overwrite a newer attempt. Public sale references may be stored only where needed.

The journal is participant-local. Transaction hashes, sale references and their associations can reveal participation even without secret material. Restrict access; do not publish the journal or send it to general telemetry. Storage protection, retention and the provider's recovery-reference format remain integration details.

Do not put account keys, private-account identifiers, private balances, membership secrets, Merkle paths, raw private witnesses or proving inputs in the core journal, app logs, error messages or indexer events. Keep the protected material needed for recovery in the wallet/proving boundary. A readable status record must not be sufficient to spend funds or generate membership proofs.

After restart, restore progress and request the provider access needed to reconcile. Check the original submission and synchronize private state. A missing transaction hash still requires recovery through the original attempt reference; it does not permit a new trade. If the provider cannot determine the outcome, show that limit and keep replacement blocked. Do not ask for user keys as a workaround.

A known rejected attempt can be replaced only after its result is established, private state is current, and state, quote, bounds, funding and proof are refreshed. Obtain new approval for the new transaction. A confirmed trade with delayed wallet synchronization remains a confirmed trade; synchronization failure must not create a second purchase.

Fresh accounts, signatures, nonces and nullifiers are not an application-level duplicate-trade guarantee. A rebuilt transaction can spend newer private state and make another purchase. Recovery must distinguish checking the original attempt from intentionally creating a new trade.

## Pre-trade disclosure

Before wallet approval, show the sale and operation, quoted input/output, explicit limits, protocol fee treatment, and actual transaction fee estimate or its stated uncertainty. Identify the participant-controlled private return targets and state that output, unused input and unused gas are required to end private.

State that amounts, timing, public pool state and trade effects remain visible. Fresh public accounts do not hide those effects or prove that transactions cannot be linked. Explain that an unknown outcome blocks replacement until reconciliation and that wallet synchronization can remain pending after a confirmed trade.

For an allowlisted sale, state that membership and approved-public-trader control must authorize each buy. Do not show an allowlist collateral cap, remaining allowance or purchase counter. The approved public address is visible even when the purchase uses private funds. Explain that the creator can link the submitted batch and that reusing an address links its purchases. A private roster and fresh addresses do not hide timing, amounts or all facts observers may infer. The creator does not need private funding/return IDs or account keys.

The SDK applies its private-route account and return policies. Callers who construct other transactions can bypass those client policies and expose accounts or balances. They cannot bypass on-chain custody, sale or allowlist enforcement. Do not promise SDK-enforced privacy for such callers.

Warning acknowledgement frequency is deferred. Disclosure must still be present before each transaction approval; this specification does not require an extra checkbox or decide how often it is acknowledged.

## Acceptance scenarios

These are required tests for later implementation. They have not been executed for this specification. Each scenario needs evidence for the selected platform, provider and mechanism, not only a host-side helper test.

| Scenario | Required result |
| --- | --- |
| Curve private buy succeeds | Pool and fee state change once. Received launch tokens, unused collateral and unused gas end in participant-controlled private accounts. |
| Curve private sell succeeds | Permitted sale mode, correct collateral fee treatment and explicit output bound. Received collateral, unused launch input and unused gas end private. |
| Fixed-price sale private buy succeeds | Fixed-price sale execution follows current chain state. Pricing uses collateral after fees; exact output grosses up required net collateral. Returns use actual gross input spent without charging the fee twice. All user returns end private. |
| Weighted sale private buy succeeds | Weighted sale execution follows current chain state. Full buyer collateral enters pricing; creator-borne fees are not charged twice. All user returns end private. |
| Sale closes, pauses or rejects selling | Current mechanism checks reject the trade. No deshield or trade application changes commit. Account for any actual transaction charge separately. |
| Collateral funding fails | No funding, trade or return application changes commit. Account for any actual transaction charge separately. |
| Native funding fails or uses the wrong program owner | Reject the complete transaction. Do not treat successful collateral funding as a completed stage. |
| Fresh-account setup or source authorization fails | Reject mismatched accounts, token definitions and unauthorized spending. No partial setup/funding is accepted as route success. |
| Trade fails its limit or reserve checks | No deshield, trade or return application changes commit. Keep participant-selected limits. |
| Output re-shield fails | The purchase/sale does not commit. There is no separately completed trade to retry shielding for. |
| Unused-input return fails | Reject the complete transaction. Do not report success with public input residue. |
| Unused-gas return fails | Reject the complete transaction. Do not silently keep public gas. Distinguish actual fees from unused funding. |
| Private destination is foreign, public or incompatible | Reject preparation or execution as applicable. Account control and private-state compatibility cannot be inferred from an identifier alone. |
| User cancels before submission | No broadcast occurs. Show cancellation of preparation, not rollback of a trade. |
| App crashes before submission starts | Restore preparation or establish cancellation without submitting twice. Preserve provider control of prepared accounts. |
| App crashes at the persisted submission boundary | Treat the outcome as unknown until provider evidence establishes whether broadcast occurred. |
| Submission succeeds but its response or hash is lost | Reconcile the original attempt. Exactly one trade occurs; no replacement is submitted while the result is unknown. |
| RPC error, explorer delay or elapsed time occurs | Do not convert uncertainty into rejection or a fresh trade. |
| A callback arrives after restart or another attempt | Apply it only to its recorded attempt. Do not overwrite another outcome. |
| Trade is confirmed but wallet synchronization fails | Show confirmed trade and pending wallet update. Recover private state without repeating the trade. |
| Known terminal failure permits retry | Synchronize, refresh state, quote, bounds, funding and proof, then obtain approval for the new attempt. Report actual charges accurately. |
| Quote or private proof becomes stale | Refresh after a known rejection. A pool-state mismatch may reject the proof even when the price bound would pass. Never weaken limits silently. |
| Provider lacks required combined-transaction or recovery support | Report an integration blocker. No separate-transaction fallback and no key-custody workaround. |
| Allowlisted public buy followed by private buy, or the reverse, for each pricing engine | Accept both genuine authorized purchases when normal sale and funding checks pass. Reusing the membership path does not consume permission. |
| Independent concurrent buys using approved public traders | Both may commit when each has valid purchase-bound authorization, sufficient funding and compatible native and pool state. No one-buy guard rejects them. Reuse of a nonce or private spend still fails. |
| Batch of public accounts is prepared before list lock | Verify local control and usability, enroll each address and deliver matching membership data. Send no keys or private funding/return IDs to the creator. |
| Approved public trader buys with private funds | The actual public trader is the approved account. Deshield, buy and shield output, unused input and unused gas in one transaction. Require no private approved-account ID or separate checking account. |
| A different address from the same wallet is used | Reject an unapproved trader even if another account in that wallet is approved. |
| The wallet selects the next batch address | Prefer an unused approved address and retain the account/attempt association. If the original attempt is unresolved, block a replacement through another address. |
| No unused approved addresses remain | Report the limit. Do not update the locked list, silently reuse an address or choose an unapproved one. Intentional reuse after a privacy warning can pass normal on-chain checks. |
| The same approved public address makes another authorized buy | Accept when normal state and sale checks pass. Approval is not consumed; disclose that its purchases are linked. |
| Membership information is shared without approved-account control | The other account cannot purchase using that information alone. |
| Proof has the wrong approved account, sale, purchase or destination | Reject the purchase, including copied authorization for changed transaction contents. |
| The same transaction is replayed | No second trade occurs. Signatures, nonces and native private-state replay protections remain effective. |
| Creator service is unavailable after membership delivery | A new authorized purchase can use locally held membership data and local proving. No per-buy creator request, remote prover or remote key custody is required. |
| Allowlisted buy has a different collateral amount or follows earlier buys | Apply normal balances, reserves, pricing, fees and trade limits without an allowlist cap or purchase counter. Show no remaining allowance. |
| Logs, events and progress records are inspected | No full private roster, private funding/return IDs, account keys or private witness material is exposed. Used approved public addresses and their public effects remain visible. Verify the limited local journal separately. |
| Caller bypasses SDK policies | On-chain sale and allowlist rules still hold. Do not claim client privacy policies were enforced for that caller. |

For transaction failures, inspect the selected platform's pool state, public balances and private commitments/nullifiers. Verify rollback of application changes and separately report fees and replay-state effects. Include deliberate late-return failures; a proof-generation error before the trade is reached does not establish rollback of the full route.

## Evidence still required

- A complete successful route on the selected platform, within its total-call budget and with correct native/token source ownership.
- Return accounting for actual input, output, protocol fees and unused gas, including any platform account-balance constraints.
- Wallet support for the whole transaction, understandable approval, participant-controlled targets, recovery references, confirmation and private-state synchronization.
- Private-funded execution of the selected membership method and native account authorization bound to the actual approved public trader, sale and purchase for all pricing engines. Public-funded execution has separate signed evidence linked above. Private funding/return IDs must stay protected without a separate private eligibility-checking account.
- Precomputed public-account batches, fixed-list enrollment and protected membership delivery, address selection and recovery, repeat and concurrent authorized buys, transaction replay rejection and no per-buy creator dependency.
- Reconciliation and restart tests covering lost responses, missing hashes, authoritative terminal outcomes and no duplicate trades.
- Agreement with the accepted custody, fee, clock, event and SDK interface decisions before final design acceptance.

The [platform research](../research/private-route-transaction-patterns.md) records the old curve-buy graph, call limit and missing returns before the actor migration. Those observations do not define the selected revision's runtime limits or establish a supported private route. Private execution remains unsupported until compatible holdings and the complete transaction contract are implemented and verified. See [verification boundaries](../adr/0022-verification-boundaries.md).
