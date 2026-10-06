# ADR 0006: Namespace isolation and AccountId administration

Status: Accepted; curve administration and emergency signed guest tests pass; locked build and IDL checks pass

Date: 2026-09-29; configuration review: 2026-10-01; role-pattern review: 2026-10-06

## Context

Independent operators need separate fee settings and treasuries. Moving a namespace administrator or creator role must not change namespace or sale identity. The selected `spel-admin-authority` component provides the `AccountId` administration lifecycle.

## Decision

Use permissionless namespaces with stable identities. Bind each sale to its namespace at creation. Keep namespace-scoped account derivation and isolated custody through associated token accounts. Namespace state is logical per-namespace state. This decision does not select a namespace registry, shared physical schema or cross-factory namespace account owner.

Embed one administration component in each namespace. Macro-based programs can use `#[admin_authority]` and `#[require_admin]`; the curve's manual dispatcher calls the same library's public initialization, authorization, transfer and renounce APIs. Initialize that state with namespace creation. Do not replace per-namespace state with a single global `admin_config` PDA.

Reuse the component's initialization, transfer and renounce lifecycle. Initialization assigns the namespace's current administrator and must not leave a publicly claimable initialization window. A current administrator authorizes transfer, and the successor must accept it under the library's transfer rules. Renounce is irreversible.

Fee-rate and treasury changes are separate, explicit administration operations. A fee update preserves treasury and administrator; a treasury update preserves fee rate and administrator. Neither operation transfers administration. Use the selected lifecycle for admin transfer and renunciation. See the [shared namespace, custody, and fee contract](../product/namespace-fee-contract.md) for checks and acceptance cases.

Use separate `spel-admin-authority` state for namespace administration, each sale's creator role and each direct pool-owner role. Do not use Authority NFTs for these roles. One account may hold several roles, but namespace administration grants no creator or pool-owner rights. Curve factory-created pools stay under curve factory program custody.

Creator and direct pool-owner migration is separate work. It must cover stored role state, transfer acceptance, client inputs and existing account compatibility. Decide whether these roles may renounce before enabling renunciation; it must not leave required settlement or recovery unavailable. Existing NFT checks remain until that migration is implemented and tested.

The fixed [namespace emergency authority](0028-token-custody-and-freeze.md) is separate from administration; admin transfer or renunciation cannot change it.

Retain the reviewed curve factory's resumable creation and settlement. Pending curve pools cannot trade before funding is complete. Each curve stage rechecks the relevant authority and commits atomically with its child calls; the whole curve workflow can span transactions. This reviewed curve behavior does not define LBP creation stages, pending guards or custody.

## Consequences

The singleton/genesis-admin, address-bound and namespace-admin NFT-holder models are superseded. The curve uses a licensed local copy of v0.1.4 commit `479adf86f85b664073f359cfd2fd0087489f63a0`, with one public getter added through `AuthoritySlot::holder()`. [Provenance](../../third-party/spel-admin-authority/PROVENANCE) records the source and patch. A narrow adapter copies public account fields between the library's LEZ types and the unchanged root LEZ pin. No private slot decoding or second administrator field is used.

The [historical authority experiments](https://github.com/bleu/logos-launchpad/blob/f1198315569cfc8ada9f6c39964cb77314e2520c/packages/platform-probes/README.md) exercised admin `479adf86f85b664073f359cfd2fd0087489f63a0` with SPEL `d0bb659048c3fc6533f51c69e9f7f12382af6dd2` and component LEZ `47eba256479f6f785acbd138834340703cd03401`. Host checks covered transfer acceptance, terminal renunciation and namespace isolation. Guest checks used controlled authorization fixtures, not signed transactions or real claim application. The upstream component has no public current-admin getter; the licensed local patch adds one. Do not decode private component bytes or store a second administrator field.

On candidate LEZ `c31a6bca60537ec08f595cf786f1ae6275ab7f43`, the legacy PDA check rejected an authorized account because the old whole-account owner field was absent. A probe checked the derived address and owner-program approval in the same transaction, then used the library's authorized-signer branch. Direct calls without owner approval were rejected. This is limited compatibility evidence, not an adopted production adapter or a complete authority lifecycle. NFT-holder transfer checks also passed on that candidate, but do not justify retaining the rejected NFT role design.

The experiments are no longer maintained. Results used development receipts, not production proofs or deployed nodes. Root pins are unchanged; creator/pool-owner migration remains separate implementation work. [ADR 0027](0027-events-and-history.md) records the candidate client blocker.

The curve config has an explicit format marker and is now version 2, with embedded fixed emergency authority. It is for fresh deployment; old config bytes are rejected and no deployed-state migration is provided. Initialization requires the namespace NFT proof and acceptance by the initial administrator and emergency authority in one atomic operation. One `AccountId` may hold both authority roles. The proof holder can also be the initial administrator, with one input and signature for both roles. Transfer to the current administrator is an accepted no-op with one administrator input and signature. Later NFT custody changes grant no administration rights. Separate fee and treasury updates, transfer and terminal renunciation use the embedded administration component. The retired whole-config instruction is rejected. See [ADR 0028](0028-token-custody-and-freeze.md) for emergency PDA control, deployment changes and evidence limits.

The pinned generic CLI cannot encode enum arguments directly. Its transfer IDL uses a wire-equivalent `Option<[u32; 40]>`: `None` means Signer; `Some` holds eight program words followed by 32 seed bytes as words. Exact wire tests cover both candidates and omitted alias accounts; the real decoder rejects seed words above 255. Conditional successor signatures remain enforced by the SDK and program; the schema's `signer: false` cannot express that condition. The `Private` entry only preserves its existing ordinal and cannot be encoded by the generic CLI. The SDK can construct the private route, but full execution remains blocked as recorded in [ADR 0028](0028-token-custody-and-freeze.md#evidence-and-limits); the earlier IDL omitted this wrapper.

Signed `V03State` tests execute the actual curve guest and verify signatures, runtime PDA claims and complete account rollback on rejection. A test owner program claims its PDA and makes seed-authorized child calls for fee update and renunciation. These are development receipts, not production proofs or deployed-node evidence. Core, namespace, private-authority and settlement tests pass with the emergency integration; [ADR 0028](0028-token-custody-and-freeze.md#evidence-and-limits) records coverage and limits. CLI/IDL checks and the full locked build suite pass. LBP, events and private namespace-admin routing remain outside this integration.

## Sources

- [Reviewed `spel-admin-authority` implementation](https://github.com/mmlado/spel-admin-authority/tree/479adf86f85b664073f359cfd2fd0087489f63a0)
- [Embedded account support](https://github.com/mmlado/spel-admin-authority/blob/479adf86f85b664073f359cfd2fd0087489f63a0/docs/adr/0007-embedded-account-support.md)
- [Official administration documentation](https://docs.logos.co/lez/extensions/admin-authority)
- [SPEL extension pull request](https://github.com/logos-co/spel/pull/257)
