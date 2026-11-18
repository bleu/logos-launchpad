# ADR 0036: Distribution adapter first; liquidity adapter deferred

Status: Accepted; distribution implemented with local guest evidence; liquidity deferred

Date: 2026-11-18

## Context

The graduation hook permits adapter programs. It does not create a DEX pool. Distribution uses the current token program. Liquidity creation needs a compatible DEX, token program and oracle. Build distribution first without changing the upstream dependency pin. Keep liquidity in a later change.

This work builds on [PR 49](https://github.com/bleu/logos-launchpad/pull/49), branch `yvesfracari/shared-graduation-payouts`, reviewed at `e315593ea1f07eb05229e0a1fd5a851c0604e554`. [ADR 0020](0020-settlement-and-graduation.md) owns factory payouts and required routing. The adapter does not change those rules.

## Decision

Use one distribution program for all three sale types. Support plain transfers of the launch token and collateral. Burns, arbitrary calls, vault actions, DEX operations, staged execution and refunds are outside this version. A transfer to vault custody does not prove vesting, backing or redemption support.

Creation saves versioned configuration with `launch_rules` and `collateral_rules`. Each rule has a destination owner and integer basis points. Each list has one to eight rules and must total exactly 10,000 basis points. Zero basis points are permitted. All owners are fixed at sale creation. Namespace treasury changes and creator role transfers do not change them. Reject adapter custody as a recipient. Repeated recipients are permitted. Fixed amounts and separate remainder rules are excluded.

Initialize through one permissionless call with factory deployment, namespace, launch salt and required sale kind. Read authenticated factory state. Check sale identity, kind, asset definitions and the saved adapter deployment. Validate the saved hook configuration and bind its hash to adapter state. There is no mutable second configuration or activation step. Before creation, the SDK distribution hook builder derives custody from the adapter deployment and sale identity. It applies the same recipient check as initialization. The CLI uses this builder for `create-sale --distribution-configuration`. The generic encoder checks structure and percentages only. Factory creation still treats hook bytes as opaque.

During graduation, authenticate the factory caller and full request. Read both fungible asset definitions and canonical custody holdings after the factory payout. Each balance must cover its request amount. Save and distribute the full custody balance of each sale asset, including direct deposits. The factory request remains unchanged for callback hashing.

For each asset, calculate each early payout as `floor(saved_balance * bps / 10_000)`. The final rule transfers the full remainder, including dust. Use checked arithmetic across the full `u128` range. Skip zero transfers. A zero request still reads custody and distributes any positive balance. A zero balance skips recipient preparation and transfers.

Prepare canonical recipient holdings through the token program. Read initial recipient balances, transfer in rule order, then check exact aggregate recipient credits and zero custody balances. Store the original request receipt and actual graduation amounts. Return the existing request-hash callback only after verification. Transfers, receipt, callback and factory completion commit or roll back together.

Keep private execution state separate from the factory's adapter state contract. `ReadState` returns the compatible idle public view. `ReadDistributionState` returns the full adapter state. Reject external messages while an authenticated continuation is pending. Keep the receipt-only adapter for existing hook tests.

Add permissionless `DistributeRemaining` for later deposits. Require a committed graduation receipt and idle execution state. Use fresh full custody balances and the saved assets, recipients and percentages. Reuse the planner and balance checks. Do not repeat factory payouts or the callback. Preserve the original receipt and graduation amounts. An empty call succeeds without token mutations. Each call is atomic and emits a versioned distribution event with its sequence and actual totals.

## Consequences

Anyone who deposits either sale asset into adapter custody contributes to the saved split. There is no depositor refund right. Other assets have no recovery path in this version. Deposits do not trigger execution; someone must submit graduation or a later distribution call.

Each call rounds separately. Repeated small distributions can give the final recipient more dust than one combined distribution. There is no minimum size or waiting period. Immutable recipients cannot be repaired after creation. A failed required adapter permits retry, not manual withdrawal.

`Graduated` proves successful hook completion, not DEX liquidity. Weighted math remains unaudited. See the [transfer flow](../architecture/sequence-diagrams/distribution-adapter.md) and [execution limits](../architecture/workspace-guide.md#distribution-adapter-checks) for local evidence.
