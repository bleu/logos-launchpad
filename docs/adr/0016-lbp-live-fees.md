# ADR 0016: Live fixed-price and weighted-sale fees and anytime collection

Status: Accepted internally; To be reviewed by Logos team

Date: 2026-09-29

## Context

RFP-016 F.2 and F.10 require the namespace fee rate to be snapshotted at creation. F.10 rejects fee collection before the scheduled end, and F.5 gates creator withdrawal on that scheduled end. The accepted internal design uses live rates and collection at any time instead. These are RFP departures to be reviewed by the Logos team.

## Decision

The accepted internal fixed-price and weighted-sale fee design keeps fees creator-borne and denominated in collateral. Full collateral input participates in buyer pricing; fees accrue separately against creator proceeds with the proposal's conservative rounding. Use the namespace rate at swap execution, not a creation snapshot. Allow permissionless, full, idempotent collection to the current namespace treasury at any time. The caller cannot choose or redirect the treasury. Terminal settlement collects the remainder.

The RFP baseline does not allow collection before the scheduled end, and creator withdrawal remains gated on that timestamp. The internal fixed-price design allows fee collection at any time, including after sellout or the configured end. Fixed-price closure follows [ADR 0025](0025-sale-closing.md).

## Consequences

Do not report these RFP departures as externally approved commitments. Their external status is `To be reviewed by Logos team`. The program must prove vault coverage of fees and remaining obligations, including when fees are collected during a sale. Curve fee deductions cannot substitute for the creator-borne model. Fixed-price and weighted-sale fee execution remains future work. Production input, rounding and solvency evidence remain required.

## Sources

- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
