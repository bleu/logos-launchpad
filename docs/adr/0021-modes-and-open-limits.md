# ADR 0021: Sale modes, allowlists and open limits

Status: Sale modes and allowlist rules accepted internally. Expiry-duration and per-block limit changes: `To be reviewed by Logos team`. Proof design and verification remain open.

Date: 2026-09-29

## Context

Keep the selected sale modes. The 2026-10-01 review settled the LBP allowlist product rules. It did not choose a proof method or establish that it works on LEZ.

## Sale modes

Keep curve one-way mode, which allows buys only, and optional expiry. A curve sale closes when all `D` tokens are sold or its expiry is reached. Raising a set amount of collateral does not close it. See [ADR 0025](0025-sale-closing.md).

The contract does not enforce a minimum duration for curve expiry. This differs from RFP-015 and is `To be reviewed by Logos team`.

Keep weighted and fixed-price LBP modes, start/end times and pause controls. Weights use chain time. Anyone can call `poke` to record a time checkpoint.

Defer the optional limit on tokens allocated in one block. This differs from the LBP RFP and is `To be reviewed by Logos team`. It is not implemented or removed from scope.

## LBP allowlist rules

When an allowlist is enabled:

- Each approved participant gets one successful purchase per sale. Public and private routes share it. Another credential for the same approved identity does not grant another purchase.
- Failed transactions do not use the allowance. The successful purchase and allowance use must finish together.
- The participant must prove membership and control of the approved account for this purchase. Sharing membership information alone must not grant access.
- One public collateral limit applies to everyone. It limits buyer input under the agreed LBP fee and pricing rules.
- The participant list stays private. Public checks must not expose it or require a persistent private participant ID.

On-chain programs must enforce these rules on both routes. Proofs must be tied to the sale and actual purchase, with protection against reuse. Account control does not prove that each account belongs to a different person or prevent sharing full account keys.

## Open work

The proof method, verifier, identity format, enrollment, duplicate-identity checks and account-change rules still need design and tests. DistributionX is a research reference, not an approved design. No proof inputs or cost claims are settled. See the [full participation specification](../architecture/private-participation.md).

Accepted product rules do not mean the proof or either route works. Deferred requirements still need Logos-team review.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
