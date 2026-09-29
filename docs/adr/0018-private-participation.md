# ADR 0018: SDK-managed staged private participation

Status: Accepted for the route; private LBP and allowlist details remain open

Date: 2026-09-29

## Context

A private participant can use a public pool without making its reserves or trade effects private. The LP-0021 Wallet Provider SDK is the selected integration, but it is an open prize specification with no released SDK and no available final API. Its requirements cover account selection, user approval, signing and submission. Private deshield, re-shield and recovery support remain unverified. See [ADR 0029](0029-wallet-integration.md).

## Decision

Keep direct public swaps and the three-step private route: deshield assets and gas, trade through a fresh single-use public account, then re-shield. The SDK is planned to own orchestration, shielded-target validation, mandatory re-shield behavior and pre-trade disclosure through the LP-0021 provider once its SDK is developed and integrated. Account selection, user approval, signing and submission follow the provider specification. The final API and private-route support remain unavailable for verification. Do not investigate a single-transaction private trade in this scope.

Current recovery design resumes re-shield of purchased tokens after interruption. Do not extend that statement to unapproved refund, failed-trade or private-sell recovery semantics. Private LBP and allowlist design remain pending further specification.

## Consequences

Stages can fail separately and need explicit state and tests. Users who bypass the SDK cannot be forced by the program to re-shield. Private curve sells and LBP buys remain production gaps; this record is not evidence that they work. Disclosures must state what becomes public.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
- [Curve PoC private boundary decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0005-private-trade-boundary-and-verification.md)
