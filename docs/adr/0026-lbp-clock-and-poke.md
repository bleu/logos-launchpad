# ADR 0026: LBP clock and poke

Status: Accepted

Date: 2026-09-29

## Context

LBP prices depend on a time-based weight schedule. Observers need an inspectable state checkpoint without allowing checkpoint timing to change the price.

## Decision

Derive linear LBP weights from trusted LEZ time at trade execution. A permissionless `poke` stores a checkpoint for observers, but pricing correctness does not require a prior poke.

Pause stops trades. It does not stop chain time or weight progression. After unpause, execution derives the current weight from the same schedule.

## Consequences

Tests must prove that execution pricing is correct with or without an intervening poke and that a pause does not shift the schedule endpoint. Event and state displays can use checkpoints without treating them as the pricing authority.

## Sources

- [RFP-016 LBP launchpad](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md)
