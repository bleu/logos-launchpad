# ADR 0025: Mechanism-specific sale closing

Status: Accepted internally; To be reviewed by Logos team

Date: 2026-09-29

## Context

Curve and LBP sales have different canonical closing conditions. An earlier shared closing rule and collateral-target clarifications do not describe the selected design. Closure and settlement are separate lifecycle steps.

## Decision

This record supersedes the prior shared closing description in [ADR 0008](0008-curve-allocation-and-close.md) and [ADR 0021](0021-modes-and-open-limits.md).

A curve sale closes when all `D` sale tokens have been purchased. This is a supply-driven close condition. A curve sale has no collateral-raised target. Optional expiry closes at `D` exhaustion or its configured end timestamp, whichever comes first. The contract does not validate a minimum sale duration when expiry is configured. This is an accepted internal departure from RFP-015 with external status `To be reviewed by Logos team`. `D` exhaustion still closes the sale immediately. A creator cannot close a factory sale early. No separate close transaction is required at sell-out or expiry.

After expiry, settlement burns any unsold `D`. It does not add that inventory to the creator's allocation. The reserved `R` allocation and the separate creator allocation retain their existing post-close rules. Settlement retries must not burn or pay an allocation twice.

An LBP closes at its end timestamp. That timestamp is the schedule endpoint used for weight interpolation. No collateral-raised target closes an LBP.

Closure permits the mechanism's post-close operations. Settlement separately collects remaining fees and performs permitted allocation or proceeds transfers. This decision does not add automatic DEX graduation.

## Consequences

Mechanism specifications and tests must keep curve supply exhaustion, optional curve expiry and LBP schedule closure distinct. LBP live-rate and anytime-collection RFP departures have external status `To be reviewed by Logos team` under [ADR 0016](0016-lbp-live-fees.md).

## Sources

- [RFP-015 bonding-curve launchpad](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-015-bonding-curve-launchpad.md)
- [RFP-016 LBP launchpad](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md)
