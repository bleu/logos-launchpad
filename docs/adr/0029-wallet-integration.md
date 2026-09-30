# ADR 0029: Integrate the LEZ Wallet Provider SDK

Status: Accepted; integration awaits development of LP-0021

Date: 2026-09-29

## Context

[LP-0021: LEZ Wallet and Provider SDK](https://github.com/logos-co/lambda-prize/blob/master/prizes/LP-0021.md) is an open prize for a wallet and a shared interface for apps. Its requirements cover account access, balance queries, transfer proposals and smart-contract transaction proposals. It is a specification, not a released SDK.

## Decision

Integrate the LP-0021 Wallet Provider SDK once it is developed. The launchpad proposes actions through the provider. The wallet handles account selection, user approval, signing and submission. User keys stay outside the shared core.

The prize requires explicit approval before signing or submission, and before an app reads a private account balance. Use its approval and result handling rather than build a separate wallet flow.

## Consequences

The integration choice is settled; the final API and compatible release remain pending. Verify support for the combined deshield, trade and re-shield transaction against [ADR 0018](0018-private-participation.md) when the SDK is available. LP-0021 alone does not establish support for the full private route.

## Sources

- [LP-0021 prize specification](https://github.com/logos-co/lambda-prize/blob/master/prizes/LP-0021.md)
- [Announcement supplied by the user](https://x.com/logos_devs/status/2103043576292856202). The post was inaccessible; the technical details above come from the prize specification.
