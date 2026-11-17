# ADR 0033: Separate fixed-price sale collateral accounting from vault balances

Status: Accepted; recorded accounting implemented; execution and readiness limits remain.

## Decision

Keep fixed-price recorded collateral separate from actual vault balance. Preparation sets the record to zero. Activation requires that record to remain zero, but accepts unsolicited tokens in a valid collateral vault. Funding leaves those tokens untouched and gives the sale no credit for them. Canonical account, token-owner, definition, fresh-custody and inventory checks remain required.

Record net buy collateral as creator proceeds and namespace fees separately under [ADR 0015](0015-fee-accounting.md). Both buy forms use the saved positive price and checked arithmetic; reject inventory or slippage violations. [Trade modes](0021-modes-and-open-limits.md#sale-modes) govern sells. Require recorded proceeds to back gross collateral output, then debit the record by `gross_output - retained_fee`. The saved price and payout timing do not change.

Unsolicited transfers are surplus. They cannot increase recorded sale inventory, pricing collateral, accrued fees or creator proceeds. Update recorded amounts only with corresponding custody transfers; require sufficient backing for recorded collateral and accrued fees. Collection reduces vault and fee liability together, not creator proceeds.

[Ending](0025-sale-closing.md) controls payout eligibility. Empty inventory blocks buys but is not an early payout right under `Scheduled`. Selected trading depletion is final; settlement burns cannot create depletion evidence. [Settlement](0020-settlement-and-graduation.md) pays recorded obligations once and preserves surplus.

## Reason and limits

A donation must not block activation or become creator proceeds. Surplus remains held without a sweep, refund or payout interface. Weighted pricing neutrality is a distinct invariant under [ADR 0030](0030-lbp-pricing-collateral.md). Fee departures still have status `To be reviewed by Logos team`. See [execution limits](../architecture/workspace-guide.md#execution-and-evidence-limits).
