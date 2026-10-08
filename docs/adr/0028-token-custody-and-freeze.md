# ADR 0028: Token custody and freeze boundaries

Status: Accepted; platform compatibility and instruction gates remain unverified

Date: 2026-09-29; emergency-control review: 2026-10-08

## Context

Token or mint authority, ATA vault ownership, namespace administration and pause or freeze controls are distinct capabilities. An administration extension alone does not provide token custody or freeze support.

## Decision

Use LP-0013 platform mechanisms for token and mint authority and associated token-account vault ownership. Keep namespace administration under [ADR 0006](0006-namespaces-and-authority.md), separate from creator authority and token custody.

Treat RFP-002 pause and freeze integration as a separate concern. Reuse platform mechanisms where compatible, but verify compatibility and explicit instruction gates on the selected platform.

### Namespace emergency control

Each namespace has its own emergency authority and freeze state. The deployment selects the initial emergency authority for that namespace. Namespace creation must validate that selection; the namespace initializer cannot substitute another authority. This rule does not select a registry, shared physical schema or cross-factory account owner.

New namespaces start unfrozen, with no transition period. Normal funding, schedule, lifecycle and trade checks still apply. The emergency authority can freeze or unfreeze its namespace. Freeze stops curve swaps and LBP buys only. Poke, fee collection, settlement, public recovery, namespace administration and emergency-authority transfer remain available under their normal checks. Creator pause is separate; creator resume cannot bypass namespace freeze. Freeze does not stop weights or move a sale's end timestamp. A freeze or authority change in one namespace must not change another namespace.

Only the current emergency authority can transfer its role. Transfer takes effect immediately, without successor acceptance, and preserves the namespace's freeze state. After initial setup, neither the deployment authority nor the namespace administrator can replace the emergency authority through those roles. Emergency authority cannot be renounced. The namespace administrator and emergency authority may be the same account, but each operation must check the required role. Namespace administration transfer or renunciation must preserve the emergency authority and freeze state.

Authorized repeated freeze and unfreeze calls succeed. Transfer to the current emergency authority also succeeds. Each call must still check current authority, even when it changes nothing. Unauthorized calls fail.

This review supersedes the [earlier freeze policy](https://github.com/bleu/logos-launchpad/blob/e88e70c/docs/adr/0028-token-custody-and-freeze.md) and its allowance for later freeze gates on settlement and recovery. The selected emergency control stops swaps only. It does not grant token or mint freeze authority. Separate curve and LBP handlers must enforce the same namespace policy without combining their mechanism state or lifecycle code.

## Consequences

Implementation must separately test authority changes, vault custody, namespace administration, pause and freeze behavior. Checks must cover namespace isolation, deployment-selected initialization, immediate emergency-authority transfer, preserved freeze state, repeated calls, unauthorized calls and permitted operations during freeze. Rejected calls must commit no state change. A successful admin-library integration does not prove token custody or freeze compatibility. These are accepted rules, not evidence of implemented handlers. See [M1 scope and interface evidence](0022-verification-boundaries.md#milestone-and-interface-boundary).

The [historical authority experiment](https://github.com/bleu/logos-launchpad/blob/f1198315569cfc8ada9f6c39964cb77314e2520c/packages/platform-probes/authority/src/lib.rs) used freeze `a4743468bd3a87093931c48c965cc084aa4690c3` (v0.1.5) with the component stack recorded in [ADR 0006](0006-namespaces-and-authority.md). It checked fixed emergency authority, namespace isolation and swaps-only freeze admission. Fee collection, settlement and creator recovery remained available under their own checks. These were mock operations with saved creator/custody references, not real swaps or token transfers. Guest checks used controlled authorization flags and development receipts. The experiment is no longer maintained; lasting coverage belongs with the real curve controls. LBP integration remains unverified.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
