# ADR 0018: SDK-managed single-transaction private participation

Status: Product and recovery requirements accepted; platform, wallet and allowlist proof verification pending

Date: 2026-09-29

## Context

A private participant can use a public pool without making its reserves or trade effects private. The imported curve-buy PoC combines funding, deshield, purchase and re-shield calls in one transaction. It does not establish a working full route, complete return accounting or production readiness. See the [platform research](../research/private-route-transaction-patterns.md).

The LP-0021 Wallet Provider SDK is the selected integration, but it is an open prize specification with no available final API. Support for the combined private transaction, account control and safe outcome recovery remains unverified. See [ADR 0029](0029-wallet-integration.md).

## Amendments

The 2026-09-30 amendment supersedes the earlier staged private route and its post-trade re-shield recovery design. The SDK still manages the flow, but deshield, trade and re-shield execute within one transaction.

The 2026-10-01 review adds participant-controlled private destinations, complete return of unused input and gas, and pending-outcome recovery rules. It also establishes the LBP allowlist product policy in [ADR 0021](0021-modes-and-open-limits.md). It does not select a proof construction or approve an unverified platform implementation.

## Decision

Keep direct public trades and an SDK-managed private route for curve buys, permitted curve sells and LBP buys. The SDK constructs one transaction that funds fresh single-use public accounts with assets and gas, executes the public trade, and returns the received tokens, unused input and unused gas to participant-controlled private accounts. Actual fees are excluded from the return. These application changes must commit together or fail together. There is no separate-transaction fallback or separately completed trade followed by a re-shield retry.

Every private return target must be controlled by the participant and compatible with its asset and current private state. Unknown, foreign or incompatible destinations are rejected. Different assets may need different account types. If platform rules prevent complete returns, record a compatibility blocker instead of reporting success with public residue.

The SDK owns fresh quotes, explicit trade limits, destination validation, transaction construction, progress and pre-trade disclosure. CLI and QML use the same contract. The wallet provider owns account selection, private balance access approval, transaction approval, signing and submission once integrated. User keys stay outside the shared core.

Persist the submission boundary before invoking an operation that can broadcast. A lost response, missing hash, app interruption or timeout leaves the original attempt pending until its outcome is established. Do not submit a replacement trade while that outcome is unknown. A confirmed trade with incomplete wallet synchronization is not a failed trade. A known rejection permits a new attempt only with current state, a fresh quote, explicit limits and renewed approval for changed transaction contents.

Keep recovery progress participant-local and minimal. Do not put private-account identifiers, keys, balances, membership secrets or private witnesses in core progress records, logs or events. Provider recovery references and confirmation rules require integration verification.

Follow the [private participation and recovery specification](../architecture/private-participation.md) for mechanism rules, progress states, return accounting, disclosures and acceptance scenarios. Follow ADR 0021 for the shared public/private LBP purchase allowance and private membership requirements. Warning acknowledgement frequency remains deferred; disclosure before transaction approval remains required.

## Consequences

The app handles one transaction and its outcome. It does not recover separately committed deshield, trade or re-shield stages. Pool state, trade amounts, timing and public effects remain visible; SDK policy does not guarantee privacy for callers who construct other transactions.

The selected platform must demonstrate successful execution and late-failure rollback for the whole flow. Actual transaction fees may still be charged after a rejected execution. Failure recovery does not promise a fee refund, automatic reversal or a provider capability that has not been verified. Curve sell, LBP, allowlist, wallet and return-accounting implementation remain future work.

## Sources

- [Private participation and recovery specification](../architecture/private-participation.md)
- [Platform research and primary-source evidence](../research/private-route-transaction-patterns.md)
- [Imported private-buy guest](../../methods/guest/src/bin/private_buy.rs)
- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
- [Historical curve PoC private boundary decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0005-private-trade-boundary-and-verification.md)
