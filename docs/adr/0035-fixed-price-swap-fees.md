# ADR 0035: Fee-first fixed-price buys

Status: Accepted internally; To be reviewed by Logos team

Date: 2026-10-29

## Context

[ADR 0016](0016-lbp-live-fees.md) and [ADR 0033](0033-fixed-price-lbp-recorded-collateral.md) used full buyer collateral for fixed-price pricing and charged fees against creator proceeds. Fixed-price buys will use the existing curve fee-first policy. Trader-borne fixed-price fees are an additional RFP-016 departure, with external status `To be reviewed by Logos team`.

## Decision

For exact input, charge `fee = ceil(gross * bps / 10_000)` and price tokens from `net = gross - fee`, rounding token output down. Spend the full gross input without a rounding refund. For exact output, round the saved-price collateral requirement up to obtain `net`, then charge `gross = ceil(net * 10_000 / (10_000 - bps))`. The fee is `gross - net`. The maximum-input limit caps gross payment. Use checked arithmetic and reject zero net input, zero output and amounts outside inventory or slippage limits.

Record net collateral as creator proceeds and fees separately. Keep the live namespace rate, current treasury, anytime permissionless full idempotent collection and settlement rules unchanged. Unsolicited transfers remain surplus.

This supersedes only the fixed-price fee burden and pricing portions of ADR 0016 and ADR 0033. Their other rules remain in force. Curve behavior is unchanged. The later [ADR 0031](0031-shared-launch-factory.md) amendment applies the same namespace fee incidence to weighted buys; [ADR 0030](0030-lbp-pricing-collateral.md) defines their seed-inclusive accounting.

## Consequences

At a positive fee rate, the same gross fixed-price input buys fewer tokens, and exact output costs more gross collateral. Quotes and trade limits must use these amounts. This internal decision does not claim external approval or new execution evidence; see the [workspace guide](../architecture/workspace-guide.md#execution-and-evidence-limits).
