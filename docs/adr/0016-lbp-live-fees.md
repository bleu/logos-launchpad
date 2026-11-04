# ADR 0016: Live fixed-price and weighted-sale fees and anytime collection

Status: Accepted internally; fixed-price fee burden and pricing superseded by [ADR 0035](0035-fixed-price-swap-fees.md); To be reviewed by Logos team

Date: 2026-09-29

## Context

RFP-016 F.2 and F.10 require the namespace fee rate to be snapshotted at creation. F.10 rejects fee collection before the scheduled end, and F.5 gates creator withdrawal on that scheduled end. The accepted internal design uses live rates and collection at any time instead. These are RFP departures to be reviewed by the Logos team.

## Decision

The original fixed-price and weighted-sale design kept fees creator-borne and denominated in collateral. Full collateral input participated in buyer pricing; fees accrued separately against creator proceeds with the proposal's conservative rounding. ADR 0035 replaces only that fixed-price fee burden and pricing rule. The future weighted-sale design remains creator-borne with full input in pricing.

Use the namespace rate at swap execution, not a creation snapshot. Allow permissionless, full, idempotent collection to the current namespace treasury at any time. The caller cannot choose or redirect the treasury. Terminal settlement collects the remainder.

The RFP baseline does not allow collection before the scheduled end and gates creator withdrawal on that timestamp. The internal fixed-price design allows fee collection at any time and creator withdrawal at the configured end or sellout, whichever occurs first. Without an end, withdrawal waits for sellout. Fixed-price closure and settlement follow [ADR 0025](0025-sale-closing.md).

## Consequences

Do not report these RFP departures as externally approved commitments. Their external status is `To be reviewed by Logos team`. The program must prove vault coverage of fees and remaining obligations, including when fees are collected during a sale. Fixed-price buys now use fee-first pricing under ADR 0035; that rule must not replace the future weighted-sale creator-borne model. Weighted-sale fee execution remains future work. Production input, rounding and solvency evidence remain required.

## Sources

- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
