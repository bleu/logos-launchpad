# Fixed-price and weighted sale overview

Fixed-price and weighted sales use four-stage setup on the actor interface. They support public trades, fee collection, creator payouts and optional graduation. Their pricing and custody stay separate. See [ADR 0031](../adr/0031-shared-launch-factory.md) for shared policies and the [workspace guide](../architecture/workspace-guide.md#execution-and-evidence-limits) for current checks and limits.

## Fixed-price sale

The creator supplies inventory, a positive exact price in smallest units and an optional creator allocation. All three sale types also support a protected [graduation reserve `R`](../adr/0020-settlement-and-graduation.md#graduation-reserve). Setup binds the sale to its namespace and initializes a separate creator role. It funds inventory and releases an `AfterSetup` allocation in the final stage; an `AfterEnd` allocation stays in escrow. Recorded collateral starts at zero. Donations remain surplus. See [setup](../adr/0032-fixed-price-lbp-setup.md), [recorded collateral](../adr/0033-fixed-price-lbp-recorded-collateral.md) and the [setup sequence](../architecture/sequence-diagrams/create-lbp-pool.md).

The selected ending policy controls settlement and the separate `AfterEnd` claim. Unsold inventory and recorded collateral follow the shared [payout route](../adr/0020-settlement-and-graduation.md). Manual settlement requires no configured adapter. See [ADR 0025](../adr/0025-sale-closing.md) for ending rules.

## Weighted sale

Weighted setup also requires a positive collateral seed, normalized weight endpoints and an explicit start and end. Final setup transfers the full seed from the creator; a donation cannot replace it. Pricing collateral includes the seed and excludes accrued namespace fees and surplus. See [ADR 0030](../adr/0030-lbp-pricing-collateral.md).

Each weighted trade reads one pricing timestamp through a temporary interface. Local tests use a fake clock. Normal builds reject weighted trades with `ClockUnavailable` until a supported provider is available. Inclusion-time bounds separately enforce the sale start and end. Observer checkpoints do not select prices or affect settlement. See [ADR 0026](../adr/0026-lbp-clock-and-poke.md) and the [temporary clock guide](../architecture/workspace-guide.md#temporary-weighted-pricing-clock).

## Shared rules and limits

Trade modes and sells follow [ADR 0021](../adr/0021-modes-and-open-limits.md); quotes and slippage follow [ADR 0017](../adr/0017-quotes-and-slippage.md). [ADR 0015](../adr/0015-fee-accounting.md) defines live namespace fees, retained sell fees and anytime collection. Fixed-price and weighted-sale RFP departures are accepted internally but remain `To be reviewed by Logos team` externally.

Public buy allowlists have [signed local execution evidence](../architecture/allowlist-proof.md#public-execution-evidence). Wallet allowlist flows and complete private routes remain pending. Private helpers are explicitly unsupported. The weighted-math kernel is unaudited; its qualification does not prove sale custody, clock support or deployment readiness. See the workspace guide for the full evidence limits.

See [ADR 0036](../adr/0036-graduation-adapter-delivery.md) for the plain-transfer distribution adapter and later deposits.
