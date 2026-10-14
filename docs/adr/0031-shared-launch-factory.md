# ADR 0031: Share launch setup without sharing mechanism rules

Status: Shared fixed-price sale creation and funding implementation; locked local verification passed using development receipts. Real proofs and deployment readiness pending.

Date: 2026-10-13

## Context

Bonding-curve, fixed-price, and weighted sales all need new-token creation, allocation, namespace binding and setup checks. Those operations can use one launch factory when their rules match. Their pricing, custody, fees and lifecycle rules do not match.

This record partially supersedes [ADR 0005](0005-mechanism-boundaries.md). It replaces the separate-factory requirement only for common launch setup. It does not combine bonding-curve, fixed-price, or weighted-sale pool behavior.

## Decision

Use the existing launch factory for common new-token creation, allocation, namespace binding and setup validation when the rules are the same. Keep typed bonding-curve, fixed-price sale, and weighted-sale inputs separate.

Bonding-curve, fixed-price, and weighted sales retain their own pool pricing, token custody, lifecycle and fee rules. Do not add a generic pool lifecycle or a common mechanism state model. Share a helper only when its behavior and error contract match.

## Consequences

A common factory can create and prepare sales without making sale state interchangeable. Bonding-curve close and settlement rules remain bonding-curve rules. Fixed-price sale creation and funding rules are in [ADR 0032](0032-fixed-price-lbp-setup.md). Weighted-sale accounting, sells, end hooks and lifecycle work need separate decisions.

## Sources

- [Mechanism boundaries](0005-mechanism-boundaries.md)
- [Typed sale inputs](0007-typed-sale-inputs.md)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
