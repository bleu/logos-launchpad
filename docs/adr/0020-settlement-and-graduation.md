# ADR 0020: Manual settlement and an optional adapter hook

Status: Accepted and implemented for all three sale types; production-readiness limits remain; live DEX integration deferred

Date: 2026-09-29; amended 2026-10-01 and 2026-11-12

## Context

The original inactive hook provided no executable adapter boundary. A post-close callback allows adapter integration without choosing a DEX or promising a liquidity result.

## Decision

This amendment supersedes the inactive-hook decision. Keep creator-authorized manual settlement and add an optional adapter program and opaque configuration saved at creation. Configuration requires an adapter program. Neither can change or be overridden at execution. The adapter interprets its settings and checks its result.

After creation completes and the sale closes, anyone can submit the separate hook transaction. Closure does not call it. Hook execution is public only. Private manual settlement is unsupported on the selected actor revision because the temporary token implementation has no private holdings. Automation and payer policy are deferred.

Both routes share custody preparation, [fee collection](0015-fee-accounting.md) and [unsold-`D` burn](0025-sale-closing.md). Preparation is idempotent. The adapter must first initialize and claim its canonical custody PDA, derived under its program from the saved sale identity. The factory rejects missing or wrongly owned custody before preparing its ATAs. It cannot claim custody for the adapter.

The curve hook transfers full `R` and recorded real collateral to those ATAs. Fixed-price hooks transfer only recorded net proceeds. Weighted hooks return the outstanding saved seed to the current creator and transfer only net buyer proceeds to the adapter. Fixed-price and weighted hooks send zero launch tokens; neither has an `R` reserve. Unsold inventory burns once on either route. Fees go to the current treasury; surplus stays in custody, and separate creator allocation keeps its existing release rule. A saved seed-return marker prevents a second seed payment or subtraction.

Each request carries a typed asset intent. Permissionless initialization may leave an adapter unready. Anyone may then activate it once by selecting one accepted intent kind; that choice cannot be cleared or changed. The callback binds the factory to the hash of the full request. The factory forwards no arbitrary remaining accounts and grants no factory PDA seed to the adapter. Transaction signer permissions remain available under runtime rules. The adapter must verify its caller and authorize its own PDAs. See the [handler](../../crates/factory-core/src/hook.rs) for the message contract.

All three engines use shared route and completion rules from `crates/sale-policy`. A route-specific payout or completed manual settlement blocks competing and duplicate payouts. Manual completion is final even with zero assets. Graduation chosen first may also send zero assets. Fees, burns, creator-allocation claims and weighted seed recovery do not select a route. Curve custody stages project into this shared progress; LBP pools store it directly and acknowledge matching progress to the factory.

The final transfers, callback and `Graduated` marker must succeed or roll back together. LBP graduation also includes final fees, unsold burn and outstanding seed return in that transaction. Pending replies bind the saved pool, factory, adapter and receipt hash before progress is committed. The factory resolves the current creator through the role-authority module; a caller hint only admits the expected holding and grants no rights. Earlier completed stages remain complete. A failure permits retry or creator-authorized manual recovery until a payout selects a route. `Graduated` proves callback success, not a DEX liquidity result. See the [local execution evidence](../architecture/workspace-guide.md#execution-and-evidence-limits) for checks and limits.

## Consequences

Settlement is atomic per transaction, not across the whole workflow. An immutable adapter cannot be replaced to repair a bad choice. The hook preserves existing manual payouts and allocation rights. See the [curve overview](../product/curve-contract.md#release-compatibility) for release compatibility and [verification boundaries](0022-verification-boundaries.md) for required evidence.

## Sources

- [Pinned LEZ ATA creation and default-owner claim](https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lez/programs/associated_token_account/src/create.rs)
- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Curve PoC settlement decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0007-factory-closure-and-creator-settlement.md)
