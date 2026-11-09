# ADR 0020: Manual settlement and an optional adapter hook

Status: Accepted and implemented for bonding-curve settlement; fixed-price settlement follows [ADR 0025](0025-sale-closing.md); weighted-sale settlement follows ADR 0025 with local guest checks; production-readiness limits remain; live DEX integration deferred

Date: 2026-09-29; amended 2026-10-01

## Context

The original inactive hook provided no executable adapter boundary. A post-close callback allows adapter integration without choosing a DEX or promising a liquidity result.

## Decision

This amendment supersedes the inactive-hook decision. Keep creator-authorized manual settlement and add an optional adapter program and opaque configuration saved at creation. Configuration requires an adapter program. Neither can change or be overridden at execution. The adapter interprets its settings and checks its result.

After creation completes and the sale closes, anyone can submit the separate hook transaction. Closure does not call it. Hook execution is public only. Private manual settlement is unsupported on the selected actor revision because the temporary token implementation has no private holdings. Automation and payer policy are deferred.

Both routes share custody preparation, [fee collection](0015-fee-accounting.md) and [unsold-`D` burn](0025-sale-closing.md). Preparation is idempotent. The adapter must first initialize and claim its canonical custody PDA, derived under its program from the saved sale identity. The factory rejects missing or wrongly owned custody before preparing its ATAs. It cannot claim custody for the adapter.

The final hook stage transfers full `R` and recorded real collateral to those ATAs, then calls the saved adapter. Exclude fees, surplus deposits and creator allocation. The [actor migration](0034-lez-v04-migration.md) replaces the snapshot callback with a typed graduation message. It forwards no arbitrary remaining accounts and grants no factory PDA seed to the adapter. Transaction signer permissions remain available under the runtime rules, so the old generic unauthorized extra-account guard is not reproduced. The adapter must verify its runtime caller and authorize its own PDAs. See the [handler](../../crates/factory-core/src/hook.rs) for the message contract.

The first actual settlement payout selects the route and blocks competing or duplicate payouts. Preparation and creator-allocation claims do not select it. Empty manual transfers (`NoPayout`) must still permit one uncalled zero-amount hook, including after a creator-allocation claim.

The required atomic boundary contains the final transfers, callback and `Graduated` marker. They must succeed or roll back together. Earlier completed stages remain complete. A failure permits retry or creator-authorized manual recovery until a payout selects a route. `Graduated` proves callback success, not a DEX liquidity result. Sale execution and callback rollback still need verification on the selected actor revision; host checks and the token fixture do not prove them.

## Consequences

Settlement is atomic per transaction, not across the whole workflow. An immutable adapter cannot be replaced to repair a bad choice. These rules do not change fixed-price or weighted-sale settlement. See the [curve overview](../product/curve-contract.md#release-compatibility) for release compatibility and [verification boundaries](0022-verification-boundaries.md) for required evidence.

## Sources

- [Pinned LEZ ATA creation and default-owner claim](https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lez/programs/associated_token_account/src/create.rs)
- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Curve PoC settlement decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0007-factory-closure-and-creator-settlement.md)
