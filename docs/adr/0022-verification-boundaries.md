# ADR 0022: Explicit verification boundaries

Status: Accepted

Date: 2026-09-29; milestone-boundary review: 2026-10-08

## Context

The imported curve CI omitted client/CLI tests and guest builds. Root Cargo commands do not cover every nested workspace. Math parity and host tests alone cannot prove production integration.

## Decision

Keep local tests with each package. Put cross-package build, pin, IDL and integration checks in root `verify/`, called by root CI. Preserve separate guest workspaces and pinned toolchains/lockfiles. Explicitly include client/CLI tests, guest builds and weighted-math integration.

Repair stale E2E commands before execution and use disposable state. Report ignored, unavailable and opt-in checks. Verify each mechanism's qualified math limits, rounding and trade guards, plus namespace isolation, custody, clock, events, administration, freeze authority, private-route failure behavior and Basecamp packaging.

For [curve settlement](0020-settlement-and-graduation.md), use handler tests for account guards, stages and emitted calls. Real guest tests must check payout balances, callback execution, rollback, retry and manual recovery. Record zero-amount and replay coverage at the tested layer; every host guard need not be repeated in guest tests. Emitted calls alone do not prove rollback. Current results and limits belong in the [curve evidence record](../product/curve-contract.md#acceptance-evidence).

### Milestone and interface boundary

M1 owns program handlers, on-chain source IDLs, generated artifacts and local guest evidence for the public curve and LBP programs. Handler and guest checks must cover authority, custody, explicit trade limits, lifecycle, failure behavior and the [namespace emergency control](0028-token-custody-and-freeze.md#namespace-emergency-control). Record the tested layer and any missing coverage. Development receipts are local evidence, not production proofs.

When M1 changes an interface, update generated artifacts and existing callers so they still build and pass their current checks. Complete SDK and CLI journeys belong to M2. Private routes, allowlist proofs, event history, product apps, live deployment and production readiness remain outside M1. These limits assign delivery and evidence to milestones; they do not remove product requirements.

M1 does not lock interfaces or promise compatibility with the imported PoC. Interfaces can change during development. A change needs matching handlers, source IDLs, generated artifacts and affected callers, plus a compatibility review that states any account-state or wire break. Do not use hand-edited generated output to hide drift. The [milestone plan](../../plans/m1-m4-replan.md) assigns program slices and later product journeys.

## Consequences

Historical pins support reproduction; they do not select a production platform. This decision does not install CI or establish production readiness.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
- [Weighted-math evidence](https://github.com/bleu/lez-weighted-math/tree/f99195f4bcc375940171e4ee5373e67592622800)
