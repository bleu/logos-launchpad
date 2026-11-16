# ADR 0016: Live fixed-price and weighted-sale fees and anytime collection

Status: Accepted internally; fixed-price fee burden and pricing superseded by [ADR 0035](0035-fixed-price-swap-fees.md), weighted fee burden and pricing by [ADR 0031](0031-shared-launch-factory.md); To be reviewed by Logos team

Date: 2026-09-29

## Context

RFP-016 F.2 and F.10 require the namespace fee rate to be snapshotted at creation. F.10 rejects fee collection before the scheduled end, and F.5 gates creator withdrawal on that scheduled end. The accepted internal design uses live rates and collection at any time instead. These are RFP departures to be reviewed by the Logos team.

## Decision

The original fixed-price and weighted-sale design kept fees creator-borne and used full input for pricing. ADR 0035 replaces fixed-price incidence; ADR 0031 replaces weighted incidence. Both now price net collateral input and gross up required net collateral for exact output. Namespace fees remain collateral-denominated and separate from recorded creator proceeds.

Use the namespace rate at swap execution, not a creation snapshot. Allow permissionless, full, idempotent collection to the current namespace treasury at any time. The caller cannot choose or redirect the treasury. Terminal settlement collects the remainder.

The RFP baseline does not allow collection before the scheduled end and gates creator withdrawal on that timestamp. The internal design allows fee collection at any time and creator withdrawal when the selected ending policy permits it under [ADR 0025](0025-sale-closing.md).

## Consequences

Do not report these RFP departures as externally approved commitments. Their external status is `To be reviewed by Logos team`. The program must prove vault coverage of fees and remaining obligations, including when fees are collected during a sale. Fixed-price and weighted buys use the shared namespace fee rule under ADR 0031. Weighted-sale buys, fee collection and settlement have local guest checks. See [execution evidence](../architecture/workspace-guide.md#execution-and-evidence-limits). Production input, rounding and solvency evidence remain required.

## Sources

- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
