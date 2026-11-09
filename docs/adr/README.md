# Architecture decision records

`Accepted` means an adopted design or reviewed baseline, not necessarily shipped behavior. RFP departures marked `To be reviewed by Logos team` lack external approval. Weighted math remains unaudited. The application is migrated to the selected actor revision and the listed local checks passed. Earlier guest and development-receipt evidence alone does not establish compatibility on that revision; see [0034](0034-lez-v04-migration.md) for remaining gaps.

Read [CONTEXT.md](../../CONTEXT.md) for terms and [repository layout](../architecture/repository-layout.md) for planned ownership and checks.

## Repository and program boundaries

| ADR | Decision | Status |
| --- | --- | --- |
| [0001](0001-monorepo.md) | One maintained monorepo | Accepted |
| [0002](0002-in-repo-weighted-math.md) | Reusable weighted math inside the repo | Accepted |
| [0003](0003-source-reuse-and-licenses.md) | Direct source reuse and licenses | Accepted |
| [0004](0004-upstream-patterns.md) | Adopt AMM patterns without a fork | Accepted |
| [0005](0005-mechanism-boundaries.md) | Separate mechanism rules and pools; share matching launch setup | Accepted; partially superseded for shared setup by [0031](0031-shared-launch-factory.md) |
| [0006](0006-namespaces-and-authority.md) | Separate roles using AccountId authority | Accepted; separate curve creator and direct pool-owner roles implemented; see [execution evidence](../architecture/workspace-guide.md#execution-and-evidence-limits); fixed-price sale creation and funding gap partially superseded by [0032](0032-fixed-price-lbp-setup.md) |
| [0007](0007-typed-sale-inputs.md) | Typed immutable sale inputs | Accepted internally; To be reviewed by Logos team |
| [0008](0008-curve-allocation-and-close.md) | Curve allocations | Accepted internally; To be reviewed by Logos team |
| [0009](0009-curve-arithmetic.md) | Checked curve pricing and reserve solvency | Accepted baseline |

## Weighted math

| ADR | Decision | Status |
| --- | --- | --- |
| [0010](0010-weighted-integer-arithmetic.md) | Raw integer amounts and localized widening | Accepted unaudited baseline |
| [0011](0011-weighted-precision.md) | Public/internal precision and cancellation | Accepted unaudited baseline |
| [0012](0012-weighted-series.md) | Logarithm and exponential series | Accepted unaudited baseline |
| [0013](0013-weighted-exact-output.md) | Exact-output inversion and reserve cap | Accepted unaudited baseline |
| [0014](0014-weighted-math-evidence.md) | Independent numerical and parity evidence | Accepted unaudited baseline |

## Product and release decisions

| ADR | Decision | Status |
| --- | --- | --- |
| [0015](0015-fee-accounting.md) | Collateral fees and current-treasury collection | Accepted internally; fixed-price fees implemented; weighted-sale fees implemented with local guest checks; RFP departures: To be reviewed by Logos team |
| [0016](0016-lbp-live-fees.md) | Live fixed-price and weighted-sale fees and anytime collection | Accepted internally; fixed-price fee burden and pricing superseded by [0035](0035-fixed-price-swap-fees.md); weighted-sale fees remain creator-borne and are implemented with local guest checks; To be reviewed by Logos team |
| [0017](0017-quotes-and-slippage.md) | Fresh quotes and explicit slippage limits | Accepted |
| [0018](0018-private-participation.md) | SDK-managed single-transaction private route and recovery | Product and recovery rules accepted; platform, wallet and proof verification pending |
| [0019](0019-product-packages.md) | Two QML mini-apps and a non-UI shared core | Accepted |
| [0020](0020-settlement-and-graduation.md) | Manual settlement and optional adapter hook | Accepted and implemented for bonding-curve settlement; fixed-price and weighted-sale settlement follow [0025](0025-sale-closing.md); production readiness and live DEX integration pending |
| [0021](0021-modes-and-open-limits.md) | Sale types, trade modes, deferred ceiling and allowlist rules | Accepted internally; ceiling deferral: To be reviewed by Logos team; proof design open |
| [0022](0022-verification-boundaries.md) | Verification and M1/M2 scope | Accepted |
| [0025](0025-sale-closing.md) | Mechanism-specific sale closing | Accepted internally; fixed-price settlement has host-handler coverage only; weighted-sale settlement and rollback have local guest checks; expiry-validation departure: To be reviewed by Logos team |
| [0026](0026-lbp-clock-and-poke.md) | Weighted-sale clock reads and single-timestamp pricing | Accepted; pinned upstream timestamp reads and observer checkpoints; private-route and sequencer evidence limits remain |
| [0027](0027-events-and-history.md) | Platform events and existing history tooling | Accepted internally; failed-event retention: To be reviewed by Logos team; actor event emission and ingestion unverified |
| [0028](0028-token-custody-and-freeze.md) | Token custody and namespace emergency controls | Accepted; actor bonding-curve controls have host checks; fixed-price buy freeze has seeded guest checks; weighted frozen buys and payouts have local guest checks; broader platform checks pending |
| [0029](0029-wallet-integration.md) | Integrate the LEZ Wallet Provider SDK | Accepted; awaits LP-0021 development |
| [0030](0030-lbp-pricing-collateral.md) | Fee-neutral weighted-sale pricing collateral and separate custody records | Accepted internally; seeded buy and fee-collection guest checks; weighted setup guest checks passed; weighted settlement and rollback guest checks passed; fee departures: To be reviewed by Logos team |
| [0031](0031-shared-launch-factory.md) | Share matching launch setup without sharing mechanism rules | Fixed-price and weighted setup implemented; weighted setup, buy and rollback guest checks passed; real proofs and deployment readiness pending |
| [0032](0032-fixed-price-lbp-setup.md) | Fixed-price sale creation and funding | Migrated four-stage implementation; local host and guest-build checks passed; setup execution, real proofs and deployment readiness pending; collateral activation rule superseded by [0033](0033-fixed-price-lbp-recorded-collateral.md) |
| [0033](0033-fixed-price-lbp-recorded-collateral.md) | Recorded fixed-price sale collateral excludes unsolicited vault transfers | Accepted; fixed-price fee burden and pricing superseded by [0035](0035-fixed-price-swap-fees.md); recorded collateral rules remain; settlement has host-handler coverage under [0025](0025-sale-closing.md) |
| [0034](0034-lez-v04-migration.md) | Fixed development target, temporary local compatibility support and maintained tooling | Application and development base migrated; local checks include seeded curve trades and fixed-price buys, fees and rollback; see the [workspace guide](../architecture/workspace-guide.md#execution-and-evidence-limits) for remaining gaps |
| [0035](0035-fixed-price-swap-fees.md) | Fee-first fixed-price buys; curve and future weighted-sale policy unchanged | Accepted internally; trader-borne fixed-price fees are an additional RFP-016 departure: To be reviewed by Logos team |

IDs 0023 and 0024 are intentionally unused.

## Changing a decision

Give new records unique IDs. Keep one paragraph per line. Cite public sources and state context, decision and consequences. If a decision changes, add or amend a record with an explicit supersession link; do not silently erase its history.
