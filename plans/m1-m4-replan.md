# M1–M4 plan

Status: M1 contract reviewed. Implementation checks remain open.

See the [product goals](../docs/product/prd.md) and [repository layout](../docs/architecture/repository-layout.md). Keep curve and LBP code separate. Keep pending Logos-team reviews visible in the [ADR index](../docs/adr/README.md).

## M1: Public programs

M1 owns handlers, on-chain IDLs, generated artifacts and local guest evidence. Update existing callers when interfaces change. Complete SDK and CLI journeys belong to M2. See [ADR 0022](../docs/adr/0022-verification-boundaries.md#milestone-and-interface-boundary).

Use the [curve contract](../docs/product/curve-contract.md) and [LBP contract](../docs/product/lbp-contract.md) for pricing, fee and lifecycle rules. Deliver these slices in order:

1. Complete the existing public curve baseline and namespace controls. Check administration, custody, trades, fees, closing and settlement. Preserve settlement retries. Graduation does not prove DEX liquidity.
2. Add the fixed-price LBP factory and pool. Check namespace binding, schedule inputs, both buy forms, rounding, limits, fees, custody and payouts after close. Reject oversized buys in full.
3. Add weighted LBP buys, chain-time weights, poke and creator pause/resume. Check both modes, checkpoint updates and unchanged state after failed calls. Pause must not stop weights or move the end.

For each slice, check handlers, IDLs, generated artifacts and guest execution. Test authority, namespace isolation, custody, failed calls and repeated calls. Record missing evidence. Both mechanisms must enforce the [emergency control](../docs/adr/0028-token-custody-and-freeze.md#namespace-emergency-control).

Private routes, allowlist proofs, event history, product apps, live deployment and production readiness are outside M1.

## M2: Public SDK and CLI

Deliver complete public journeys through shared SDK logic and thin CLI commands. Check fresh quotes, explicit limits, full input amounts, fee treatment, authority, failed calls and settlement retries. Fee collection must use the current treasury. Batch collection must report each sale's result.

## M3: Apps and history

Deliver the admin and configured-sales apps through the shared core. Check confirmations, errors, package builds, installation and supported flows. Keep administration in the admin app.

Verify events and indexer capture before adding history and monitoring. Use namespace-scoped backend reads. Show data freshness. Do not use indexed data to authorize transactions. Keep private account data out of events. Record a platform blocker if this path fails; history remains required. Derived analytics are excluded.

## M4: Hardening and deployment

Check full public lifecycles, namespace isolation, custody, numerical limits and failed calls. Record exact platform and tool versions. Prepare audit evidence with trust boundaries and known limits. Deployment requires platform readiness, compatibility evidence and audit approval.

## Separate work

Private trading requires evidence for the one-transaction route, wallet support, participant-controlled returns and outcome recovery. Allowlisted buys require a reviewed proof method, verifier, cost, confidentiality and replay protection. Do not start these flows before their gates pass.
