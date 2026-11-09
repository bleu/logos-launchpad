# ADR 0031: Share launch setup without sharing mechanism rules

Status: Fixed-price and weighted setup implemented. Host checks passed. Weighted setup, buy and rollback guest checks passed with development execution. Real proofs and deployment readiness pending.

Date: 2026-10-13

## Context

Bonding-curve, fixed-price, and weighted sales all need new-token creation, allocation, namespace binding and setup checks. Those operations can use one launch factory when their rules match. Their pricing, custody, fees and lifecycle rules do not match.

This record partially supersedes [ADR 0005](0005-mechanism-boundaries.md). It replaces the separate-factory requirement only for common launch setup. It does not combine bonding-curve, fixed-price, or weighted-sale pool behavior.

## Decision

Use the existing launch factory for common new-token creation, allocation, namespace binding and setup validation when the rules are the same. Keep typed bonding-curve, fixed-price sale, and weighted-sale inputs separate.

Bonding-curve, fixed-price, and weighted sales retain their own pool pricing, token custody, lifecycle and fee rules. Do not add a generic pool lifecycle or a common mechanism state model. Share a helper only when its behavior and error contract match.

Weighted creation uses four stages: mint the exact supply and create the creator role; split sale inventory and creator allocation; prepare the weighted pool and vaults; fund and activate the pool. Keep typed creation messages separate. Use one setup record with an explicit sale mode. Existing fixed-price message tags and state bytes remain unchanged.

The final weighted stage transfers the full collateral seed from the current creator authority. It also transfers the sale inventory from factory custody. A prior vault donation does not replace the seed. Check the seed balance and vault capacity before transfer. Activate only when inventory and seed are backed. Release an `AfterSetup` allocation in the same transaction. Keep an `AfterEnd` allocation in escrow. All effects must roll back if any operation fails.

Weighted weights and start rules are in [ADR 0026](0026-lbp-clock-and-poke.md). The factory does not share weighted pool pricing or buy rules with fixed-price pools.

## Consequences

A common factory can create and prepare sales without making sale state interchangeable. Bonding-curve close and settlement rules remain bonding-curve rules. Fixed-price sale creation and funding rules are in [ADR 0032](0032-fixed-price-lbp-setup.md). Weighted-sale accounting, sells, end hooks and lifecycle work need separate decisions.

## Sources

- [Mechanism boundaries](0005-mechanism-boundaries.md)
- [Typed sale inputs](0007-typed-sale-inputs.md)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)

## Clock adapter amendment: 2026-11-09

On the selected actor revision, setup reads the canonical clock instead of sending removed assertion messages. A narrow adapter stores the pending response and its inclusive millisecond bounds. Only an authenticated timestamp reply can release that response. It contains the final state and the deferred setup effects. Keep four setup stages, saved-time comparisons and inclusion-time bounds. See [clock integration](0026-lbp-clock-and-poke.md#upstream-clock-amendment-2026-11-09).
