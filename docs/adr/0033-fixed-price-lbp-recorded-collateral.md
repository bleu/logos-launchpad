# ADR 0033: Separate fixed-price sale collateral accounting from vault balances

Status: Accepted; fixed-price fee burden and pricing superseded by [ADR 0035](0035-fixed-price-swap-fees.md). Recorded collateral rules are implemented for fixed-price setup, public buys and fee collection. Settlement follows [ADR 0025](0025-sale-closing.md), with signed guest coverage of transfers, burns and rollback.

Date: 2026-10-14

## Context

An outside holder can send collateral to a prepared pool without creator approval. Requiring an empty collateral vault at activation lets that transfer block setup.

This record supersedes only the physical collateral-empty activation rule in [ADR 0032](0032-fixed-price-lbp-setup.md). Fresh-custody preparation and launch-token inventory checks remain unchanged.

## Decision

Keep recorded collateral separate from the actual collateral vault balance. Preparation initializes recorded collateral to zero. Activation requires that recorded amount to remain zero, but accepts a valid collateral vault containing unsolicited tokens. Funding leaves those tokens untouched and does not credit them to the sale. Canonical account, token-owner and definition checks still apply.

Fixed-price trade modes follow [ADR 0021](0021-modes-and-open-limits.md). Sells require recorded proceeds to back gross collateral output. Debit recorded collateral by `gross_output - retained_fee` under [ADR 0015](0015-fee-accounting.md); the saved price and payout timing stay unchanged. Both buy forms use the positive saved price and reject a trade that exceeds its inventory, slippage limit or checked arithmetic range. This record originally used full payment for pricing and charged the live namespace fee against creator proceeds. ADR 0035 replaces only that fee burden and pricing rule: exact input prices tokens after fees, and exact output grosses up the rounded net collateral requirement. Live rates and anytime collection remain under [ADR 0016](0016-lbp-live-fees.md). The fixed-price fee departures remain `To be reviewed by Logos team`.

Creation selects `Scheduled`, `Depletion` or `Either` under [ADR 0025](0025-sale-closing.md). `Scheduled` requires an end and ignores depletion for payout eligibility. `Depletion` has no end; `Either` requires one. Selected depletion ends trading and permits proceeds and `AfterEnd` allocation claims. Empty inventory always prevents buys, but settlement burns never create depletion evidence.

Unsolicited transfers remain surplus. They do not increase sale inventory, pricing collateral, accrued fees or creator proceeds. Buy and fee-collection handlers update recorded amounts only with the corresponding custody transfers and check sufficient backing. Accrued fees remain separate from creator proceeds. Weighted-sale pricing and fee policy remain under [ADR 0030](0030-lbp-pricing-collateral.md) and [ADR 0016](0016-lbp-live-fees.md).

## Consequences

Collateral donations do not block activation. Surplus remains held without a sweep, refund or payout interface. The pool-state layout changed; rebuild guests and use fresh local fixture state. Fixed-price settlement preserves surplus under [ADR 0025](0025-sale-closing.md).
