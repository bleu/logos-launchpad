# ADR 0028: Token custody and freeze boundaries

Status: Accepted; platform compatibility and instruction gates remain unverified

Date: 2026-09-29; emergency-control review: 2026-10-08

## Context

Token or mint authority, ATA vault ownership, namespace administration and pause or freeze controls are distinct capabilities. An administration extension alone does not provide token custody or freeze support.

## Decision

Use LP-0013 platform mechanisms for token and mint authority and associated token-account vault ownership. Keep namespace administration under [ADR 0006](0006-namespaces-and-authority.md), separate from creator authority and token custody.

Treat RFP-002 pause and freeze integration as a separate concern. Reuse platform mechanisms where compatible, but verify compatibility and explicit instruction gates on the selected platform.

### Namespace emergency control

Each namespace has its own emergency authority and freeze state. The deployment selects the initial authority. Namespace creation must check this selection. The initializer cannot select a different authority. Namespace state remains as defined in [ADR 0006](0006-namespaces-and-authority.md).

New namespaces start unfrozen. Freeze stops curve swaps and LBP buys only. Other actions retain their normal checks. These include poke, fee collection, settlement, public recovery, administration and authority transfer. Creator resume cannot permit swaps while the namespace is frozen. Freeze does not stop weights or change sale end times. Controls in one namespace must not affect another.

Only the current emergency authority can transfer its role. Transfer is immediate and keeps the freeze state. No successor acceptance is required. After setup, deployment and administrator roles cannot replace it. Renunciation is not permitted. One account can hold both administrator and emergency roles. Check each role separately. Administrator transfer or renunciation must keep the emergency authority and freeze state.

Authorized repeated freeze and unfreeze calls succeed. Transfer to the current authority also succeeds. Unauthorized calls fail, including calls that change nothing.

This decision supersedes the [earlier freeze policy](https://github.com/bleu/logos-launchpad/blob/e88e70c/docs/adr/0028-token-custody-and-freeze.md). Token and mint freeze authority remain separate. Curve and LBP keep separate handlers and state.

## Consequences

These rules still need implementation. Tests must cover the controls above and unchanged state after failed calls. Administration integration does not prove custody or freeze compatibility. See [M1 evidence requirements](0022-verification-boundaries.md#milestone-and-interface-boundary).

The [historical authority experiment](https://github.com/bleu/logos-launchpad/blob/f1198315569cfc8ada9f6c39964cb77314e2520c/packages/platform-probes/authority/src/lib.rs) used freeze `a4743468bd3a87093931c48c965cc084aa4690c3` (v0.1.5) with the component stack recorded in [ADR 0006](0006-namespaces-and-authority.md). It checked fixed emergency authority, namespace isolation and swaps-only freeze admission. Fee collection, settlement and creator recovery remained available under their own checks. These were mock operations with saved creator/custody references, not real swaps or token transfers. Guest checks used controlled authorization flags and development receipts. The experiment is no longer maintained; lasting coverage belongs with the real curve controls. LBP integration remains unverified.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
