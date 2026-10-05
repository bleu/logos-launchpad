# Repository baseline and planned layout

The repository contains an imported PoC baseline and future product design. The root curve crates, `cli/`, `methods/`, `idl-src/` and `idl/` retain their existing behavior. Weighted math is an independent nested workspace at `packages/weighted-math/`. These imports do not implement the full production specification. `packages/launchpad/` has initial product package entry points. The LBP, full shared-core business logic and product app workflows remain future work. Keep working PoC boundaries before changing names or extracting shared code. See [architecture and interfaces](system.md) for component diagrams, the [product brief](../product/prd.md) for use cases and the [workspace guide](workspace-guide.md) for commands.

## Code ownership

Ownership here means responsibility for behavior and review. Named maintainers and `CODEOWNERS` rules are not configured. They are optional and do not block implementation pull requests or imply branch protection.

| Path or future component | Owns | Must not own |
| --- | --- | --- |
| `crates/curve-math/` | Checked constant-product arithmetic | Accounts, namespace policy, launch allocations |
| `crates/pool/` | Neutral curve reserves, swaps, custody and pool lifecycle | Creator allocations, LBP lifecycle |
| `crates/curve-core/` | Curve wire and account state, namespace validation, factory custody and authorization | Product UI, curve factory allocation policy or a selected cross-factory namespace schema |
| `crates/factory-core/` | Curve factory supply, allocation, creator authorization and staged settlement | Reimplementation of curve pricing or LBP factory policy |
| `crates/private-flow-core/` | Existing private helper and its on-chain validation | The complete SDK privacy or provider integration contract |
| Future `crates/lbp-pool/` | LBP clock weights, state, trading, custody, creator-borne collateral fees, current-treasury fee collection and lifecycle | Curve semantics hidden behind a generic pool interface |
| Future `crates/lbp-factory/` | LBP creation validation, sale-to-namespace binding and LBP sale policy | Curve allocations, creator settlement rules or unreviewed LBP permissions |
| `packages/weighted-math/` | Standalone math kernel, harness, fixtures, proof material, fuzz target and parity workspace | Custody, authorities or product flow |
| `methods/guest/` | Thin dispatch entry points and guest-specific build profiles | A second copy of handler logic |
| `crates/launchpad-client/` | Quotes, authoritative reads, transaction construction and the PoC single-transaction private-buy flow | QML state, direct indexer HTTP calls or duplicated pricing rules |
| `cli/` | Argument parsing, command output and SDK calls | Independent protocol logic |
| `packages/launchpad/core/` | Initial native `launchpad_core` module and pure, tested `get_status`; planned stable non-UI SDK boundary, backend module read API and generated app-facing glue from a module `.lidl` contract | Separate pricing or transaction rules, user-key custody or an on-chain program IDL |
| `packages/launchpad/apps/admin/` | Initial `launchpad_admin_app` entry point; planned namespace administration and indexed namespace monitoring only | Creator or participant operations |
| `packages/launchpad/apps/configured-sales/` | Initial `launchpad_configured_sales_app` entry point; planned creator and participant listings, sale configuration, quotes, supported trades and indexed history | Namespace administration or direct indexer HTTP calls |

The LBP's live-rate and anytime current-treasury collection design is accepted internally. Its RFP departures have external status `To be reviewed by Logos team` in [ADR 0016](../adr/0016-lbp-live-fees.md).

Pure math is below state machines; program handlers add platform accounts and calls; thin guests dispatch handlers. Clients build invocations against these contracts. CLI and product interfaces call shared client/core logic. Planned QML workflows request indexed listings, history and monitoring through the planned shared-core backend module read API, not direct indexer HTTP calls. The indexed read boundary can use a different backend module only after implementation review; this layout does not select one. Shared utilities need matching behavior and error contracts before extraction. Separate mechanisms can share tools without sharing state models.

Weighted math is an imported independent workspace under `packages/weighted-math/`. A future LBP can consume its `crates/weighted-math-core/` library by Cargo path. Root workspace configuration must exclude nested workspaces as needed. Keep lockfiles and toolchain pins for each build context; do not assume that a root `cargo test --workspace` covers nested math, guest or fuzz workspaces. See the [workspace guide](workspace-guide.md) for commands.

## Artifacts

| Artifact | Producer/owner | Consumer | Required check |
| --- | --- | --- | --- |
| On-chain source IDLs in `idl-src/` | Program interface owner | On-chain program IDL generator | Handler and interface review |
| Generated on-chain program IDLs in `idl/` | Existing source-IDL generation path | SDK, CLI and bindings | `verify/check-idl.sh` regeneration and drift check |
| Guest binaries and program identities | Separate `methods/guest/` build | Deployment and client configuration | Pinned guest build and identity consistency |
| Weighted kernel library | Math package | LBP program and test harness | Math tests, input qualification and guest parity |
| Client library and CLI binary | `launchpad-client` and `cli` | Developers and operators | Include their tests, not only pure/core crates |
| Core module `.lidl` (`core-lidl`) | `packages/launchpad/core/` | Generated adapter | `get_status` interface check |
| Generated `launchpad_core` module | `packages/launchpad/core/` native module build | Basecamp module API integration | Pure logic tests and package smoke check |
| Core `.lgx` (`core-lgx`) | `packages/launchpad/core/` packaging | Admin and configured-sales apps | Build, install and `get_status` call check |
| Admin `.lgx` (`admin-lgx`) | `packages/launchpad/apps/admin/` | Namespace administrators | Build, install and `Test backend` call check |
| Configured-sales `.lgx` (`configured-sales-lgx`) | `packages/launchpad/apps/configured-sales/` | Creators and participants | Build, install and `Test backend` call check |

Any planned generated interface must be reproducible. Do not edit generated output to fix drift. Do not commit deployment keys. `packages/launchpad/` contains real initial product package entry points: `launchpad_core`, `launchpad_admin_app` and `launchpad_configured_sales_app`. Its flake exports `core-lgx`, `admin-lgx` and `configured-sales-lgx`. The apps only provide `Test backend` -> `Backend OK`, backed by pure, tested `get_status`; see the [package README](../../packages/launchpad/README.md) for package-specific build and evidence. Full SDK and shared-core business logic, namespace administration and sales workflows remain unimplemented. A production shared-core module `.lidl` contract and cross-platform compatibility remain unverified. On-chain program IDLs remain separate transaction-interface artifacts. The Basecamp QML runtime bridge is a platform runtime concern, not generated C ABI glue. Only the QML packages are mini-apps. Weighted math is a Rust library, not another `.lgx` application.

## Where checks live

Package-local tests cover local arithmetic, state and API behavior. Root `verify/` scripts cover cross-package builds, dependency pins, on-chain program IDL drift and integration. Root `.github/workflows/` calls those checks with explicit toolchains and working directories. Copied workflows under the math subtree will not run there automatically.

Retain useful pin and on-chain program IDL checks plus the math harness when source code is copied. Host and guest builds, client/CLI tests, on-chain program IDL drift, pins and weighted-math integration need explicit commands or jobs. Guest parity, fuzzing and platform/package tests also need explicit jobs or commands; record checks that are not run by default.

Live E2E verification remains future work and must use isolated disposable wallet and localnet state. Ignored tests, unavailable tools and missing proof/package evidence must remain visible failures or gaps, never implicit passes.

## Review rules

A public interface change needs matching on-chain source IDL or module `.lidl` contract, generated output, client behavior and compatibility review. A math change needs updated numerical evidence and a review of its input envelope. A lifecycle change needs authorization, accounting and settlement tests. A packaging change must prove that its module can be installed and called. The [verification ADR](../adr/0022-verification-boundaries.md) defines the evidence limits.
