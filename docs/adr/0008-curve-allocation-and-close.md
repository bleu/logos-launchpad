# ADR 0008: Curve allocations and closing

Status: Accepted internally; To be reviewed by Logos team

Date: 2026-09-29

## Context

The curve proposal describes `R` as a DEX-seed reserve. Curve closing must distinguish the canonical supply-driven condition, optional expiry, and post-close settlement.

## Decision

For curve sales, `D` is the purchasable allocation. The canonical close condition is supply-driven: the factory closes the sale when all `D` tokens have been purchased. No collateral-raised target is a close condition.

`R` is the owner-reserved DEX-seed reserve and remains untouched until close. When `R` is not used for graduation, the factory can transfer it to the creator after close. Do not invent automatic graduation or a new manual-close eligibility rule.

An optional expiry mode closes at `D` exhaustion or the configured end timestamp, whichever occurs first. The contract does not validate a minimum sale duration when expiry is configured. This is an accepted internal departure from RFP-015 with external status `To be reviewed by Logos team`. `D` exhaustion still closes the sale immediately. Manual-close eligibility and unsold-`D` handling still require specification.

Keep allocation and closing policy in the factory. Closure is distinct from settlement. Retain reviewed curve depletion-close and settlement behavior where compatible, then test it against these production rules.

## Consequences

Source docs and interfaces that describe a different `R` lifecycle need explicit adaptation. Closure, allocation claims and settlement tests must cover each terminal path. These are curve rules; they do not define LBP allocations or its lifecycle.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Curve PoC settlement decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0007-factory-closure-and-creator-settlement.md)
