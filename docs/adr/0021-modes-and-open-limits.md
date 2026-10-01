# ADR 0021: Keep adopted modes and mark unresolved limits

Status: Modes and allowlist product policy accepted internally; To be reviewed by Logos team for the expiry-validation and allocation-ceiling departures; proof design and verification open

Date: 2026-09-29

## Context

The proposals include optional sale modes. This design retains selected modes and the reviewed LBP allowlist product rules. It does not select the private membership proof or complete all LBP limits.

## Amendment

The 2026-10-01 review establishes purchase eligibility and allowance policy. The earlier pause on those product choices is resolved. Proof construction, enrollment tooling, verifier integration and current-platform compatibility remain open. DistributionX remains a reference to investigate, not an approved design.

## Decision

Retain curve one-way mode and the optional expiry mode. Curve closing follows [ADR 0025](0025-sale-closing.md): `D` exhaustion closes immediately, while an optional configured expiry closes at `D` exhaustion or its end timestamp. The contract does not validate a minimum sale duration when expiry is configured. This is an accepted internal departure from RFP-015 with external status `To be reviewed by Logos team`. No collateral-raised target closes a curve sale.

Retain weighted and fixed-price LBP modes, start/end times, clock-derived weights, permissionless poke and pause controls.

Defer the optional LBP per-block token-allocation ceiling. This is an accepted internal departure from the LBP RFP with external status `To be reviewed by Logos team`. Do not silently drop it from scope or claim it is implemented. LBP allowlist support remains required.

When an LBP allowlist is enabled, each approved participant has one successful purchase per sale. Public and private buys share that allowance; switching routes or obtaining another credential for the same approved identity must not grant a second purchase. A rejected transaction does not consume the purchase allowance. Consumption must commit with the successful purchase.

Purchase permission cannot be transferred by sharing membership information. The participant must also prove control of the approved account, and the permission must authorize the actual purchase. This is account authorization, not proof of human uniqueness or protection against voluntary sharing of full account keys.

One common public collateral limit applies to all allowed participants. It bounds buyer collateral input under the accepted LBP fee and pricing rules. The participant list stays private. Public verification must not expose the roster or require a persistent private participant identifier. These rules apply to both routes and must be enforced on-chain.

The selected proof approach must support private membership, approved-account control, the common cap and sale-scoped replay protection. No final proof inputs, tree/hash, verifier, identity format or cost claim is established here. Enrollment, identity deduplication and account-change behavior still need specification and evidence. See the [private participation contract](../architecture/private-participation.md).

## Consequences

Mode-specific behavior and the allowlist policy need executable tests before delivery. Accepted product rules do not establish proof feasibility or a working public/private integration. Deferral is not external approval to remove a requirement. The proposal's allowlist sketch remains an input for proof review, not an accepted cryptographic design.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
