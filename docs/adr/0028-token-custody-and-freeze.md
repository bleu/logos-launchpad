# ADR 0028: Token custody and freeze boundaries

Status: Accepted; local execution evidence has platform and readiness limits.

## Decision

Use compatible LP-0013 token/mint authority and ATA custody mechanisms. Namespace administration, creator authority, token custody and RFP-002 pause/freeze integration are separate capabilities. Verify explicit instruction gates on the selected platform; an administration extension does not supply token custody.

### Namespace emergency control

Each namespace has separate emergency authority and freeze state. Deployment selects the initial authority; creation must check it and cannot accept an initializer's substitute. New namespaces start unfrozen. Namespace state follows [ADR 0006](0006-namespaces-and-authority.md).

Freeze blocks trades in all three engines. Poke, fee collection, settlement, public recovery, administration and role transfer keep their normal checks. Creator resume cannot bypass freeze. Freeze does not stop weighted weights or move sale ends. Namespace controls must remain isolated.

Only the current emergency authority can transfer its role. Transfer is immediate, needs no successor acceptance and preserves freeze state. Deployment and administrator roles cannot replace it after setup. Renunciation is forbidden. An account can hold both roles, but each action checks its own authority. Administrator transfer or renunciation preserves emergency authority and freeze.

Authorized repeated freeze/unfreeze and transfer-to-self calls succeed without a change. Unauthorized calls fail even if they would change nothing. Token and mint freeze authority remain separate; engines retain separate handlers and state.

## Reason and limits

Emergency control must survive administrator changes without granting administrator custody rights. The build selects initial authority through `CURVE_EMERGENCY_AUTHORITY`; missing, malformed and default IDs fail the build. Tests supply a fixture account.

The helper subset adapts reviewed authority and freeze behavior and retains its [MIT license](../../third-party/spel-emergency/LICENSE-MIT). Do not add automatic instruction wrapping, per-account freeze, administrator-selected or administrator-recoverable emergency authority, emergency renunciation or successor-signature checks. Those conflict with this policy.

Host checks alone do not prove token movement or rollback. See [current execution evidence](../architecture/workspace-guide.md#execution-and-evidence-limits) and [verification requirements](0022-verification-boundaries.md#milestone-and-interface-boundary). Broader platform compatibility and production readiness remain unverified.

## Sources

- [Authority helper source](https://github.com/mmlado/spel-authority/blob/97af481b29b5364589776beb4c088ebe234ad7ed/src/lib.rs)
- [Freeze helper source](https://github.com/mmlado/spel-freeze-authority/blob/a4743468bd3a87093931c48c965cc084aa4690c3/freeze-authority/src/config.rs)
- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
