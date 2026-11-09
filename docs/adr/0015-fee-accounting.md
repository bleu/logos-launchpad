# ADR 0015: Collateral fees and current-treasury collection

Status: Accepted internally; fixed-price and weighted-sale RFP departures: To be reviewed by Logos team

Date: 2026-09-29; fee-limit review: 2026-10-01

## Decision

Protocol fees use collateral and are tracked separately from reserves and creator proceeds. Collection transfers all accrued fees to the current namespace treasury, including fees accrued before a treasury change. It is permissionless, idempotent and available at any time. The caller cannot redirect payment. Terminal settlement collects any remainder atomically with the relevant reserve or proceeds transfer.

If the curve pool is the treasury, collection records and clears fees without a self-transfer. Reserve withdrawal leaves those collected fees in the vault. This prevents collection from treating fees as liquidity.

Namespace fee rates must be integers from 0 through 9,999 basis points at creation and update. Reject 10,000 or more: a 100% fee consumes all curve buy input. Rounding can still consume a small trade below this limit, so trade guards remain necessary.

Curve swaps use the live namespace rate. Deduct fees from buy input or sell output in token1, the factory's collateral token. Fixed-price buys use fee-first pricing under [ADR 0035](0035-fixed-price-swap-fees.md). Weighted-sale fees remain creator-borne under [ADR 0016](0016-lbp-live-fees.md). Both fixed-price and weighted-sale RFP departures remain `To be reviewed by Logos team`.

Fee-rate and treasury updates must be separate operations under [ADR 0006](0006-namespaces-and-authority.md). The curve PoC enforces the rate limit but still uses one combined configuration update. Fixed-price public buys and fee collection follow ADR 0035; weighted-sale public buys and fee collection are implemented under ADR 0016. See the [shared contract](../product/namespace-fee-contract.md) for custody checks.

## Sources

- [Curve revised fee decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0005-dual-input-fees-and-monotonic-reserve-product.md)
- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
