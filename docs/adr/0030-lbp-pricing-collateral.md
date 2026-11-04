# ADR 0030: Fee-neutral weighted-sale pricing collateral

Status: Accepted internally; host and seeded buy/fee-collection guest checks passed. Weighted setup and rollback guest checks passed. Payout verification remains pending.

Date: 2026-10-02

## Context

Collecting creator-borne fees from the vault must not change the next weighted buy's price.

## Decision

Keep pricing collateral separate from actual vault collateral. Pricing collateral includes the seed and full successful buyer inputs; fee collection does not reduce it. Surplus does not enter pricing. Creator payouts use recorded proceeds backed by available vault collateral, not the pricing record or unsolicited surplus; see [ADR 0033](0033-fixed-price-lbp-recorded-collateral.md). This applies to buy-only weighted sales; bonding-curve accounting is unchanged.

## Consequences

Code and tests must check vault coverage, prevent duplicate payouts and keep pricing unchanged by collection. Fee policy and pending Logos review remain under [ADR 0016](0016-lbp-live-fees.md).

## Sources

- [Collateral-denominated fees](0015-fee-accounting.md)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
