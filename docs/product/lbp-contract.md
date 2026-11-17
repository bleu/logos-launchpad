# Fixed-price and weighted sale overview

Fixed-price and weighted sales use four-stage setup on the actor interface. They support public trades, fee collection, creator payouts and optional graduation. Their pricing and custody stay separate. See [ADR 0031](../adr/0031-shared-launch-factory.md) for shared policies and the [workspace guide](../architecture/workspace-guide.md#execution-and-evidence-limits) for current checks and limits.

## Fixed-price sale

The creator supplies inventory, a positive exact price in smallest units and an optional creator allocation. Setup binds the sale to its namespace and initializes a separate creator role. It funds inventory and releases an `AfterSetup` allocation in the final stage; an `AfterEnd` allocation stays in escrow. Recorded collateral starts at zero. Donations remain surplus. See [setup](../adr/0032-fixed-price-lbp-setup.md), [recorded collateral](../adr/0033-fixed-price-lbp-recorded-collateral.md) and the [setup sequence](../architecture/sequence-diagrams/create-lbp-pool.md).

The selected ending policy controls settlement and the separate `AfterEnd` claim. Manual settlement pays recorded net proceeds, collects remaining namespace fees and burns unsold inventory. Completed payouts and burns cannot repeat. See [ADR 0025](../adr/0025-sale-closing.md).

## Weighted sale

Weighted setup also requires a positive collateral seed, normalized weight endpoints and an explicit start and end. Final setup transfers the full seed from the creator; a donation cannot replace it. Pricing collateral includes the seed and excludes accrued namespace fees and surplus. See [ADR 0030](../adr/0030-lbp-pricing-collateral.md).

Each weighted trade and `poke` read one saved previous-block timestamp from the pinned upstream clock. Normal builds use this provider. Inclusion-time bounds separately enforce the sale start and end. Observer checkpoints do not select prices or affect settlement. See [ADR 0026](../adr/0026-lbp-clock-and-poke.md).

## Shared rules and limits

Trade modes and sells follow [ADR 0021](../adr/0021-modes-and-open-limits.md); quotes and slippage follow [ADR 0017](../adr/0017-quotes-and-slippage.md). [ADR 0015](../adr/0015-fee-accounting.md) defines live namespace fees, retained sell fees and anytime collection. Fixed-price and weighted-sale RFP departures are accepted internally but remain `To be reviewed by Logos team` externally.

Public buy allowlists have [signed local execution evidence](../architecture/allowlist-proof.md#public-execution-evidence). Wallet allowlist flows and complete private routes remain pending. Private helpers are explicitly unsupported. The weighted-math kernel is unaudited; its qualification does not prove sale custody, clock support or deployment readiness. See the workspace guide for the full evidence limits.
