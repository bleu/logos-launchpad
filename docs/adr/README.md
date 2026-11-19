# Architecture decision records

These records state current decisions. `Accepted` does not mean implemented or production-ready. `Logos review pending` means `To be reviewed by Logos team`; internal acceptance does not establish external approval. Weighted math is unaudited.

Read [CONTEXT.md](../../CONTEXT.md) for terms. See the [workspace guide](../architecture/workspace-guide.md#execution-and-evidence-limits) for current checks and limits, not a test report in each ADR. Private routes, real proofs, package loading and deployment readiness remain unverified.

| ADR | Topic | Status |
| --- | --- | --- |
| [0001](0001-monorepo.md) | One maintained monorepo | Accepted |
| [0002](0002-in-repo-weighted-math.md) | Independent in-repo weighted math | Accepted; unaudited |
| [0003](0003-source-reuse-and-licenses.md) | Source reuse and licenses | Accepted |
| [0004](0004-upstream-patterns.md) | AMM patterns without a fork | Accepted |
| [0006](0006-namespaces-and-authority.md) | Namespaces and separate role authority | Accepted |
| [0007](0007-typed-sale-inputs.md) | Typed immutable sale inputs | Logos review pending |
| [0008](0008-curve-allocation-and-close.md) | Curve allocations | Logos review pending |
| [0009](0009-curve-arithmetic.md) | Curve arithmetic and backing | Accepted baseline |
| [0010](0010-weighted-integer-arithmetic.md) | Weighted integer arithmetic | Accepted; unaudited |
| [0011](0011-weighted-precision.md) | Weighted precision | Accepted; unaudited |
| [0012](0012-weighted-series.md) | Weighted series | Accepted; unaudited |
| [0013](0013-weighted-exact-output.md) | Weighted exact output | Accepted; unaudited |
| [0014](0014-weighted-math-evidence.md) | Weighted math evidence | Accepted; unaudited |
| [0015](0015-fee-accounting.md) | Collateral fee snapshots, formulas and collection | RFP-016 departures; Logos review pending |
| [0017](0017-quotes-and-slippage.md) | Quotes and slippage | Accepted |
| [0018](0018-private-participation.md) | Atomic private route and recovery | Accepted design; private execution pending |
| [0019](0019-product-packages.md) | Two QML apps and shared core | Accepted design; packaging unverified |
| [0020](0020-settlement-and-graduation.md) | Shared payouts and adapter initialization | Implemented; live DEX deferred |
| [0021](0021-modes-and-open-limits.md) | Trade modes, allowlists and allocation ceiling | Public admission implemented; wallet/private route pending; optional per-block ceiling implemented |
| [0022](0022-verification-boundaries.md) | Verification boundaries | Accepted |
| [0025](0025-sale-closing.md) | Ending and payout eligibility | Implemented; RFP-015 expiry departure needs Logos review |
| [0026](0026-lbp-clock-and-poke.md) | Weighted clock and observations | Upstream clock in normal builds; local execution limits remain |
| [0027](0027-events-and-history.md) | Events and history | Emission implemented; ingestion unverified; retention departure needs Logos review |
| [0028](0028-token-custody-and-freeze.md) | Custody and emergency freeze | Accepted; platform limits remain |
| [0029](0029-wallet-integration.md) | Wallet Provider SDK | Accepted; LP-0021 integration pending |
| [0030](0030-lbp-pricing-collateral.md) | Weighted pricing collateral invariant | Accepted; fee departures need Logos review |
| [0031](0031-shared-launch-factory.md) | Shared factory and policy boundaries | Accepted; public paths implemented |
| [0032](0032-fixed-price-lbp-setup.md) | Fixed-price setup and funding | Implemented; local execution evidence |
| [0033](0033-fixed-price-lbp-recorded-collateral.md) | Fixed-price recorded collateral invariant | Accepted; implemented |
| [0034](0034-lez-v04-migration.md) | Actor target and temporary compatibility | Accepted; readiness gaps remain |
| [0036](0036-graduation-adapter-delivery.md) | Plain-transfer distribution adapter; liquidity deferred | Implemented; local guest evidence; liquidity deferred |

## Editing decisions

Edit the governing record to state the current decision, short reasons and limits or open questions. Remove obsolete rules and amendment narratives. Keep safety constraints, pending external approvals and evidence limits. Use public sources, short plain English and one paragraph per line. Keep implementation status distinct from design acceptance; link to maintained evidence instead of copying result inventories.

Keep retained IDs and filenames stable. Use a new unique ID only for a separate decision. When merging records, update links to the retained topic and check anchors. Do not renumber records or fill unused IDs.
