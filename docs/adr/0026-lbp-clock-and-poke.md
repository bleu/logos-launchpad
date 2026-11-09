# ADR 0026: Weighted-sale clock reads and pricing

Status: Accepted local design; temporary fake clock. Real LEZ timestamp reads and live clock freshness remain unverified.

Date: 2026-09-29; checkpoint and interpolation review: 2026-10-02; time-window amendment: 2026-10-29; inclusion-time amendment: 2026-11-04

## Context

Weighted-sale prices depend on a time-based weight schedule. The selected LEZ actor clock can check a time interval. It cannot return its time to the sale program. An exact-second check gives too little time to prepare and submit a buy, including private-route work.

The [historical clock experiment](https://github.com/bleu/logos-launchpad/blob/f1198315569cfc8ada9f6c39964cb77314e2520c/crates/launchpad-client/tests/clock_probe.rs) used an older interface. It does not establish actor clock reads. The current clock stores Unix milliseconds. It is updated at the end of a block. Checks use the last recorded block time.

## Clock-read amendment: 2026-11-05

This amendment supersedes the buyer-selected pricing windows below. Weighted buys now supply amounts and price limits only. Each buy requests one timestamp through a separate read-and-reply interface. The reply is accepted only from the configured clock actor during the pending clock-read phase. The buyer cannot select that actor or supply its time.

The temporary local provider returns Unix milliseconds from fixture state. Divide by 1000 to select the pricing second. Model previous-block time, not exact execution time. Require the pricing second to be at or after the scheduled start and before the scheduled end. Interpolate the collateral weight at that second and use its exact token-weight complement. Use current recorded reserves and retain the math limits, creator-borne fees and buyer price limits.

Attach the immutable sale start and end as inclusion-time bounds. These bounds remain necessary when saved clock time is stale. Setup keeps its existing canonical clock assertions. Fixed-price and bonding-curve behavior is unchanged. No checkpoint is needed for pricing. Observation design remains deferred.

The pinned LEZ clock cannot return a timestamp. `LAUNCHPAD_MOCK_CLOCK=1` enables the temporary provider for local tests. Normal builds reject weighted buys with `ClockUnavailable`. The fixture has a separate actor address and no public time-setting message. Replace the provisional interface when a verified LEZ timestamp-read API is available. The future capability is a project assumption, not implemented platform behavior.

This design removes the buyer's choice of pricing time. It also makes weighted buys depend on an API that is not yet available on the selected platform. Local host and guest tests use a fake clock. They do not establish real clock support, freshness, private execution or deployment readiness. See the [workspace guide](../architecture/workspace-guide.md#temporary-weighted-pricing-clock).

## Earlier window decision (superseded for weighted buy pricing)

This amendment supersedes the earlier rule that each buy uses weights from an exact trusted execution timestamp. It also supersedes the timestamp-keyed buy and poke checkpoint rules. Weighted buys use a buyer-selected time window and the least favourable scheduled weights for the buyer within that window.

Each weighted buy supplies `valid_from` and `valid_until` in Unix seconds. The window includes `valid_from` and excludes `valid_until`. Require `sale_start <= valid_from < valid_until <= sale_end`. Reject an invalid window. Do not shorten it without the buyer's instruction. Use the canonical clock to check `valid_from * 1000 <= clock_time < valid_until * 1000`. Convert bounds with checked arithmetic. Also attach the same half-open interval as a response validity window. LEZ must check it against the inclusion-block timestamp. Saved clock data alone cannot enforce the sale boundaries. Intersect added bounds with existing response bounds. A failed check fails the whole transaction.

Use immutable linear weight endpoints with a fixed total of `10^18`. Each token weight must be at least `10^16` (1%) and at most `99 * 10^16` (99%). Store normalized integer weights, not whole percentages. This gives smooth schedules without whole-percentage steps. The internal math scales remain `2^52` and `2^62`, as specified in [ADR 0011](0011-weighted-precision.md). This change does not adopt Balancer's power implementation.

Use an explicit scheduled start for weighted creation. Check that the canonical clock is at or before this start when creation executes. Also require creation inclusion at or before the scheduled start; convert its inclusive millisecond bound to an exclusive upper bound with checked addition. Do not provide a start-now option until the clock can return a trusted time. Every incomplete setup stage and pool activation must enter before the sale end. Enforce this with an inclusion-time upper bound, even when the saved clock is stale. See [ADR 0031](0031-shared-launch-factory.md) for factory setup.

Use these endpoints for interpolation. Floor the collateral weight's interpolation and calculate the token weight as its exact complement. Evaluate the pair at `valid_from` and `valid_until - 1`, the last included Unix second. Select the smaller collateral weight and its token complement. This pair gives the least token output for exact input and the greatest collateral input for exact output. Linear schedules are monotone, so no interior timestamp gives a worse pair. Apply the same rule to rising, falling and constant schedules.

Price each buy with this pair and the current recorded pricing reserves. Check minimum output or maximum input against that result. Keep creator-borne fees and pricing collateral under [ADR 0016](0016-lbp-live-fees.md) and [ADR 0030](0030-lbp-pricing-collateral.md). The buyer selects the window and price limits. Do not replace them during execution.

A buy does not require a poke or checkpoint. A stored pair must not select a later price. Future observation records must identify the window and pricing pair, not an exact execution timestamp. Poke support and observation records remain future work.

The [namespace emergency freeze](0028-token-custody-and-freeze.md) stops buys. It does not stop the schedule or extend a validity window. The original sale end still closes trading. Fixed-price and bonding-curve time rules are unchanged. For weighted-sale weights, this record supersedes the execution-time rule in [ADR 0017](0017-quotes-and-slippage.md).

## Consequences

A wider window gives more time to execute but can give the buyer a worse scheduled price. This replaces exact execution-time pricing. The window protects the sale from a favourable historical pair selected by the buyer. Slippage limits protect the buyer from an unacceptable reserve change.

Host tests cover window bounds, time conversion, both buy forms, both schedule directions, constant weights, price limits and current reserves. Seeded guest tests cover successful buys, validity windows and rollback. Clock update checks, observation records and full private-route checks remain pending. Failed buys must commit no reserve, fee or transfer changes.

## Sources

- [Selected LEZ clock interface](https://github.com/logos-blockchain/logos-execution-zone/blob/578ce58a71b79c49d48e97e51c520bf71f0252d9/lez/programs/clock/core/src/lib.rs)
- [Selected LEZ clock handler](https://github.com/logos-blockchain/logos-execution-zone/blob/578ce58a71b79c49d48e97e51c520bf71f0252d9/lez/programs/clock/src/main.rs)
- [Upstream clock proposal](https://github.com/logos-blockchain/logos-execution-zone/pull/988): proposes configurable assertion granularity; it is not the selected dependency.
- [RFP-016 fixed-price and weighted sales](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md)
