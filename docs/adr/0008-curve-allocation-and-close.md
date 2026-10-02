# ADR 0008: Curve allocations

Status: Accepted internally; To be reviewed by Logos team

Date: 2026-09-29

## Decision

The curve factory keeps three allocations separate: purchasable `D`, owner-reserved `R` for DEX seeding, and the creator allocation. `R` remains untouched until close. This prevents trading from spending tokens reserved for settlement.

Allocation policy belongs to the factory, not the neutral pool. [ADR 0025](0025-sale-closing.md) supersedes this record's earlier closing rules and governs unsold `D`. [ADR 0020](0020-settlement-and-graduation.md) governs post-close payouts, including manual settlement when `R` is not used for graduation. These rules do not define LBP allocations.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Curve PoC settlement decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0007-factory-closure-and-creator-settlement.md)
