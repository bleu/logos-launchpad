# ADR 0033: Separate fixed-price sale collateral accounting from vault balances

Status: Accepted; implemented for fixed-price setup, public buys and fee collection. Close and settlement remain future work.

Date: 2026-10-14

## Context

An outside holder can send collateral to a prepared pool without creator approval. Requiring an empty collateral vault at activation lets that transfer block setup.

This record supersedes only the physical collateral-empty activation rule in [ADR 0032](0032-fixed-price-lbp-setup.md). Fresh-custody preparation and launch-token inventory checks remain unchanged.

## Decision

Keep recorded collateral separate from the actual collateral vault balance. Preparation initializes recorded collateral to zero. Activation requires that recorded amount to remain zero, but accepts a valid collateral vault containing unsolicited tokens. Funding leaves those tokens untouched and does not credit them to the sale. Canonical account, token-owner and definition checks still apply.

A fixed-price sale is buy-only. Exact-input buys spend the full collateral input and floor token output. Exact-output buys round collateral payment up. Both use the positive saved price and reject a trade that exceeds its inventory, slippage limit or checked arithmetic range. Buys use the full payment for pricing. The live namespace fee is charged against creator proceeds and tracked separately. The fee rate and anytime collection policy remain `To be reviewed by Logos team` under [ADR 0016](0016-lbp-live-fees.md).

An optional end time stops buys at that time. A sale with no end time remains open until its inventory sells out. Sellout stops later buys and makes recorded proceeds collectible. Sellout also makes an `AfterEnd` creator allocation eligible; payout is future work under [ADR 0025](0025-sale-closing.md).

Unsolicited transfers remain surplus. They do not increase sale inventory, pricing collateral, accrued fees or creator proceeds. Buy and fee-collection handlers update recorded amounts only with the corresponding custody transfers and check sufficient backing. Accrued fees remain separate from creator proceeds. Weighted-sale pricing and fee policy remain under [ADR 0030](0030-lbp-pricing-collateral.md) and [ADR 0016](0016-lbp-live-fees.md).

## Consequences

Collateral donations do not block activation. Surplus remains held without a sweep, refund or payout interface. The pool-state layout changed; rebuild guests and use fresh local fixture state. Fixed-price close and payout handlers remain future work.
