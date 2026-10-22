# ADR 0025: Mechanism-specific sale closing

Status: Accepted internally; expiry-validation departure: To be reviewed by Logos team

Date: 2026-09-29

## Decision

Use separate closing rules because bonding-curve sales are supply-driven and fixed-price and weighted sales are time-driven. This supersedes the earlier shared closing descriptions in [ADR 0008](0008-curve-allocation-and-close.md) and [ADR 0021](0021-modes-and-open-limits.md). Neither mechanism closes on a collateral-raised target.

A curve sale closes when all `D` is purchased or its optional expiry is reached, whichever occurs first. A creator cannot close a factory sale early. No separate close transaction is required. The contract enforces no minimum expiry duration, an accepted internal departure from RFP-015 with external status `To be reviewed by Logos team`.

The [actor migration](0034-lez-v04-migration.md) keeps public application, SDK and CLI timestamps in Unix seconds. The selected LEZ native clock uses Unix milliseconds. Programs convert time bounds at the clock-call boundary with checked arithmetic. Native clock bounds are inclusive: strictly-before-end uses an upper bound one millisecond before the converted end; after expiry uses the converted end as its lower bound. Deadline execution checks these trusted intervals, not exact equality with a client timestamp.

After expiry, settlement burns unsold `D` once; it cannot become creator allocation. `R` and the separate creator allocation retain their [post-close payout rules](0020-settlement-and-graduation.md). Retries must not burn or pay twice.

Fixed-price and weighted sales close at their scheduled end. For a weighted sale, this is also the weight-interpolation endpoint. Closure permits post-close operations; settlement is separate. Fixed-price and weighted-sale close and settlement handlers remain future work. See [bonding-curve settlement](0020-settlement-and-graduation.md) and [fixed-price and weighted-sale fees and withdrawals](0016-lbp-live-fees.md).

## Sources

- [RFP-015 bonding-curve launchpad](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-015-bonding-curve-launchpad.md)
- [RFP-016 fixed-price and weighted sales](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md)
