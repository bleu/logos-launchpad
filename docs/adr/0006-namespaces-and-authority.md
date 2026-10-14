# ADR 0006: Namespace isolation and AccountId administration

Status: Accepted; curve namespace integration tested locally on pinned LEZ

Date: 2026-09-29; configuration review: 2026-10-01; role-pattern review: 2026-10-06; namespace integration: 2026-10-09

## Context

Independent operators need separate fee settings and treasuries. Moving a namespace administrator or creator role must not change namespace or sale identity. The selected `spel-admin-authority` component provides an `AccountId` administration lifecycle. Each role needs its own platform checks.

## Partial supersession

[ADR 0032](0032-fixed-price-lbp-setup.md) partially supersedes this record's open fixed-price sale creation and funding gap. The curve stages, pending guard and factory custody remain curve-specific and do not define fixed-price sale behavior. Fixed-price sale trading and verification remain pending.

## Decision

Use permissionless namespaces with stable identities. Bind each sale to its namespace at creation. Keep namespace-scoped account derivation and isolated custody through associated token accounts. Namespace state is logical per-namespace state. This decision does not select a namespace registry, shared physical schema or cross-factory namespace account owner.

Embed one library `AdminConfig` in each namespace. Initialize it with namespace creation. Do not use a single global `admin_config` PDA. The curve's handwritten dispatcher calls the library's embedded-account API. The automatic `#[admin_authority]` and `#[require_admin]` expansion needs a `#[lez_program]` dispatcher, which the curve does not use. An explicit account adapter connects the library and root LEZ crate identities.

Reuse the component's initialization, transfer and renounce lifecycle. The authorized namespace account creates its config and becomes the first administrator in the same operation. Creation must not leave a publicly claimable initialization window. Key-to-key transfer requires both signatures in one transaction. A program-owned successor uses the library's PDA candidate checks. Renounce is irreversible and has no fee, active-pool or freeze guard. These operations preserve the separate deployment-set emergency role.

Fee-rate and treasury changes are separate, explicit administration operations. A fee update preserves treasury and administrator; a treasury update preserves fee rate and administrator. Neither operation transfers administration. Use the selected lifecycle for admin transfer and renunciation. See the [shared namespace, custody, and fee contract](../product/namespace-fee-contract.md) for checks and acceptance cases.

Use separate `spel-admin-authority` state for namespace administration, each sale's creator role and each direct pool-owner role. Do not use Authority NFTs for these roles. One account may hold several roles, but namespace administration grants no creator or pool-owner rights. Curve factory-created pools stay under curve factory program custody.

Creator and direct pool-owner migration is separate work. It must cover stored role state, transfer acceptance, client inputs and existing account compatibility. Decide whether these roles may renounce before enabling renunciation; it must not leave required settlement or recovery unavailable. Existing NFT checks remain until that migration is implemented and tested.

Retain the reviewed curve factory's resumable creation and settlement. Pending curve pools cannot trade before funding is complete. Each curve stage rechecks the relevant authority and commits atomically with its child calls; the whole curve workflow can span transactions. This reviewed curve behavior does not define fixed-price or weighted-sale creation stages, pending guards or custody.

## Consequences

The singleton/genesis-admin, address-bound and namespace-admin NFT-holder models are superseded. The curve uses the unmodified v0.1.4 package through a Git dependency pinned to `479adf86f85b664073f359cfd2fd0087489f63a0`. Tests check administrator authorization instead of reading the role holder. CLI `namespace-info` omits the administrator. Namespace config has a new format marker; old development state and the combined `UpdateConfig` instruction are removed. Fixed-price and weighted-sale work still needs separate evidence.

The [historical authority experiments](https://github.com/bleu/logos-launchpad/blob/f1198315569cfc8ada9f6c39964cb77314e2520c/packages/platform-probes/README.md) exercised admin `479adf86f85b664073f359cfd2fd0087489f63a0` with SPEL `d0bb659048c3fc6533f51c69e9f7f12382af6dd2` and component LEZ `47eba256479f6f785acbd138834340703cd03401`. Host checks covered transfer acceptance, terminal renunciation and namespace isolation. Guest checks used controlled authorization fixtures, not signed transactions or real claim application. Do not decode private component bytes or store a second administrator field.

On candidate LEZ `c31a6bca60537ec08f595cf786f1ae6275ab7f43`, the legacy PDA check rejected an authorized account because the old whole-account owner field was absent. A probe checked the derived address and owner-program approval in the same transaction, then used the library's authorized-signer branch. Direct calls without owner approval were rejected. This is limited compatibility evidence, not an adopted production adapter or a complete authority lifecycle. NFT-holder transfer checks also passed on that candidate, but do not justify retaining the rejected NFT role design.

The experiments are no longer maintained. Current [signed namespace guest tests](../../crates/launchpad-client/tests/namespace_admin.rs) cover real signatures, runtime config claims, separate settings, isolation, co-signed transfer, program-owned administration and terminal renunciation on pinned LEZ. They use development receipts, not production proofs or deployed nodes. Root LEZ pins are unchanged. Creator and pool-owner migration remain separate work. [ADR 0027](0027-events-and-history.md) records the candidate client blocker.

## Sources

- [Reviewed `spel-admin-authority` implementation](https://github.com/mmlado/spel-admin-authority/tree/479adf86f85b664073f359cfd2fd0087489f63a0)
- [Embedded account support](https://github.com/mmlado/spel-admin-authority/blob/479adf86f85b664073f359cfd2fd0087489f63a0/docs/adr/0007-embedded-account-support.md)
- [Official administration documentation](https://docs.logos.co/lez/extensions/admin-authority)
- [SPEL extension pull request](https://github.com/logos-co/spel/pull/257)
