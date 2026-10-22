# Private participation and recovery

Status: Product and recovery requirements accepted; platform, wallet and proof verification pending.

Date: 2026-10-01.

This is the shared private-flow specification for curve buys, curve sells, fixed-price sale buys and weighted sale buys. It defines required behavior, not implemented production behavior. The selected actor revision's temporary token support has no private holdings. Existing private purchase and authority helpers return explicit unsupported errors. See [ADR 0018](../adr/0018-private-participation.md), [migration limits](../adr/0034-lez-v04-migration.md) and [allowlist policy](../adr/0021-modes-and-open-limits.md).

## Transaction contract

A private route uses one transaction. It funds fresh single-use public accounts, deshields the input, executes the public trade, and returns the received tokens, unused input and unused gas to participant-controlled private accounts. Actual fees are excluded from the return. Funding, trade and returns must commit together. There is no separately committed trade followed by a re-shield retry, and no separate-transaction fallback.

A route cannot report compliant success while user funds remain public. If a platform account rule or call limit prevents complete returns, record a compatibility blocker. Do not silently retain a public balance or weaken the return rule. Actual transaction fees may still be charged after a rejected execution; the selected platform's fee behavior needs verification. Application-state rollback does not promise a fee refund.

Native gas and custom tokens may require different sources and private destinations. Each source must permit the executing program to spend that balance. The transaction must prove the required account and program authority. A guest's authorization flag alone is insufficient. Funding assets and gas through separate calls inside one transaction is permitted; funding them through separate transactions is not.

The SDK requests fresh public accounts through the wallet integration for each new attempt. It does not reuse accounts from another attempt. The provider keeps control of the fresh accounts until the outcome is resolved. Fresh account generation alone does not prevent a caller outside the SDK from reusing an address later.

All private destinations must be controlled by the participant and compatible with the asset's account rules. The SDK validates private-account type, token definition, current state where required, and control through the provider before approval. Unknown, foreign or incompatible destinations are rejected. A bare account identifier is not proof of control. Exact validation APIs and private-state access remain wallet integration requirements.

## Mechanism rules

Private and public routes use the same mechanism state, custody checks, price rules and protocol fees. Private participation does not bypass sale closing, pause controls, reserve checks or trade limits.

| Operation | Input | Received tokens | Protocol fee rule |
| --- | --- | --- | --- |
| Curve buy | Collateral | Launch tokens | Deducted from collateral input. |
| Curve sell | Launch tokens | Collateral | Deducted from collateral output. The output limit uses the applicable net result. |
| Fixed-price sale buy | Collateral | Launch tokens | Creator-borne claim on collateral proceeds. Full buyer collateral enters pricing. |
| Weighted sale buy | Collateral | Launch tokens | Creator-borne claim on collateral proceeds. Full buyer collateral enters pricing. |

A curve sell is available only when the sale mode permits it. Fixed-price and weighted sale selling is not added by this specification. Fixed-price and weighted sale buys retain their separate price rules and close at their scheduled endpoints. See [sale closing](../adr/0025-sale-closing.md), [weighted sale clock](../adr/0026-lbp-clock-and-poke.md) and [fee accounting](../adr/0015-fee-accounting.md).

The SDK obtains a fresh quote from current chain state and retains explicit participant-selected bounds. Apply the [quote and slippage decision](../adr/0017-quotes-and-slippage.md) to each supported operation. This specification does not add an on-chain instruction or change exact-input and exact-output meanings.

Return accounting uses the actual trade input and output, including the mechanism's fee treatment. An input cap is not the amount necessarily spent. Do not re-shield the full original cap after part of it entered the pool. Do not deduct the creator-borne fixed-price or weighted sale protocol fee again from the participant's returns. Native gas funding and actual transaction charges are separate from collateral protocol fees.

Public-state changes can invalidate a private proof even when the displayed price stays within the trade limit. Rebuilding requires fresh state, quote, funding checks and any required proof. Keep explicit limits unless the participant changes them. Never silently lower minimum output, raise maximum input or request an unrestricted trade. Changed transaction contents require renewed wallet approval.

## Fixed-price and weighted sale allowlist policy

These rules apply only when a fixed-price or weighted sale allowlist is enabled. They are accepted product requirements. The proof construction, identity format, enrollment tooling and verifier remain unselected and unverified.

- Each approved participant has one successful purchase per sale. A purchase below the limit still consumes that purchase allowance.
- Public and private routes use the same allowance. Switching route, receiving another credential or changing the public trade account must not grant another purchase for the same approved identity.
- Membership information alone cannot transfer permission. The participant must prove control of the approved account, and that permission must authorize the actual purchase.
- One common public collateral limit applies to all allowed participants. It limits the buyer's collateral input under the accepted fixed-price or weighted sale pricing and fee rules. Native transaction charges are separate.
- The participant list stays private. Public verification must not require publication of the roster or a persistent private participant identifier.
- Allowance consumption commits with the successful purchase. A rejected transaction does not consume the purchase allowance, even if actual transaction fees are charged.
- Invalid, copied, replayed or mismatched participation proofs must not permit a purchase. Concurrent public and private attempts may produce at most one successful purchase for the approved identity and sale.

Enforce membership, account control, collateral limits and allowance consumption on-chain for both routes. SDK checks alone are insufficient. The selected proof approach must establish sale-scoped replay protection without creating a persistent cross-sale private identity. DistributionX remains a reference, not the selected method.

Approved-account control does not prove that each account belongs to a different person. It does not prevent someone from sharing full account keys. Do not claim human uniqueness, identity checks or protection against that key sharing. Approved-identity deduplication and replacement rules need definition in the eligibility-proof investigation; multiple credentials for one approved identity must not create multiple allowances.

## Responsibilities

| Component | Required responsibility |
| --- | --- |
| Configured-sales app | Show the quote, explicit limits, public effects and progress. Present unresolved outcomes without calling them failures. |
| SDK and shared core | Validate requests and targets, obtain fresh state, build the combined transaction and track its intent and outcome. CLI and QML use the same rules. |
| Wallet provider | Select accounts, approve private balance access, obtain transaction approval, sign, submit and provide outcome and private-state synchronization support. |
| On-chain programs and platform | Enforce custody, mechanism and allowlist rules; validate the complete transaction; commit or reject its application changes. |

User keys stay outside the shared core. Wallet state, private witnesses and proving data need a protected wallet/proving boundary. The final provider interface and the location of proving remain integration questions. Do not assume that LP-0021 supplies all required methods. See [ADR 0029](../adr/0029-wallet-integration.md).

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

For an allowlisted sale, show the common public collateral limit and the one-successful-purchase rule across both routes. State that approved-account control is required. Public buying exposes the public trading account; a private roster does not hide that public account or all facts observers may infer from its trades.

The SDK applies its private-route account and return policies. Callers who construct other transactions can bypass those client policies and expose accounts or balances. They cannot bypass on-chain custody, sale or allowlist enforcement. Do not promise SDK-enforced privacy for such callers.

Warning acknowledgement frequency is deferred. Disclosure must still be present before each transaction approval; this specification does not require an extra checkbox or decide how often it is acknowledged.

## Acceptance scenarios

These are required tests for later implementation. They have not been executed for this specification. Each scenario needs evidence for the selected platform, provider and mechanism, not only a host-side helper test.

| Scenario | Required result |
| --- | --- |
| Curve private buy succeeds | Pool and fee state change once. Received launch tokens, unused collateral and unused gas end in participant-controlled private accounts. |
| Curve private sell succeeds | Permitted sale mode, correct collateral fee treatment and explicit output bound. Received collateral, unused launch input and unused gas end private. |
| Fixed-price sale private buy succeeds | Fixed-price sale execution follows current chain state. Full buyer collateral enters pricing; creator-borne fees are not charged twice. All user returns end private. |
| Weighted sale private buy succeeds | Weighted sale execution follows current chain state. Full buyer collateral enters pricing; creator-borne fees are not charged twice. All user returns end private. |
| Sale closes, pauses or rejects selling | Current mechanism checks reject the trade. No deshield or trade application changes commit. Account for any actual transaction charge separately. |
| Collateral funding fails | No funding, trade or return application changes commit. No purchase allowance is consumed. |
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
| Allowlisted public buy followed by private buy, or the reverse | First successful purchase consumes the shared allowance. The second is rejected. |
| Concurrent public/private buys or duplicate credentials | At most one purchase commits for the same approved identity and sale. A rejected attempt does not consume another allowance. |
| Membership information is shared without approved-account control | The other account cannot purchase using that information alone. |
| Invalid, replayed or wrong-sale proof is used | Reject purchase and do not consume an unused allowance. |
| Purchase is below, at or above the common collateral limit | Apply the same public cap to both routes. A successful smaller purchase consumes the one purchase; an above-cap purchase is rejected. |
| Logs, events and progress records are inspected | No private roster, persistent private participant identifier, account keys or private witness material is exposed. Verify the limited local journal separately. |
| Caller bypasses SDK policies | On-chain sale and allowlist rules still hold. Do not claim client privacy policies were enforced for that caller. |

For transaction failures, inspect the selected platform's pool state, public balances, private commitments/nullifiers and allowance state. Verify rollback of application changes and separately report fees and replay-state effects. Include deliberate late-return failures; a proof-generation error before the trade is reached does not establish rollback of the full route.

## Evidence still required

- A complete successful route on the selected platform, within its total-call budget and with correct native/token source ownership.
- Return accounting for actual input, output, protocol fees and unused gas, including any platform account-balance constraints.
- Wallet support for the whole transaction, understandable approval, participant-controlled targets, recovery references, confirmation and private-state synchronization.
- A selected, tested allowlist proof method with private membership, account binding, common-cap enforcement, sale-scoped replay protection and cross-route allowance consumption.
- Enrollment, approved-identity deduplication, credential delivery and loss/account-change behavior that do not bypass the one-purchase rule.
- Reconciliation and restart tests covering lost responses, missing hashes, authoritative terminal outcomes and no duplicate trades.
- Agreement with the accepted custody, fee, clock, event and SDK interface decisions before final design acceptance.

The [platform research](../research/private-route-transaction-patterns.md) records the old curve-buy graph, call limit and missing returns before the actor migration. Those observations do not define the selected revision's runtime limits or establish a supported private route. Private execution remains unsupported until compatible holdings and the complete transaction contract are implemented and verified. See [verification boundaries](../adr/0022-verification-boundaries.md).
