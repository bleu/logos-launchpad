# ADR 0020: Manual settlement and inactive graduation

Status: Accepted

Date: 2026-09-29

## Context

The curve proposal anticipates DEX graduation through RFP-004, but no live graduation integration is established by the reviewed PoC. This design retains manual settlement and an inactive hook.

## Decision

Keep manual post-close settlement as the delivered path. The factory owns allocation claims, creator authorization and proceeds settlement; the neutral pool owns its close and reserve-withdrawal checks. Collect remaining protocol fees as part of terminal settlement.

Retain the inactive graduation hook as a requirement. Do not wire it to an assumed DEX interface or claim automatic graduation works. `R` remains the DEX-seed reserve until close. If it is not used for graduation, the factory can transfer it to the creator after close.

## Consequences

Settlement must remain safe and repeatable across its permitted resumable stages. Future activation needs a live DEX contract, separate design review and integration evidence. An inactive hook is not a delivered external liquidity migration.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Curve PoC settlement decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0007-factory-closure-and-creator-settlement.md)
