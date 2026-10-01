# ADR 0022: Explicit verification boundaries

Status: Accepted

Date: 2026-09-29

## Context

The reviewed curve CI omits client/CLI tests and guest builds. Its E2E script has stale commands. Weighted host/guest parity is useful math evidence, but does not prove production integration or proving readiness.

## Decision

Keep local behavior tests with each package. Put cross-package build, pin, on-chain program IDL and integration checks in root `verify/`, called by root CI. Preserve separate guest workspaces and pinned toolchains/lockfiles. Include client and CLI tests, guest builds and weighted-math integration explicitly; root Cargo commands do not cover every nested workspace.

Reuse existing pin and on-chain program IDL checks plus numerical harnesses. Repair E2E commands before execution and use isolated disposable state. Report ignored, unavailable and opt-in checks. Verify custody, clock, events, administration, freeze authority and Basecamp packaging on the selected platform.

Before a mechanism accepts creation or trade inputs, verify its qualified math envelope. Evidence must cover its applicable weight and amount limits, dust guards and oversized-trade guards without assuming undocumented thresholds. Also verify rounding, namespace isolation, single-transaction private-route execution and failure behavior. The amended [ADR 0020](0020-settlement-and-graduation.md) replaces the inactive-hook scope with an executable public adapter hook. Use handler tests for the fixed envelope, account and authority guards, stage transitions and emitted calls. Use real guest execution to verify exact settlement transfers, callback execution, failure rollback, retry and manual fallback. Record zero-amount and replay evidence at its tested layer; do not describe host-covered guards as absent or require every guard to be repeated in the guest lane. Check balances and state after a failed final transaction; emitted child calls alone do not prove rollback. See the [curve contract evidence map](../product/curve-contract.md#acceptance-evidence).

## Consequences

A green host suite is not a release gate by itself. Historical pins are a reproducibility baseline, not an automatic production target. Build reproduction and CI implementation remain separate tasks; no new workflow is installed by this decision.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
- [Weighted-math evidence](https://github.com/bleu/lez-weighted-math/tree/f99195f4bcc375940171e4ee5373e67592622800)
