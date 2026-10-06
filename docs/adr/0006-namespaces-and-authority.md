# ADR 0006: Namespace isolation and AccountId administration

Status: Accepted; target-platform compatibility remains unverified

Date: 2026-09-29; configuration review: 2026-10-01; role-pattern review: 2026-10-06

## Context

Independent operators need separate fee settings and treasuries. Moving a namespace administrator or creator role must not change namespace or sale identity. The confirmed `spel-admin-authority` component provides a standard `AccountId` administration lifecycle, but its compatibility with the selected platform is unverified.

## Decision

Use permissionless namespaces with stable identities. Bind each sale to its namespace at creation. Keep namespace-scoped account derivation and isolated custody through associated token accounts. Namespace state is logical per-namespace state. This decision does not select a namespace registry, shared physical schema or cross-factory namespace account owner.

Embed one `#[admin_authority]` administration state in each namespace and use `#[require_admin]` for namespace administration. Initialize that state with namespace creation. Do not replace per-namespace state with a single global `admin_config` PDA. Namespace-derived account validation and every administration transfer path require platform verification before implementation claims compatibility.

Reuse the component's initialization, transfer and renounce lifecycle. Initialization assigns the namespace's current administrator and must not leave a publicly claimable initialization window. A current administrator authorizes transfer, and the successor must accept it under the library's transfer rules. Renounce is irreversible.

Fee-rate and treasury changes are separate, explicit administration operations. A fee update preserves treasury and administrator; a treasury update preserves fee rate and administrator. Neither operation transfers administration. Use the selected lifecycle for admin transfer and renunciation. See the [shared namespace, custody, and fee contract](../product/namespace-fee-contract.md) for checks and acceptance cases.

Use separate `spel-admin-authority` state for namespace administration, each sale's creator role and each direct pool-owner role. Do not use Authority NFTs for these roles. One account may hold several roles, but namespace administration grants no creator or pool-owner rights. Curve factory-created pools stay under curve factory program custody.

Creator and direct pool-owner migration is separate work. It must cover stored role state, transfer acceptance, client inputs and existing account compatibility. Decide whether these roles may renounce before enabling renunciation; it must not leave required settlement or recovery unavailable. Existing NFT checks remain until that migration is implemented and tested.

Retain the reviewed curve factory's resumable creation and settlement. Pending curve pools cannot trade before funding is complete. Each curve stage rechecks the relevant authority and commits atomically with its child calls; the whole curve workflow can span transactions. This reviewed curve behavior does not define LBP creation stages, pending guards or custody.

## Consequences

The singleton/genesis-admin, address-bound and namespace-admin NFT-holder models are superseded. Administration transfer must not bypass isolation or pending-state checks. The reviewed v0.1.4 commit `479adf86f85b664073f359cfd2fd0087489f63a0` is evidence for the selected library API, not a production dependency pin, integration proof or a compatibility claim. LBP implementation still needs separate evidence.

The [historical authority experiments](https://github.com/bleu/logos-launchpad/blob/f1198315569cfc8ada9f6c39964cb77314e2520c/packages/platform-probes/README.md) exercised admin `479adf86f85b664073f359cfd2fd0087489f63a0` with SPEL `d0bb659048c3fc6533f51c69e9f7f12382af6dd2` and component LEZ `47eba256479f6f785acbd138834340703cd03401`. Host checks covered transfer acceptance, terminal renunciation and namespace isolation. Guest checks used controlled authorization fixtures, not signed transactions or real claim application. The upstream component has no public current-admin getter; the licensed local patch adds one. Do not decode private component bytes or store a second administrator field.

On candidate LEZ `c31a6bca60537ec08f595cf786f1ae6275ab7f43`, the legacy PDA check rejected an authorized account because the old whole-account owner field was absent. A probe checked the derived address and owner-program approval in the same transaction, then used the library's authorized-signer branch. Direct calls without owner approval were rejected. This is limited compatibility evidence, not an adopted production adapter or a complete authority lifecycle. NFT-holder transfer checks also passed on that candidate, but do not justify retaining the rejected NFT role design.

The experiments are no longer maintained. Results used development receipts, not production proofs or deployed nodes. Root pins are unchanged; namespace controls and creator/pool-owner migration remain separate implementation work. [ADR 0027](0027-events-and-history.md) records the candidate client blocker.

## Sources

- [Reviewed `spel-admin-authority` implementation](https://github.com/mmlado/spel-admin-authority/tree/479adf86f85b664073f359cfd2fd0087489f63a0)
- [Embedded account support](https://github.com/mmlado/spel-admin-authority/blob/479adf86f85b664073f359cfd2fd0087489f63a0/docs/adr/0007-embedded-account-support.md)
- [Official administration documentation](https://docs.logos.co/lez/extensions/admin-authority)
- [SPEL extension pull request](https://github.com/logos-co/spel/pull/257)
