# ADR 0033: Separate fixed-price LBP collateral accounting from vault balances

Status: Accepted; implemented for fixed-price setup. Trading and settlement remain future work.

Date: 2026-10-14

## Context

An outside holder can send collateral to a prepared pool without creator approval. Requiring an empty collateral vault at activation lets that transfer block setup.

This record supersedes only the physical collateral-empty activation rule in [ADR 0032](0032-fixed-price-lbp-setup.md). Fresh-custody preparation and launch-token inventory checks remain unchanged.

## Decision

Keep recorded collateral separate from the actual collateral vault balance. Preparation initializes recorded collateral to zero. Activation requires that recorded amount to remain zero, but accepts a valid collateral vault containing unsolicited tokens. Funding leaves those tokens untouched and does not credit them to the sale. Canonical account, token-owner and definition checks still apply.

Unsolicited transfers remain surplus. They do not increase sale inventory, pricing collateral, accrued fees or creator proceeds. Future buys, fee collection and settlement must update recorded amounts atomically with the corresponding custody operations and check sufficient backing. Accrued fees remain separate from creator proceeds. Weighted pricing and fee policy remain under [ADR 0030](0030-lbp-pricing-collateral.md) and [ADR 0016](0016-lbp-live-fees.md).

## Consequences

Collateral donations no longer block activation. Surplus remains held without a sweep, refund or payout interface. This decision adds no buys, fees, settlement or recovery handlers. The changed pool-state layout requires rebuilt guests and fresh local fixture state.
