# ADR 0019: Two QML mini-apps and a non-UI shared core

Status: Accepted

Date: 2026-09-29

## Context

Administrators, creators and participants need different product flows. Operational history and monitoring remain in scope, while the derived analytics view is excluded. Basecamp QML reads network-backed data through backend modules. This product placement refines the public proposal.

## Decision

Deliver two separately packaged QML mini-apps: an admin app for namespace administration only, and a configured-sales app for creator operations, participant listings, quotes, supported purchases and history within a configured namespace. App placement does not grant or merge namespace-administrator, creator or participant rights.

Deliver one separate non-UI shared core module. The core can be packaged as an `.lgx` module, but it is not a mini-app. Three planned `.lgx` packages therefore do not mean three mini-apps.

Keep business logic and transaction construction in the planned Rust SDK and shared core. The CLI calls the SDK directly. QML calls the QML and Basecamp module API, which connects through a planned generated module adapter to the shared core by a planned C ABI. The C ABI defines compiled-code calling rules in the documented Rust module build path. Compatibility remains unverified.

The Basecamp QML runtime bridge is a separate platform runtime facility for module calls. It is not the generated module adapter and does not prove C ABI compatibility. A module `.lidl` contract describes the app-facing module interface used to generate the planned adapter. An on-chain program IDL remains a separate transaction-interface artifact. Neither is a runtime transaction service.

Both QML apps request relevant indexed listings, history and monitoring through a backend module read API. Use the planned shared-core read boundary unless implementation review requires another module. Do not create a new indexer, add direct QML HTTP calls to the indexer, or restore the excluded analytics view. Indexed data can lag and does not authorize a transaction; contracts enforce current state.

Integrate the LP-0021 Wallet Provider SDK once developed; see [ADR 0029](0029-wallet-integration.md).

## Consequences

A package split needs build, install and user-flow evidence. A working Rust library is not proof of a working Basecamp package. Shared core changes must preserve both apps' contracts. The admin app does not own creator operations, and the configured-sales app does not own namespace administration. Each action must enforce its own authority rules.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
