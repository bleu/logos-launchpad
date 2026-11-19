# ADR 0008: Curve allocations

Status: Accepted internally; To be reviewed by Logos team

## Decision

Mint purchasable curve allocation `D`, optional graduation reserve `R` and the separate creator allocation. Check their sum before minting. Fund only `D` into the pool; keep `R` in mint custody and creator allocation in its separate escrow. `R` cannot affect trading or depletion. Post-close payouts follow the shared [graduation policy](0020-settlement-and-graduation.md).

## Reason and limits

Factory allocation policy keeps launch terms outside the neutral curve pool. [Ending rules](0025-sale-closing.md) govern payout eligibility. These rules do not define fixed-price or weighted allocations. External Logos review remains pending.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Curve PoC settlement decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0007-factory-closure-and-creator-settlement.md)
