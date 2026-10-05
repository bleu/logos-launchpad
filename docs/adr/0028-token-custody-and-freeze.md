# ADR 0028: Token custody and namespace emergency freeze

Status: Accepted internally; curve core and signed guest tests pass; creator-pause departure: `To be reviewed by Logos team`; production verification remains open

Date: 2026-09-29; emergency-policy review: 2026-10-02

## Context

Token or mint authority, ATA vault ownership, namespace administration and emergency controls are separate capabilities. A bug can affect many sales, so the emergency stop must cover a namespace's swaps without blocking settlement or recovery.

## Decision

Use LP-0013 platform mechanisms for token and mint authority and associated token-account vault ownership. Keep namespace administration under [ADR 0006](0006-namespaces-and-authority.md), separate from creator authority and token custody.

Set one emergency `AccountId` when a namespace is created. Support signing accounts and program-controlled accounts (PDAs). Set the administrator and emergency authority in the same creation transaction; one account may hold both roles. An emergency PDA must already be claimed by its stated owner program. This authority is fixed: no transfer, renunciation or later appointment. Namespace administration changes cannot change it. Creator and administrator roles do not grant emergency control.

Only that emergency authority can freeze or unfreeze swaps throughout its namespace. The switch covers both bonding curves and LBPs, public and private routes, and sales created while the namespace is frozen. Other namespaces remain active. Repeated authorized calls succeed without another state change or event; every call still checks authority.

Freeze blocks swaps only. Fee collection, settlement and recovery retain their normal permissions and checks. Time, weights and closing rules continue unchanged. Unfreezing cannot reopen an ended sale.

This supersedes the creator pause/resume paragraph in [ADR 0021](0021-modes-and-open-limits.md). Creators have no separate pause control. RFP-016 grants the creator emergency pause, so this replacement is `To be reviewed by Logos team`.

## Consequences

The curve v2 namespace config embeds the public primitives from the licensed `spel-freeze-authority` v0.1.5 copy. [Provenance](../../third-party/spel-freeze-authority/PROVENANCE) records its source and local changes. The program exposes no emergency transfer, renounce or later appointment. Initialization sets both roles atomically, including when one `AccountId` holds both. An emergency PDA must already be claimed by the matching owner program; control requires a seed-authorized call from that program. The direct CLI wallet route is signer-only.

This format is for fresh deployments only. Version 2 rejects old namespace state; there is no migration. Rebuild guests and clients together: binaries, program IDs and derived addresses change. The root LEZ pin is unchanged.

## Evidence and limits

[Core namespace tests](../../crates/curve-core/tests/namespace_admin.rs) and [curve handler tests](../../crates/curve-core/src/tests.rs) pass. [Signed namespace guest tests](../../crates/launchpad-client/tests/namespace_admin.rs) execute the actual curve guest and cover atomic role setup, shared signer and PDA roles, owner-program seed authorization, namespace checks and rollback. Admin transfer and renunciation leave emergency control unchanged.

[Settlement guest tests](../../crates/launchpad-client/tests/settlement.rs) pass for the freeze gate on exact-input and exact-output buys and exact-input sells, namespace isolation and factory creation while frozen. They also cover fee collection, settlement and creator recovery while frozen, and show that unfreezing cannot reopen expired or exhausted sales. A narrow actual-guest `Private` swap-wrapper test reaches the same gate without the full deshield envelope.

[Private-authority guest regressions](../../crates/launchpad-client/tests/private_authority.rs) pass. The full deshield/buy/return `Execute` route still reaches `MaxChainedCallsDepthExceeded`; its unchanged regression passes by checking that limit. The wrapper test does not establish full private-route integration.

CLI tests, pinned IDL checks and the full locked build suite pass. Lockfiles remain unchanged during verification. Guest tests use development receipts, not real cryptographic proofs or a deployed-node guarantee. LBP state, events and full private-route integration remain outside this curve integration. The separate [platform probes](../../packages/platform-probes/README.md) retain their own evidence limits.

## Sources

- [RFP-002 freeze authority](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-002-freeze-authority-lib.md)
- [RFP-016 LBP launchpad](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md)
- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
