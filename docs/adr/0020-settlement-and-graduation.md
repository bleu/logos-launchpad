# ADR 0020: Shared settlement payouts and adapter initialization

Status: Accepted and implemented for all three sale types; production-readiness limits remain; live DEX integration deferred

## Reason

A shared payout policy permits adapter integration without choosing a DEX or promising a liquidity result.

## Decision

Creation saves immutable `graduation_policy { launch_bps, collateral_bps }`, an optional adapter program and opaque configuration. Each share is an integer in `0..=10_000`. Configuration requires an adapter; nonzero shares without one are invalid. Saved terms cannot change or be overridden at execution.

Eligible balances are accounted unsold launch inventory and recorded collateral, including the weighted seed. For each asset, the adapter receives `floor(eligible_balance * asset_bps / 10_000)` and the current creator receives the remainder. Checked arithmetic must support the full balance range without intermediate overflow. Assign each eligible unit once. There is no curve `R` allocation or issuance, unsold burn or separate seed refund. The seed remains a funding, pricing and sell-budget protection term. [Accrued fees](0015-fee-accounting.md) go to the current treasury; surplus stays in custody. Separate creator allocation is excluded from the split and keeps its release rule.

Without an adapter, creator-authorized manual settlement pays all eligible balances to the current creator. With an adapter, every manual payout entry point rejects bypass, including zero shares and completed retries. Fee collection and separate allocation claims remain independent. Shared route and completion rules in `crates/sale-policy` prevent competing and duplicate payouts, including zero-amount completion.

After setup completes and the [ending policy](0025-sale-closing.md) permits settlement, anyone can submit the separate public hook transaction. Closure does not call it. Selected depletion can leave no launch inventory to split. Payout preserves the unsold snapshot and cannot create depletion evidence. Private manual settlement is unsupported because the temporary token implementation has no private holdings. Automation and payer policy remain deferred.

Adapter setup uses one external `Initialize` call with required kind `Curve`, `FixedPrice` or `Weighted`. It is permissionless, with no unready state, activation or reinitialization. An authenticated read of saved factory state verifies the sale kind and selected adapter program before initialization commits. The factory must exist but need not be active. The adapter claims its canonical custody PDA derived from the saved sale identity. The factory rejects missing or wrongly owned custody before preparing canonical ATAs; it cannot claim custody for the adapter. Preparation is idempotent.

Each kind-tagged request carries both asset amounts. The adapter verifies its kind and caller and authorizes its own PDAs. The callback binds the factory to the full request hash. The factory forwards no arbitrary remaining accounts and grants no factory PDA seed to the adapter. Runtime transaction signer permissions still apply. See the [handler](../../crates/factory-core/src/hook.rs) for the message contract.

The factory resolves the current creator through role-authority `Resolve`. A caller hint admits expected accounts; the verified reply determines the recipient and grants no caller authority. Pending replies bind the saved pool, factory, adapter and request receipt before progress commits. Final adapter and creator transfers, callback and `Graduated` marker succeed or roll back together. Earlier completed preparation stays complete. Failed hooks permit retry only, with no manual override or adapter replacement. `Graduated` proves callback success, not DEX liquidity.

## Consequences

Settlement is atomic per transaction, not across the workflow. A broken immutable adapter can lock payouts; retry cannot repair it. See [execution limits](../architecture/workspace-guide.md#execution-and-evidence-limits) and [verification boundaries](0022-verification-boundaries.md) for evidence requirements.

## Sources

- [Pinned LEZ ATA creation and default-owner claim](https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lez/programs/associated_token_account/src/create.rs)
- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Curve PoC settlement decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0007-factory-closure-and-creator-settlement.md)
