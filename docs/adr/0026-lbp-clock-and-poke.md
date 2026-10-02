# ADR 0026: LBP clock and poke

Status: Accepted

Date: 2026-09-29; checkpoint and interpolation review: 2026-10-02

## Context

LBP prices depend on a time-based weight schedule. Observers need an inspectable state checkpoint without allowing checkpoint timing to change the price.

## Decision

Derive linear LBP weights from trusted LEZ time at trade execution. Use endpoint pairs with one equal total weight. Floor the collateral weight's convex interpolation and calculate the token weight as its exact complement. This keeps the total fixed and preserves the qualified endpoint bounds for rising, falling or constant schedules. The [LBP contract](../product/lbp-contract.md#clock-and-linear-weights) defines the checked integer equation and timestamp constraints; target clock access and units still need platform verification.

A permissionless `poke` stores a checkpoint for observers, but pricing correctness does not require a prior poke. Every successful buy also stores the execution timestamp and pair when that timestamp differs from the last checkpoint. At the same timestamp, reuse the pair without a checkpoint write. Each buy still uses current reserves and emits its own trade event. A failed buy commits no checkpoint or other trade state. Compare trusted timestamps, not block numbers. This extends the original poke-only checkpoint decision.

Pause stops trades. It does not stop chain time or weight progression. After unpause, execution derives the current weight from the same immutable schedule. The original end closes trading even if no buy or poke occurs there.

## Consequences

Future-handler tests must prove correct execution pricing with or without an intervening poke, timestamp-keyed checkpoint writes, current-reserve pricing for same-time buys, and no checkpoint change on failure. Pause must not shift the schedule endpoint. Event and state displays can use checkpoints without treating them as the pricing authority. These requirements do not establish a working LBP clock integration.

## Sources

- [RFP-016 LBP launchpad](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md)
