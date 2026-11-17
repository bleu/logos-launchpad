# Repository layout

The root application uses the selected LEZ actor revision. Nested workspaces keep separate lockfiles and build contexts. See [workspace commands and evidence limits](workspace-guide.md). The [system overview](system.md) shows component boundaries; [ADR 0031](../adr/0031-shared-launch-factory.md) defines shared policy and separate pricing/custody rules.

## Responsibilities

These are code and review boundaries, not assigned maintainers or branch-protection rules.

| Path | Responsibility |
| --- | --- |
| `crates/curve-math/` | Checked constant-product arithmetic, without accounts or sale policy |
| `crates/pool/` | Neutral curve reserves, swaps and lifecycle |
| `crates/curve-core/` | Curve actor messages, namespace checks, custody and authorization |
| `crates/factory-core/` | Shared launch setup and separate typed sale inputs; allocation and factory lifecycle |
| `crates/lbp-core/` | Fixed-price and weighted pool state, setup, pricing, trades, fees, controls and settlement |
| `crates/sale-policy/` | Shared fee, trade-control, ending and settlement-route rules; no custody state |
| `crates/eligibility-policy/` | Shared public-trader membership checks |
| `crates/private-flow-core/` | Private helper boundary; currently returns unsupported errors |
| `packages/weighted-math/` | Independent unaudited math kernel, numerical fixtures, harness, fuzzing and guest parity; no sale policy |
| `packages/lez-compat/` | Temporary token, ATA and role-authority support; no replacement wallet, network-fee mechanism or execution engine |
| `methods/guest/` | Thin actor dispatch and guest build profiles, without duplicated handler logic |
| `crates/launchpad-client/` | Quotes, current-state reads and actor transaction construction |
| `cli/` | Argument parsing, output and client calls, without independent protocol logic |
| `packages/launchpad/core/` | Planned non-UI SDK and backend read boundary; current package is a status placeholder |
| `packages/launchpad/apps/admin/` | Planned namespace-administration app; current package is a status placeholder |
| `packages/launchpad/apps/configured-sales/` | Planned creator/participant app; current package is a status placeholder |

CLI and QML must use shared client/core rules. Planned QML workflows read indexed data through a backend module API, not direct indexer HTTP calls. The shared core must not hold user keys. See [wallet integration](../adr/0029-wallet-integration.md) and [package requirements](../../packages/launchpad/README.md).

## Artifacts and checks

| Artifact | Producer | Required check |
| --- | --- | --- |
| Actor messages and generated `idl/launchpad.json` | Rust declarations and `verify/generate-idl.sh` | `verify/check-idl.sh`, Borsh wire checks and client review |
| Guest binaries and image IDs | Guest builds | Pinned builds and matching client configuration; program addresses are not image IDs |
| Weighted kernel | Math workspace | Numerical evidence, input qualification and guest parity |
| Client library and CLI | Root workspace | Explicit client/CLI tests; default core tests do not cover them |
| Module `.lidl` contract and native adapter | Launchpad core package | Generated adapter and module compatibility checks |
| Core, admin and configured-sales `.lgx` archives | Package builds | Installation, loading and backend smoke checks |

Module `.lidl` contracts are distinct from on-chain program IDLs. Only the two QML packages are mini-apps; weighted math is a Rust library. Generated interfaces must be reproducible. Do not edit generated output to fix drift or commit deployment keys.

Package-local tests check local behavior. Root `verify/` scripts check builds, pins, wire formats and integration. Root CI selects those scripts; copied math workflows do not run automatically. See the [workspace guide](workspace-guide.md#ci) for default and manual checks. Missing tools, ignored tests and deferred proof/package checks are gaps, not passes.

An interface change needs matching declarations, generated output and clients. A math change needs numerical evidence and input-envelope review. A lifecycle change needs authority, accounting, rollback and retry tests. A packaging change needs installation and call checks. See [verification boundaries](../adr/0022-verification-boundaries.md).
