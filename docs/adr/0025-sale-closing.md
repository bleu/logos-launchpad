# ADR 0025: Mechanism-specific sale closing

Status: Accepted internally; expiry-validation departure: To be reviewed by Logos team

Date: 2026-09-29

## Decision

Use separate closing rules because curve sales are supply-driven and LBP sales are time-driven. This supersedes the earlier shared closing descriptions in [ADR 0008](0008-curve-allocation-and-close.md) and [ADR 0021](0021-modes-and-open-limits.md). Neither mechanism closes on a collateral-raised target.

A curve sale closes when all `D` is purchased or its optional expiry is reached, whichever occurs first. A creator cannot close a factory sale early. No separate close transaction is required. The contract enforces no minimum expiry duration, an accepted internal departure from RFP-015 with external status `To be reviewed by Logos team`.

After expiry, settlement burns unsold `D` once; it cannot become creator allocation. `R` and the separate creator allocation retain their [post-close payout rules](0020-settlement-and-graduation.md). Retries must not burn or pay twice.

An LBP closes at its scheduled end, which is also the weight-interpolation endpoint. Closure permits post-close operations; settlement is separate. See [curve settlement](0020-settlement-and-graduation.md) and [LBP fees and withdrawals](0016-lbp-live-fees.md).

## Sources

- [RFP-015 bonding-curve launchpad](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-015-bonding-curve-launchpad.md)
- [RFP-016 LBP launchpad](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md)
