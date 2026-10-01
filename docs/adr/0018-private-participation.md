# ADR 0018: SDK-managed single-transaction private participation

Status: Accepted for the route; private LBP and allowlist details remain open

Date: 2026-09-29

## Context

A private participant can use a public pool without making its reserves or trade effects private. The imported curve-buy PoC combines funding, deshield, purchase and re-shield calls in one transaction. This is useful baseline code, not proof of production readiness.

The LP-0021 Wallet Provider SDK is the selected integration, but it is an open prize specification with no released SDK and no available final API. Its requirements cover account selection, user approval, signing and submission. Support for the combined private transaction remains unverified. See [ADR 0029](0029-wallet-integration.md).

## Amendment

The 2026-09-30 amendment supersedes the earlier staged private route and its post-trade re-shield recovery design. The SDK still manages the flow, but deshield, trade and re-shield execute within one transaction.

## Decision

Keep direct public swaps and an SDK-managed private purchase route. The SDK constructs one transaction that funds a fresh single-use public account with assets and gas, trades through that account, and re-shields the purchased tokens. These operations must commit together or fail without a partial trade or re-shield result. Verify that behavior on the target platform before release.

The SDK owns quotes, explicit trade limits, shielded-target validation, transaction construction and pre-trade disclosure. The LP-0021 provider owns account selection, user approval, signing and submission once developed and integrated. The shared core does not hold user keys. Neither the provider specification nor the imported PoC establishes that this integration works.

Private curve sells, private LBP buys and allowlist proofs still need specifications and verification. This decision does not add a separate-transaction fallback or a purchased-token re-shield recovery flow.

## Consequences

The app handles one transaction submission and its outcome, rather than managing separately committed trade stages. Failure handling, unknown submission outcomes and retry safety require platform and provider verification; no automatic refund, reversal or recovery guarantee is added here. Disclosures must state that pool state and trade effects remain public.

## Sources

- [Imported private-buy guest](../../methods/guest/src/bin/private_buy.rs)
- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
- [Historical curve PoC private boundary decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0005-private-trade-boundary-and-verification.md)
