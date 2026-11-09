# ADR 0025: Mechanism-specific sale closing

Status: Accepted internally; expiry-validation departure: To be reviewed by Logos team

Date: 2026-09-29

## Decision

Use separate closing rules because bonding-curve sales are supply-driven and fixed-price and weighted sales are time-driven. This supersedes the earlier shared closing descriptions in [ADR 0008](0008-curve-allocation-and-close.md) and [ADR 0021](0021-modes-and-open-limits.md). Neither mechanism closes on a collateral-raised target.

A curve sale closes when all `D` is purchased or its optional expiry is reached, whichever occurs first. A creator cannot close a factory sale early. No separate close transaction is required. The contract enforces no minimum expiry duration, an accepted internal departure from RFP-015 with external status `To be reviewed by Logos team`.

Public application, SDK and CLI timestamps remain Unix seconds. Programs convert stored schedule bounds to LEZ milliseconds with checked arithmetic and attach response validity windows. LEZ checks these against the inclusion-block timestamp: starts are inclusive and ends are exclusive. Buys must enter before the stored end; expiry-based payouts must enter at or after it. Saved clock data can be from the previous block and cannot enforce these deadlines. Caller `now` values remain hints; any lifecycle branch they select must have the matching inclusion-time bound. Sellout, depletion and already-completed payouts need no expiry bound. See the [time research note](../research/lez-clock-and-transaction-time.md) for upstream sources and producer-timestamp limits.

After expiry, settlement burns unsold `D` once; it cannot become creator allocation. `R` and the separate creator allocation retain their [post-close payout rules](0020-settlement-and-graduation.md). Retries must not burn or pay twice.

Fixed-price sales close at their configured end, if present, or when all sale inventory sells, whichever occurs first. If no end is configured, sellout closes the sale. No final trade or separate close transaction is required. Only the current creator-role holder can authorize settlement, which pays all recorded creator proceeds to that creator, collects remaining fees to the current namespace treasury and burns exactly the unsold sale inventory. Partial withdrawals are not supported. Extra deposits remain surplus. Freeze blocks buys, not settlement. Successful retries do not pay or burn twice.

The separate `AfterEnd` creator allocation can be claimed at expiry or sellout, independently of proceeds settlement. Burning unsold inventory does not count as sellout: the pool preserves the unsold count and records completed proceeds settlement separately. Expiry-based payouts require an inclusion-time lower bound. Fixed-price host-handler tests check state and requested operations; actual fixed-price settlement transfer, burn and transaction rollback execution remain unverified.

Weighted sales close at their scheduled end, which is also the weight-interpolation endpoint. Sellout does not permit early settlement. Only the current creator-role holder can authorize settlement. Settlement pays all recorded creator proceeds to that creator, collects remaining fees to the current namespace treasury and burns unsold sale inventory. Surplus remains in custody. Pause and namespace freeze do not block settlement. The separate `AfterEnd` allocation claim can run before or after proceeds settlement. Both require inclusion at or after the scheduled end. Successful retries cannot pay or burn twice. See [fee rules](0016-lbp-live-fees.md) and [pricing collateral](0030-lbp-pricing-collateral.md).

Weighted settlement and allocation claims are implemented. Local [guest tests](../../methods/tests/actor_sale.rs) cover authority checks, end bounds with stale saved time, payouts while frozen, both claim orders and retries. Settlement-specific failed-transfer rollback remains unverified. These checks use development execution. Live services, full private routes, real proofs and deployment readiness remain unverified. See the [workspace evidence](../architecture/workspace-guide.md#execution-and-evidence-limits).

## Sources

- [RFP-015 bonding-curve launchpad](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-015-bonding-curve-launchpad.md)
- [RFP-016 fixed-price and weighted sales](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md)
