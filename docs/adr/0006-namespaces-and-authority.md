# ADR 0006: Namespace isolation and AccountId administration

Status: Accepted; target-platform compatibility remains unverified

Date: 2026-09-29

## Context

Independent operators need separate fee settings and treasuries. Moving a namespace administrator or creator role must not change namespace or sale identity. The confirmed `spel-admin-authority` component provides a standard `AccountId` administration lifecycle, but its compatibility with the selected platform is unverified.

## Decision

Use permissionless namespaces with stable identities. Bind each sale to its namespace at creation. Keep namespace-scoped account derivation and isolated custody through associated token accounts. Namespace state is logical per-namespace state. This decision does not select a namespace registry, shared physical schema or cross-factory namespace account owner.

Embed one `#[admin_authority]` administration state in each namespace and use `#[require_admin]` for namespace administration. Initialize that state with namespace creation. Do not replace per-namespace state with a single global `admin_config` PDA. Namespace-derived account validation and every administration transfer path require platform verification before implementation claims compatibility.

Reuse the component's initialization, transfer and renounce lifecycle. Initialization assigns the namespace's current administrator and must not leave a publicly claimable initialization window. A current administrator authorizes transfer, and the successor must accept it under the library's transfer rules. Renounce is irreversible.

Creator and pool-owner authority remain separate roles. Where a creator role uses an Authority NFT, its holder checks remain explicit. Namespace administration does not grant creator rights or change creator NFT custody. Curve factory-created pools stay under curve factory program custody.

Retain the reviewed curve factory's resumable creation and settlement. Pending curve pools cannot trade before funding is complete. Each curve stage rechecks the relevant authority and commits atomically with its child calls; the whole curve workflow can span transactions. This reviewed curve behavior does not define LBP creation stages, pending guards or custody.

## Consequences

The singleton/genesis-admin, address-bound and namespace-admin NFT-holder models are superseded. Administration transfer must not bypass isolation or pending-state checks. The reviewed v0.1.4 commit `479adf86f85b664073f359cfd2fd0087489f63a0` is evidence for the selected library API, not a production dependency pin, integration proof or a compatibility claim. LBP implementation still needs separate evidence.

## Sources

- [Reviewed `spel-admin-authority` implementation](https://github.com/mmlado/spel-admin-authority/tree/479adf86f85b664073f359cfd2fd0087489f63a0)
- [Embedded account support](https://github.com/mmlado/spel-admin-authority/blob/479adf86f85b664073f359cfd2fd0087489f63a0/docs/adr/0007-embedded-account-support.md)
- [Official administration documentation](https://docs.logos.co/lez/extensions/admin-authority)
- [SPEL extension pull request](https://github.com/logos-co/spel/pull/257)
