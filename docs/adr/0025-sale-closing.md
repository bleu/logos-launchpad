# ADR 0025: Mechanism-specific sale closing

Status: Accepted internally; expiry-validation departure: To be reviewed by Logos team

Date: 2026-09-29; weighted controls and settlement evidence: 2026-11-06

## Decision

Use separate closing rules because bonding-curve sales are supply-driven and fixed-price and weighted sales are time-driven. This supersedes the earlier shared closing descriptions in [ADR 0008](0008-curve-allocation-and-close.md) and [ADR 0021](0021-modes-and-open-limits.md). Neither mechanism closes on a collateral-raised target.

A curve sale closes when all `D` is purchased or its optional expiry is reached, whichever occurs first. A creator cannot close a factory sale early. No separate close transaction is required. The contract enforces no minimum expiry duration, an accepted internal departure from RFP-015 with external status `To be reviewed by Logos team`.

Public application, SDK and CLI timestamps remain Unix seconds. Programs convert stored schedule bounds to LEZ milliseconds with checked arithmetic and attach response validity windows. LEZ checks these against the inclusion-block timestamp: starts are inclusive and ends are exclusive. Buys must enter before the stored end; expiry-based payouts must enter at or after it. Saved clock data can be from the previous block and cannot enforce these deadlines. Caller `now` values remain hints; any lifecycle branch they select must have the matching inclusion-time bound. Sellout, depletion and already-completed payouts need no expiry bound. See the [time research note](../research/lez-clock-and-transaction-time.md) for upstream sources and producer-timestamp limits.

After expiry, settlement burns unsold `D` once; it cannot become creator allocation. `R` and the separate creator allocation retain their [post-close payout rules](0020-settlement-and-graduation.md). Retries must not burn or pay twice.

Fixed-price sales close at their configured end, if present, or when all sale inventory sells, whichever occurs first. If no end is configured, sellout closes the sale. No final trade or separate close transaction is required. Only the current creator-role holder can authorize settlement, which pays all recorded creator proceeds to that creator, collects remaining fees to the current namespace treasury and burns exactly the unsold sale inventory. Partial withdrawals are not supported. Extra deposits remain surplus. Freeze blocks buys, not settlement. Successful retries do not pay or burn twice.

The separate `AfterEnd` creator allocation can be claimed at expiry or sellout, independently of proceeds settlement. Burning unsold inventory does not count as sellout: the pool preserves the unsold count and records completed proceeds settlement separately. Expiry-based payouts require an inclusion-time lower bound. Fixed-price host-handler tests check state and requested operations; actual fixed-price settlement transfer, burn and transaction rollback execution remain unverified.

Weighted sales close at their scheduled end, which is also the weight-interpolation endpoint. Depleted inventory does not permit early settlement. Only the current creator-role holder can pause or resume a funded weighted sale before that end. Repeated authorized calls succeed without another change event. Pause and namespace freeze block buys. They do not stop the schedule or move the end. Resume cannot clear namespace freeze.

Only the current creator-role holder can authorize weighted proceeds settlement. Pay that creator the full recorded collateral: the initial seed plus net buyer proceeds. Collect remaining fees to the current namespace treasury. Burn exactly the unsold sale inventory. Exclude surplus deposits and pricing-only collateral. Partial withdrawals are not supported. The separate `AfterEnd` allocation becomes available at the scheduled end. Its claim can run before or after proceeds settlement. Keep `AfterSetup` allocation behavior unchanged. Fee collection, settlement and allocation claims remain available while frozen. Successful retries cannot pay or burn twice. Weighted settlement and allocation retries retain the scheduled-end inclusion lower bound.

Local [guest tests](../../methods/tests/actor_sale.rs) cover controls, scheduled close, both payout orders, frozen payouts and retries. Forced fee-credit, burn, proceeds-credit and allocation-credit failures preserve full state and signer nonce. Repaired retries pay and burn once. These checks use the temporary pricing clock for setup trades. They do not prove live clock support, private execution or production readiness. See the [workspace evidence](../architecture/workspace-guide.md#execution-and-evidence-limits) and [fee policy](0016-lbp-live-fees.md).

## Sources

- [RFP-015 bonding-curve launchpad](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-015-bonding-curve-launchpad.md)
- [RFP-016 fixed-price and weighted sales](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md)
