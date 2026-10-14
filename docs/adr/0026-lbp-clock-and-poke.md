# ADR 0026: Weighted-sale clock and poke

Status: Accepted

Date: 2026-09-29; checkpoint and interpolation review: 2026-10-02

## Context

Weighted-sale prices depend on a time-based weight schedule. Observers need an inspectable state checkpoint without allowing checkpoint timing to change the price.

## Decision

Derive linear weighted-sale weights from trusted LEZ time at trade execution. Use endpoint pairs with one equal total weight. Floor the collateral weight's convex interpolation and calculate the token weight as its exact complement. This keeps the total fixed and preserves the qualified endpoint bounds for rising, falling or constant schedules. The [weighted-sale overview](../product/lbp-contract.md#future-work) lists clock and weight-schedule integration as future work.

The [historical clock experiment](https://github.com/bleu/logos-launchpad/blob/f1198315569cfc8ada9f6c39964cb77314e2520c/crates/launchpad-client/tests/clock_probe.rs) checked canonical clock-account reads on LEZ `a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a`. The value is Unix milliseconds from the saved clock account, not the transaction's supplied execution timestamp. Later values were observed; missing, malformed and substituted inputs were rejected. These development-mode results do not prove live-node freshness or weighted-sale integration. The experiment is no longer part of the maintained test suite.

A permissionless `poke` stores a checkpoint for observers, but pricing correctness does not require a prior poke. Every successful buy also stores the execution timestamp and pair when that timestamp differs from the last checkpoint. At the same timestamp, reuse the pair without a checkpoint write. Each buy still uses current reserves and emits its own trade event. A failed buy commits no checkpoint or other trade state. Compare trusted timestamps, not block numbers. This extends the original poke-only checkpoint decision.

The [namespace emergency freeze](0028-token-custody-and-freeze.md) stops swaps. It does not stop chain time or weight progression. After unfreezing, execution derives the current weight from the same immutable schedule. The original end closes trading even if no buy or poke occurs there.

## Consequences

Future-handler tests must prove correct execution pricing with or without an intervening poke, timestamp-keyed checkpoint writes, current-reserve pricing for same-time buys, and no checkpoint change on failure. Freeze must not shift the schedule endpoint. Event and state displays can use checkpoints without treating them as the pricing authority. These requirements do not establish a working weighted-sale clock integration.

## Sources

- [RFP-016 fixed-price and weighted sales](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md)
