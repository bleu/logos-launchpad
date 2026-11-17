# ADR 0020: Manual settlement and an optional adapter hook

Status: Accepted and implemented for all three sale types; production-readiness limits remain; live DEX integration deferred

Date: 2026-09-29; amended 2026-10-01, 2026-11-12 and 2026-11-17

## Context

The original inactive hook provided no executable adapter boundary. A post-close callback allows adapter integration without choosing a DEX or promising a liquidity result.

## Decision

This accepted amendment supersedes the previous curve `R` reserve, unsold-inventory burns, separate weighted seed return and optional manual fallback. All three sale types use one payout policy. Creation saves immutable `graduation_policy { launch_bps, collateral_bps }`, an optional adapter program and its opaque configuration. Each percentage is an integer from 0 through 10,000 basis points. Configuration requires an adapter program; nonzero percentages without an adapter are rejected. Saved terms cannot change or be overridden at execution.

Eligible balances are accounted unsold launch tokens and recorded collateral, including the weighted seed. For each asset, the adapter receives `floor(eligible_balance * asset_bps / 10_000)` and the current creator receives the remainder. Checked arithmetic must support the full balance range without an overflowing intermediate product. Both shares together assign every eligible unit once. Unsold launch tokens are not burned. Accrued protocol fees go to the current namespace treasury; unsolicited surplus stays in custody. The separate creator allocation keeps its release rule and is excluded from the split. `collateral_seed` remains a funding, pricing and sell-budget protection term, with no refund priority or separate payout route.

Without an adapter, creator-authorized manual settlement pays all eligible balances to the current creator. With an adapter, graduation must use the saved split; every manual payout entry point rejects bypass, including when both percentages are zero and on completed retries. Fee collection and separate creator-allocation claims do not bypass this rule. Shared route and completion rules in `crates/sale-policy` prevent duplicate payouts, including zero-amount completion.

After setup completes and the selected ending policy permits settlement, anyone can submit the separate public hook transaction. Closure does not call it. Selected depletion can leave zero unsold inventory; even a full adapter share then receives zero launch tokens. Payout preserves the accounted unsold snapshot and cannot manufacture depletion. Private manual settlement is unsupported because the temporary token implementation has no private holdings. Automation and payer policy remain deferred.

Adapter initialization is permissionless and single-step. It requires one accepted intent kind: `Curve`, `FixedPrice` or `Weighted`. There is no unready state, later activation or reinitialization. Initialization reads authenticated saved factory state and verifies the sale kind and selected adapter program before committing, so the factory must already exist but need not be active. Initialization claims the canonical adapter-owned custody PDA derived from the saved sale identity. The factory rejects missing or wrongly owned custody before preparing its canonical token ATAs; it cannot claim custody for the adapter. Every kind-tagged request carries both launch-token and collateral amounts. The adapter verifies the kind and caller and authorizes its own PDAs. The callback binds the factory to the hash of the full request. The factory forwards no arbitrary remaining accounts and grants no factory PDA seed to the adapter. Runtime transaction signer permissions still apply. See the [handler](../../crates/factory-core/src/hook.rs) for the message contract.

The factory resolves the current creator through role-authority `Resolve` for all three engines. A permissionless caller supplies a creator hint only to admit the expected accounts; the verified reply determines the recipient and grants no caller authority. Pending replies bind the saved pool, factory, adapter and full request receipt before progress is committed. Final adapter and creator transfers, callback and `Graduated` marker succeed or roll back together. Earlier completed preparation stays complete. Failed graduation permits retry only, with no manual override or adapter replacement. `Graduated` proves callback success, not automatic DEX liquidity. See the [workspace guide](../architecture/workspace-guide.md#execution-and-evidence-limits) for checks and limits.

## Consequences

Settlement is atomic per transaction, not across the whole workflow. A broken immutable adapter can lock the required payout route. Retrying does not repair that adapter. Creator-allocation rights remain separate. The project is undeployed; no compatibility or data migration is required. Rebuild clients and guests together. See [verification boundaries](0022-verification-boundaries.md) for required evidence.

## Sources

- [Pinned LEZ ATA creation and default-owner claim](https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lez/programs/associated_token_account/src/create.rs)
- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Curve PoC settlement decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0007-factory-closure-and-creator-settlement.md)
