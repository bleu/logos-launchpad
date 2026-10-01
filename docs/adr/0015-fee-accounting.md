# ADR 0015: Collateral-denominated fees and current-treasury collection

Status: Accepted internally; LBP RFP departures: To be reviewed by Logos team

Date: 2026-09-29; fee-limit review: 2026-10-01

## Context

Protocol fees must not inflate trading reserves or creator proceeds. An administrator can change the namespace treasury while fees remain uncollected. Both mechanisms need one explicit fee currency.

## Decision

Protocol fees are always denominated in the collateral token. Track accrued fees separately. For both curve and LBP sales, collection transfers all accrued fees to the current namespace treasury, is permissionless and idempotent, and is available at any time. The caller cannot choose or redirect the treasury. Terminal settlement collects any remainder atomically with the relevant reserve or proceeds transfer. Previously accrued fees follow the current treasury. If the curve pool is the current treasury, the fees are already in its collateral vault: collection records them and clears accrued fees without a transfer to itself; reserve withdrawal leaves them in that vault.

Namespace fee rates are integers from 0 through 9,999 basis points at creation and update. Reject 10,000 and larger rates when settings are created or changed. A 100% fee consumes all curve buy input, so it is not a valid namespace setting. Keep 10,000 as the arithmetic denominator. Rounding can still consume a small curve trade below this limit; mechanism input/output and custody checks remain required. Fee-rate and treasury changes are separate operations under [ADR 0006](0006-namespaces-and-authority.md).

Curve swaps use the live namespace rate and charge fees in token1, the factory's collateral token. Deduct a fee from buy input or sell output. Fees do not change the immutable curve invariant. Keep collected fees distinguishable from uncollected fees and reserves.

LBP fees are also collateral-denominated, but remain creator-borne claims on collateral proceeds and do not reduce the buyer's pricing input. Its live-rate and anytime-collection design departs from the LBP RFP baseline and is accepted internally, with external status `To be reviewed by Logos team` in [ADR 0016](0016-lbp-live-fees.md).

## Consequences

Accounting tests must prevent double collection and spending fees as liquidity. Curve fee deductions cannot substitute for the creator-borne LBP model. The [shared contract](../product/namespace-fee-contract.md) defines balances, access checks, collection behavior, and acceptance cases. The curve PoC enforces the namespace fee limit, but still replaces configuration through one combined operation; separate production updates remain planned.

## Sources

- [Curve revised fee decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0005-dual-input-fees-and-monotonic-reserve-product.md)
- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
