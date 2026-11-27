# ADR 0006: Namespace isolation and AccountId administration

Status: Accepted; actor compatibility and execution limits remain.

## Decision

Use permissionless namespaces with stable identities. Bind each sale to its creation namespace. Keep namespace-scoped derivation and isolated ATA custody. Namespace state is logical configuration; this decision does not select a registry, shared physical schema or cross-factory account owner.

Use separate role actors from `lez-compat-core::authority` for each namespace administrator, sale creator and direct pool owner. Do not use Authority NFTs or a global administrator PDA. Namespace administration grants no creator or pool-owner rights. Curve factory pools remain under factory program custody.

Initialize each role with its creation signer in the creation transaction. Leave no public claim window. Keep authority data behind its API; do not decode private role bytes or store a second administrator field. Readonly namespace data exposes a format version, not role bytes.

Key successors must consent in the same transaction. A PDA successor requires its owner program's approval and runtime seed grant. Role identity stays fixed across transfer. Curve creator and direct pool-owner transfers move all remaining rights, including allocation and manual settlement, without changing sale terms or custody. Those roles cannot renounce because lifecycle operations such as settlement need an authorized caller. Namespace administrator renunciation is terminal and has no fee, active-pool or freeze guard. It preserves the separate emergency role.

Fee-rate and treasury updates are separate explicit operations. Each preserves the other setting and administrator. Neither transfers administration. See the [namespace contract](../product/namespace-fee-contract.md).

A creator signs one transaction to [create and fund a fresh public sale](0031-shared-launch-factory.md#shared-setup). There is no partial setup to resume. Curve settlement remains resumable across transactions. Each settlement stage rechecks authority and commits atomically with its child calls.

## Reason and limits

Separate roles prevent namespace administration from granting sale custody rights. Current support uses the [temporary actor development base](0034-lez-v04-migration.md), not an embedded upstream authority library. Source similarity and host checks do not prove signed runtime or PDA-owner integration. See [current execution evidence](../architecture/workspace-guide.md#execution-and-evidence-limits). No deployed state needs migration or backward compatibility.

## Sources

- [Reviewed `spel-admin-authority` implementation](https://github.com/mmlado/spel-admin-authority/tree/479adf86f85b664073f359cfd2fd0087489f63a0)
- [Embedded account support](https://github.com/mmlado/spel-admin-authority/blob/479adf86f85b664073f359cfd2fd0087489f63a0/docs/adr/0007-embedded-account-support.md)
- [Official administration documentation](https://docs.logos.co/lez/extensions/admin-authority)
- [SPEL extension pull request](https://github.com/logos-co/spel/pull/257)
