# ADR 0006: Namespace isolation and AccountId administration

Status: Accepted; separate curve role checks implemented; see the workspace guide for execution evidence and limits

Date: 2026-09-29; configuration review: 2026-10-01; role-pattern review: 2026-10-06; namespace integration: 2026-10-09

## Context

Independent operators need separate fee settings and treasuries. Moving a namespace administrator or creator role must not change namespace or sale identity. The selected `spel-admin-authority` component provides an `AccountId` administration lifecycle. Each role needs its own platform checks.

## Partial supersession

[ADR 0032](0032-fixed-price-lbp-setup.md) partially supersedes this record's open fixed-price sale creation and funding gap. The curve stages, pending guard and factory custody remain curve-specific and do not define fixed-price sale behavior. Fixed-price public buys and fee collection are implemented; setup execution, close and payout remain pending. See the [workspace guide](../architecture/workspace-guide.md#execution-and-evidence-limits) for current checks.

[ADR 0034](0034-lez-v04-migration.md) supersedes the platform integration described below. The actor implementation uses separate role actors from `lez-compat-core::authority` instead of the embedded upstream library. It preserves the selected role lifecycle. Host tests do not establish signed runtime administration or PDA-owner integration.

## Decision

Use permissionless namespaces with stable identities. Bind each sale to its namespace at creation. Keep namespace-scoped account derivation and isolated custody through associated token accounts. Namespace state is logical per-namespace state. This decision does not select a namespace registry, shared physical schema or cross-factory namespace account owner.

Embed one library `AdminConfig` in each namespace. Initialize it with namespace creation. Do not use a single global `admin_config` PDA. The curve's handwritten dispatcher calls the library's embedded-account API. The automatic `#[admin_authority]` and `#[require_admin]` expansion needs a `#[lez_program]` dispatcher, which the curve does not use. An explicit account adapter connects the library and root LEZ crate identities.

Reuse the component's initialization, transfer and renounce lifecycle. The authorized namespace account creates its config and becomes the first administrator in the same operation. Creation must not leave a publicly claimable initialization window. Key-to-key transfer requires both signatures in one transaction. A program-owned successor uses the library's PDA candidate checks. Renounce is irreversible and has no fee, active-pool or freeze guard. These operations preserve the separate deployment-set emergency role.

Fee-rate and treasury changes are separate, explicit administration operations. A fee update preserves treasury and administrator; a treasury update preserves fee rate and administrator. Neither operation transfers administration. Use the selected lifecycle for admin transfer and renunciation. See the [shared namespace, custody, and fee contract](../product/namespace-fee-contract.md) for checks and acceptance cases.

Use separate `spel-admin-authority` state for namespace administration, each sale's creator role and each direct pool-owner role. Do not use Authority NFTs for these roles. One account may hold several roles, but namespace administration grants no creator or pool-owner rights. Curve factory-created pools stay under curve factory program custody.

Curve creator and direct pool-owner roles start with the creation signer in the creation transaction. Curve creator roles use the sale namespace and launch salt; direct pool-owner roles use the namespace and pool identity. The role address stays fixed when its holder changes. Transfers require the current holder and the successor's consent in the same transaction, or owner-program approval for a PDA successor. The new holder receives all remaining role rights, including unclaimed creator allocation and manual settlement; the former holder loses them. Transfer does not change sale terms or factory custody.

Curve creator and direct pool-owner roles cannot renounce. Required setup, settlement and reserve recovery must retain an authorized caller. Namespace administrator renunciation keeps its separate terminal lifecycle. No existing development accounts or client formats need compatibility support. There are no users, production deployments or important stored data to migrate; breaking changes are allowed.

Retain the reviewed curve factory's resumable creation and settlement. Pending curve pools cannot trade before funding is complete. Each curve stage rechecks the relevant authority and commits atomically with its child calls; the whole curve workflow can span transactions. This reviewed curve behavior does not define fixed-price or weighted-sale creation stages, pending guards or custody.

## Consequences

The singleton/genesis-admin, address-bound and namespace-admin NFT-holder models are superseded. Before the actor migration, the curve used the unmodified v0.1.4 package through a Git dependency pinned to `479adf86f85b664073f359cfd2fd0087489f63a0`. Current [namespace host tests](../../crates/curve-core/tests/namespace_admin.rs) check role authorization and emitted messages. CLI `namespace-info` omits the administrator. Namespace config has a new format marker; old development state and the combined `UpdateConfig` instruction are removed. Fixed-price and weighted-sale work still needs separate evidence.

The [historical authority experiments](https://github.com/bleu/logos-launchpad/blob/f1198315569cfc8ada9f6c39964cb77314e2520c/packages/platform-probes/README.md) exercised admin `479adf86f85b664073f359cfd2fd0087489f63a0` with SPEL `d0bb659048c3fc6533f51c69e9f7f12382af6dd2` and component LEZ `47eba256479f6f785acbd138834340703cd03401`. Host checks covered transfer acceptance, terminal renunciation and namespace isolation. Guest checks used controlled authorization fixtures, not signed transactions or real claim application. Do not decode private component bytes or store a second administrator field.

On candidate LEZ `c31a6bca60537ec08f595cf786f1ae6275ab7f43`, the legacy PDA check rejected an authorized account because the old whole-account owner field was absent. A probe checked the derived address and owner-program approval in the same transaction, then used the library's authorized-signer branch. Direct calls without owner approval were rejected. This is limited compatibility evidence, not an adopted production adapter or a complete authority lifecycle. NFT-holder transfer checks also passed on that candidate, but do not justify retaining the rejected NFT role design.

The experiments are no longer maintained. The [historical signed namespace guest tests](https://github.com/bleu/logos-launchpad/blob/5b6927cbd3b5a12941f3956bcab8458446ed52db/crates/launchpad-client/tests/namespace_admin.rs) covered real signatures, runtime config claims, separate settings, isolation, co-signed transfer, program-owned administration and terminal renunciation before the actor migration. They used development receipts, not production proofs or deployed nodes. They do not establish compatibility on the selected actor revision. See the [workspace guide](../architecture/workspace-guide.md#execution-and-evidence-limits) for current checks and gaps. Curve creator and direct pool-owner roles follow the separate lifecycle above.

## Sources

- [Reviewed `spel-admin-authority` implementation](https://github.com/mmlado/spel-admin-authority/tree/479adf86f85b664073f359cfd2fd0087489f63a0)
- [Embedded account support](https://github.com/mmlado/spel-admin-authority/blob/479adf86f85b664073f359cfd2fd0087489f63a0/docs/adr/0007-embedded-account-support.md)
- [Official administration documentation](https://docs.logos.co/lez/extensions/admin-authority)
- [SPEL extension pull request](https://github.com/logos-co/spel/pull/257)
