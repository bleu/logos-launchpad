# ADR 0030: Fee-neutral LBP pricing collateral

Status: Accepted internally; custody and numerical verification pending. Live-fee and anytime-collection departures remain `To be reviewed by Logos team`.

Date: 2026-10-02

## Context

LBP buyers use their full collateral input for pricing. Creator-borne fees can be collected while the sale is open under [ADR 0016](0016-lbp-live-fees.md). If weighted pricing reads the fee-reduced vault balance, a collection call changes the next buyer's price even when time and token reserves have not changed.

## Decision

Track weighted pricing collateral separately from actual vault collateral, outstanding fees and lifetime collected fees. Pricing collateral starts at the funded collateral seed and increases by each full successful buyer input. Fee collection changes vault and fee records only. It cannot change pricing reserves. Extra deposits remain separately recorded surplus and do not enter pricing.

Before creator withdrawal, with no unrelated collateral transfers, actual vault collateral equals pricing collateral minus lifetime collected fees plus surplus. The non-surplus vault balance must cover outstanding fees. After the original end, terminal collection pays remaining fees and creator withdrawal uses the remaining permitted vault collateral, not the pricing record. Record completed payouts; after withdrawal the pricing record cannot authorize another payout.

Keep this accounting limited to the buy-only LBP. It does not permit sell output or another transfer against a pricing balance that is no longer fully held in the vault. Fixed-price mode uses its rational price, not the collateral record, to calculate a buy. Curve reserve and fee accounting remain unchanged.

## Consequences

Future handlers must prove vault coverage and atomic, repeatable collection and settlement. Same-time quotes before and after collection must match when no trade intervenes. Rate changes affect future accrual only; current-treasury changes redirect outstanding fees. The [LBP contract](../product/lbp-contract.md) gives formulas, worked balances and future-handler acceptance cases. These are design requirements, not evidence that LBP custody or handlers work.

## Sources

- [ADR 0015: Collateral-denominated fees](0015-fee-accounting.md)
- [ADR 0016: Live LBP fees and anytime collection](0016-lbp-live-fees.md)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
