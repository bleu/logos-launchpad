# ADR 0008: Curve allocations

Status: Accepted internally; To be reviewed by Logos team

## Decision

Keep purchasable curve allocation `D`, owner-reserved DEX-seed allocation `R` and the separate creator allocation distinct. Trading cannot spend `R`; it remains untouched until close.

## Reason and limits

Factory allocation policy protects settlement inventory without adding launch terms to the neutral curve pool. [Ending rules](0025-sale-closing.md) govern unsold `D`. [Settlement](0020-settlement-and-graduation.md) governs payouts, including manual recovery of `R` when it is not used for graduation. These rules do not define fixed-price or weighted allocations. External Logos review remains pending.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Curve PoC settlement decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0007-factory-closure-and-creator-settlement.md)
