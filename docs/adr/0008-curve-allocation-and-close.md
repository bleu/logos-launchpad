# ADR 0008: Curve allocations

Status: Accepted internally; To be reviewed by Logos team

Date: 2026-09-29

## Decision

The curve factory mints exactly purchasable `D` plus the separate creator allocation. This accepted amendment removes the previous `R` DEX-seed reserve, including its issuance, creation inputs and separate payout. The project is undeployed; no compatibility or data migration is required.

Allocation policy belongs to the factory, not the neutral pool. [ADR 0025](0025-sale-closing.md) governs ending eligibility. Unsold `D` follows the common two-asset payout policy in [ADR 0020](0020-settlement-and-graduation.md), without a burn. The separate creator allocation keeps its release rule and is excluded from that split.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Curve PoC settlement decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0007-factory-closure-and-creator-settlement.md)
