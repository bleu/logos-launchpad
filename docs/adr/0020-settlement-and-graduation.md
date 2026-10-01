# ADR 0020: Manual settlement and an optional adapter hook

Status: Accepted; implemented with verified host, full guest-suite and generated-IDL checks; production-readiness limits remain; live DEX integration deferred

Date: 2026-09-29

Amended: 2026-10-01

## Context

The original decision kept manual settlement and an inactive graduation hook because the reviewed PoC had no live DEX integration. That decision did not provide an executable adapter boundary. A general post-close callback can be defined without choosing a DEX or promising an external liquidity migration.

## Decision

This amendment explicitly supersedes this record's original inactive-hook decision. Retain creator-authorized manual settlement and add an optional immutable adapter program and opaque configuration saved at curve sale creation. No setter, call-time override, new administrator role or live DEX integration is added. The adapter interprets its own settings and checks its result.

The hook is permissionless after effective close and completed creation. Closure does not call it. A client submits a separate transaction and advances the required stages. Who submits and pays remains deferred. The initial release permits public hook execution only and rejects private-wrapped hook execution. Existing private manual settlement remains available.

Share canonical custody preparation, fee-bearing reserve withdrawal and unsold-`D` burn between the routes. Remaining fees go to the current treasury in the same transaction as the relevant reserve withdrawal. Burn unsold `D` after expiry once. Keep `R` and the creator allocation separate. Adapter ATA preparation is idempotent and does not select a payout route.

Before factory destination-ATA preparation, the saved adapter must initialize and claim its own canonical custody PDA. The factory verifies ownership by that saved program. The pinned ATA `Create` path claims a default owner with `Claim::Authorized`, including its idempotent branch. A correct but untouched default custody address therefore cannot replace adapter-owned setup. Do not authorize adapter custody or share factory seeds to bypass this prerequisite. Missing or wrongly owned custody rejects preparation without payout or route selection. The failed transaction leaves its stage unchanged; earlier completed stages stay complete. Retry after adapter-owned setup and creator-authorized manual recovery remain available.

The final hook stage transfers full `R` and full recorded real collateral due to canonical adapter custody, then calls the saved adapter. Exclude fees, surplus deposits and the creator allocation. Derive custody under the saved adapter program from the saved sale identity. Supply trusted fixed context and credited ATA snapshots, followed by validated ordered remaining accounts. Require an unauthorized root factory snapshot and unauthorized remaining accounts. Reject protected aliases, duplicates and additional factory-owned accounts. Give the callback no factory PDA seeds or signing rights; the adapter verifies its runtime caller and authorizes its own PDAs.

Record `Graduated` in the final transaction. If any final transfer or callback fails, its transfers and marker roll back together. Earlier successful stages remain complete; retry and creator-authorized manual recovery remain possible. The first actual settlement payout blocks the competing route and duplicates. Preparation and creator-allocation claims do not select a route. Empty manual transfers must not block an uncalled zero-amount hook.

See the [curve contract](../product/curve-contract.md) for exact accounting, stage, zero-amount, callback and evidence rules. Its evidence map records verified host, full guest-suite and generated-IDL checks, with the remaining runtime and production-readiness limits. These rules supersede dependent references to an inactive hook; they do not change LBP lifecycle or enable automatic DEX migration.

## Consequences

Settlement is repeat-safe across stages and atomic per transaction, not across the whole workflow. A terminal factory hook marker proves successful callback execution, not a DEX pool or external liquidity result. A failing adapter cannot force the creator to lose the manual recovery path before a successful payout. An immutable adapter cannot be replaced to repair a bad selection.

The fresh release changes state and wire encoding, including the shifted private factory tag. Build and verify guest, SDK, CLI and IDL artifacts together. Use fresh local state; no deployed-state migration is claimed. Delivery requires real guest balance and rollback tests, not only emitted child-call assertions. A real DEX adapter, automation and UI remain separate work.

## Sources

- [Pinned LEZ ATA creation and default-owner claim](https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lez/programs/associated_token_account/src/create.rs)
- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Curve PoC settlement decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0007-factory-closure-and-creator-settlement.md)
