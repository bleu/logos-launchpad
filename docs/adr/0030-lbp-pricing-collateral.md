# ADR 0030: Fee-neutral LBP pricing collateral

Status: Accepted internally; custody and numerical verification pending.

Date: 2026-10-02

## Context

Collecting creator-borne fees from the vault must not change the next weighted buy's price.

## Decision

Keep pricing collateral separate from actual vault collateral. Pricing collateral includes the seed and full successful buyer inputs; fee collection does not reduce it. Surplus does not enter pricing. Creator payouts use available vault collateral, not the pricing record. This applies to buy-only LBPs; curve accounting is unchanged.

## Consequences

Code and tests must check vault coverage, prevent duplicate payouts and keep pricing unchanged by collection. Fee policy and pending Logos review remain under [ADR 0016](0016-lbp-live-fees.md).

## Sources

- [Collateral-denominated fees](0015-fee-accounting.md)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
