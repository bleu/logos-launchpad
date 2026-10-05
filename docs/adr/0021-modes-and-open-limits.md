# ADR 0021: Sale modes, allowlists and open limits

Status: Product rules accepted internally; per-block limit deferral: `To be reviewed by Logos team`; allowlist proof design and verification open.

Date: 2026-09-29; LBP mechanics and approved-account setup review: 2026-10-02

## Context

Keep the selected sale modes. The 2026-10-01 review settled the LBP allowlist product rules. It did not choose a proof method or establish that it works on LEZ.

## Sale modes

Curve creation requires `TradeMode::{BuyOnly, BuyAndSell}`. The mode cannot change after creation. This replaces the earlier unspecified one-way mode. `BuyOnly` permits exact-input and exact-output buys with collateral. It rejects sells. `BuyAndSell` also permits exact-input sells. There is no program default or exact-output sell. UI defaults remain open.

The account layer enforces direction; pure pricing state has no factory mode policy. See [implementation evidence](../product/curve-contract.md#acceptance-evidence). [ADR 0025](0025-sale-closing.md) owns closing rules and the pending expiry-duration review.

Keep weighted and true fixed-price LBP modes. Both support exact-input buys with a minimum output and exact-output buys with a maximum full input. Fixed-price exact input spends the full input and floors output; exact output ceils payment. Reject oversized buys in full, without partial fills or rounding-remainder refunds.

LBP schedules require `end > start`, with no product minimum duration. Resolve start-now at creation execution; scheduled starts cannot be in the past. [ADR 0026](0026-lbp-clock-and-poke.md) owns chain-time weights and permissionless `poke` checkpoints.

Only the current creator authority holder can pause/resume before the original end, including before start. Authorized repeated calls succeed without another state change or event. Authority must still be checked. Pause does not stop weights or move the end, and resume cannot reopen an ended sale. This settles the previously open LBP pause authority; it does not grant sale control to namespace administrators or select platform freeze gates.

Defer the optional limit on tokens allocated in one block. This differs from the LBP RFP and is `To be reviewed by Logos team`. It is not implemented or removed from scope.

## LBP allowlist rules

When an allowlist is enabled:

- Each approved participant gets one successful purchase per sale. Public and private routes share it. Another credential for the same approved identity does not grant another purchase.
- Failed transactions do not use the allowance. The successful purchase and allowance use must finish together.
- The participant must prove membership and control of the approved account for this purchase. Sharing membership information alone must not grant access.
- One public collateral limit applies to everyone. It limits buyer input under the agreed LBP fee and pricing rules.
- The participant list stays private. Public checks must not expose it or require a persistent private participant ID.

On-chain programs must enforce these rules on both routes. Proofs must be tied to the sale and actual purchase, with protection against reuse. Account control does not prove that each account belongs to a different person or prevent sharing full account keys.

## Approved-account setup

One approved LEZ account is the eligibility identity. Support public and private approved accounts. The approved account is separate from a fresh public trading account. Multiple approved accounts can have separate allowances, even when one person controls them.

The creator prepares a fixed list from known account IDs before buyers set up proof access. The creator's service may deliver membership data initially. After delivery, a buyer must be able to prove and buy without that service. Redelivery or duplicate credentials must not reset the allowance. An account cannot be replaced during the sale; lost access requires recovery of the original account.

Proving is local in a wallet or protected tool. The private route must hide the approved account from public observers; hiding it from the creator is not required. This does not add buyer tracking. Account keys stay outside the app, creator service and shared core.

## Open work

The proof construction, tree/hash, membership-data format, verifier and wallet integration remain open. DistributionX is a research reference, not an approved design. The [eligibility-proof investigation](../research/lbp-eligibility-proof.md) records the prototype, measurements and remaining gates. Its roughly 100,000-account target is a feasibility test, not a capacity guarantee. No product proof-time limit is set; measured local cost requires review. See the [full participation specification](../architecture/private-participation.md).

Accepted product rules do not mean the proof or either route works. Deferred requirements still need Logos-team review.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
