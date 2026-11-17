# ADR 0022: Explicit verification boundaries

Status: Accepted

Date: 2026-09-29; milestone-boundary review: 2026-10-08

## Context

The imported curve CI omitted client/CLI tests and guest builds. Root Cargo commands do not cover every nested workspace. Math parity and host tests alone cannot prove production integration.

## Decision

Keep local tests with each package. Put cross-package build, pin, IDL and integration checks in root `verify/`. Use light host tests, formatting and static checks for local defaults and automatic CI. Keep client/CLI tests, guest builds, weighted-math integration and execution checks available through explicit manual commands and a manual full CI run for now. Preserve separate guest workspaces and pinned toolchains/lockfiles. A passing light check does not establish full compatibility.

Repair stale E2E commands before execution and use disposable state. Report ignored, unavailable and opt-in checks. Verify each mechanism's qualified math limits, rounding and trade guards, plus namespace isolation, custody, clock, events, administration, freeze authority, private-route failure behavior and Basecamp packaging.

For [curve settlement](0020-settlement-and-graduation.md), use handler tests for account guards, stages and emitted calls. Real guest tests must check payout balances, callback execution, rollback, retry and rejection of adapter-configured manual bypass. Record zero-amount and replay coverage at the tested layer; every host guard need not be repeated in guest tests. Emitted calls alone do not prove rollback. Current results and limits belong in the [curve evidence record](../product/curve-contract.md#acceptance-evidence).

### Milestone and interface boundary

M1 owns public bonding-curve handlers and the planned fixed-price and weighted-sale handlers, on-chain source IDLs, generated artifacts and local guest evidence. Checks must cover authority, custody, trade limits, lifecycle, failed calls and [emergency controls](0028-token-custody-and-freeze.md#namespace-emergency-control). Record missing coverage. Development receipts do not prove production readiness.

Interfaces can change during development. Update handlers, source IDLs, generated artifacts and affected callers together. Review changes to stored accounts and wire formats. Existing callers must build and pass their checks. M2 owns complete SDK and CLI journeys. Private routes, allowlist proofs, event history, product apps, live deployment and production readiness are outside M1.

## Consequences

Historical pins support reproduction; they do not select a production platform. This decision does not install CI or establish production readiness.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
- [Weighted-math evidence](https://github.com/bleu/lez-weighted-math/tree/f99195f4bcc375940171e4ee5373e67592622800)
