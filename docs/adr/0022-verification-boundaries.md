# ADR 0022: Explicit verification boundaries

Status: Accepted

Date: 2026-09-29

## Context

The imported curve CI omitted client/CLI tests and guest builds. Root Cargo commands do not cover every nested workspace. Math parity and host tests alone cannot prove production integration.

## Decision

Keep local tests with each package. Put cross-package build, pin, IDL and integration checks in root `verify/`, called by root CI. Preserve separate guest workspaces and pinned toolchains/lockfiles. Explicitly include client/CLI tests, guest builds and weighted-math integration.

Repair stale E2E commands before execution and use disposable state. Report ignored, unavailable and opt-in checks. Verify each mechanism's qualified math limits, rounding and trade guards, plus namespace isolation, custody, clock, events, administration, freeze authority, private-route failure behavior and Basecamp packaging.

For [curve settlement](0020-settlement-and-graduation.md), use handler tests for account guards, stages and emitted calls. Real guest tests must check payout balances, callback execution, rollback, retry and manual recovery. Record zero-amount and replay coverage at the tested layer; every host guard need not be repeated in guest tests. Emitted calls alone do not prove rollback. Current results and limits belong in the [curve evidence record](../product/curve-contract.md#acceptance-evidence).

## Consequences

Historical pins support reproduction; they do not select a production platform. This decision does not install CI or establish production readiness.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
- [Weighted-math evidence](https://github.com/bleu/lez-weighted-math/tree/f99195f4bcc375940171e4ee5373e67592622800)
