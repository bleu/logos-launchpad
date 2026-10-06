# LBP mechanics

An LBP sells launch tokens for collateral during a fixed time window. The weighted-math kernel exists and is unaudited; the LBP factory and pool handlers remain to be built.

## Modes

- Weighted: price depends on reserves and scheduled weights. Constant weights still allow the price to change as reserves change.
- Fixed price: one exact collateral-per-token price, with each token's decimals handled separately.

The creator chooses the mode at creation. See [ADR 0021](../adr/0021-modes-and-open-limits.md) for modes and allowlists. Under [ADR 0028](../adr/0028-token-custody-and-freeze.md), creators cannot pause; a fixed namespace emergency authority controls the swaps-only freeze for both curves and LBPs. This accepted policy replaces the RFP-016 creator pause and is `To be reviewed by Logos team`; production integration remains unbuilt.

## Clock and linear weights

Chain time determines current weights. Checkpoints help observers; a buy does not need a prior poke. Emergency freeze does not stop the schedule or change the original end. See [ADR 0026](../adr/0026-lbp-clock-and-poke.md) for clock and checkpoint rules, and [ADR 0025](../adr/0025-sale-closing.md) for closing and withdrawal.

## Buys

Both modes support spending a chosen collateral amount with a minimum token output, or buying a chosen token amount with a maximum collateral payment. Requests succeed in full or fail. See [ADR 0021](../adr/0021-modes-and-open-limits.md) for fill and rounding policy, and [ADR 0017](../adr/0017-quotes-and-slippage.md) for quotes and limits.

## Collateral and fees

Pricing collateral and actual vault collateral are separate. Fee collection must not change weighted pricing; creator payouts use the actual available collateral. See [ADR 0030](../adr/0030-lbp-pricing-collateral.md) for accounting and [ADR 0016](../adr/0016-lbp-live-fees.md) for live fees and collection.

## Implementation

Code and tests should define arithmetic, validation, errors and atomic state changes. Use the weighted-math [error bounds](../../packages/weighted-math/docs/error-analysis.md) and [qualification evidence](../../packages/weighted-math/docs/qualification.md). Those checks do not establish working LBP custody or clock integration.
