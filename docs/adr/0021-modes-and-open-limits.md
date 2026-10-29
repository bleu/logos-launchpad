# ADR 0021: Sale types, trade modes, allowlists and open limits

Status: Product rules accepted internally; per-block limit deferral: `To be reviewed by Logos team`; allowlist proof design and verification open.

Date: 2026-09-29; sale mechanics review: 2026-10-02

## Context

Keep the three sale types and the bonding-curve trade modes. The 2026-10-01 review settled the fixed-price and weighted-sale allowlist product rules. It did not choose a proof method or establish that it works on LEZ.

## Sale modes

Curve creation requires `TradeMode::{BuyOnly, BuyAndSell}`. The mode cannot change after creation. This replaces the earlier unspecified one-way mode. `BuyOnly` permits exact-input and exact-output buys with collateral. It rejects sells. `BuyAndSell` also permits exact-input sells. There is no program default or exact-output sell. UI defaults remain open.

The account layer enforces direction; pure pricing state has no factory mode policy. See [implementation evidence](../product/curve-contract.md#acceptance-evidence). [ADR 0025](0025-sale-closing.md) owns closing rules and the pending expiry-duration review.

## Fixed-price and weighted sales

Fixed-price and weighted sales both support exact-input buys with a minimum output and exact-output buys with a maximum full input. Fixed-price sale exact input spends the full input and floors output; exact output ceils payment. Reject oversized buys in full, without partial fills or rounding-remainder refunds.

A fixed-price sale may omit its end time. When it has an end time, it must be after the resolved start. A weighted sale requires an end time after its resolved start. Fixed-price start-now and scheduled starts use the actor clock bounds in [ADR 0032](0032-fixed-price-lbp-setup.md). Weighted creation requires an explicit scheduled start. The canonical clock must be at or before that start. There is no weighted start-now option while the clock cannot return a trusted time. [ADR 0025](0025-sale-closing.md) defines when each sale closes. [ADR 0026](0026-lbp-clock-and-poke.md) owns weighted validity windows and schedule interpolation.

Only the current creator authority holder can pause/resume before the original end, including before start. Authorized repeated calls succeed without another state change or event. Authority must still be checked. Pause does not stop weighted-sale weight changes or move either sale's end, and resume cannot reopen an ended sale. This settles the previously open fixed-price and weighted-sale pause authority; it does not grant sale control to namespace administrators or select platform freeze gates.

Defer the optional limit on tokens allocated in one block. This differs from the RFP-016 requirement and is `To be reviewed by Logos team`. It is not implemented or removed from scope.

## Fixed-price and weighted-sale allowlist rules

When an allowlist is enabled:

- Each approved participant gets one successful purchase per sale. Public and private routes share it. Another credential for the same approved identity does not grant another purchase.
- Failed transactions do not use the allowance. The successful purchase and allowance use must finish together.
- The participant must prove membership and control of the approved account for this purchase. Sharing membership information alone must not grant access.
- One public collateral limit applies to everyone. It limits buyer input under the agreed fixed-price and weighted-sale fee and pricing rules.
- The participant list stays private. Public checks must not expose it or require a persistent private participant ID.

On-chain programs must enforce these rules on both routes. Proofs must be tied to the sale and actual purchase, with protection against reuse. Account control does not prove that each account belongs to a different person or prevent sharing full account keys.

## Open work

The proof method, verifier, identity format, enrollment, duplicate-identity checks and account-change rules still need design and tests. DistributionX is a research reference, not an approved design. No proof inputs or cost claims are settled. See the [full participation specification](../architecture/private-participation.md).

Accepted product rules do not mean the proof or either route works. Deferred requirements still need Logos-team review.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
