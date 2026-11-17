# ADR 0026: Weighted-sale clock reads and pricing

Status: Accepted; pinned upstream timestamp reads. Sequencer timing, full private routes and real proofs retain evidence limits.

Date: 2026-09-29; checkpoint and interpolation review: 2026-10-02; time-window amendment: 2026-10-29; inclusion-time amendment: 2026-11-04; poke and checkpoint amendment: 2026-11-05

## Context

Weighted-sale prices depend on a time-based weight schedule. The earlier selected LEZ actor clock could check a time interval but could not return its time to the sale program. An exact-second check gives too little time to prepare and submit a buy, including private-route work.

The [historical clock experiment](https://github.com/bleu/logos-launchpad/blob/f1198315569cfc8ada9f6c39964cb77314e2520c/crates/launchpad-client/tests/clock_probe.rs) used an older interface. It does not establish actor clock reads. The current clock stores Unix milliseconds. It is updated at the end of a block. Checks use the last recorded block time.

## Clock-read amendment: 2026-11-05 (historical provider)

This amendment supersedes the buyer-selected pricing windows below. Weighted buys now supply amounts and price limits only. Each buy requests one timestamp through a separate read-and-reply interface. The reply is accepted only from the configured clock actor during the pending clock-read phase. The buyer cannot select that actor or supply its time.

The temporary local provider returns Unix milliseconds from fixture state. Divide by 1000 to select the pricing second. Model previous-block time, not exact execution time. Require the pricing second to be at or after the scheduled start and before the scheduled end. Interpolate the collateral weight at that second and use its exact token-weight complement. Use current recorded reserves and retain the math limits and buyer price limits. Namespace fees use net-input pricing and exact-output gross-up under [ADR 0031](0031-shared-launch-factory.md).

Attach the immutable sale start and end as inclusion-time bounds. These bounds remain necessary when saved clock time is stale. Setup keeps its existing canonical clock assertions. Fixed-price and bonding-curve behavior is unchanged. No checkpoint is needed for pricing.

A permissionless poke message reads the pricing clock and records the current token weight as an observer checkpoint. Successful buys also update the checkpoint when the trusted timestamp changes from the last recorded value. Each changed checkpoint emits the same event for buys and pokes. Same-timestamp buys leave the checkpoint unchanged and emit no checkpoint event. Pricing and checkpoints use one weight calculation. Failed buys and pokes commit no checkpoint, event or state changes. The checkpoint does not affect pricing or settlement; it is an observation record only.

At this earlier pin, the LEZ clock could not return a timestamp. `LAUNCHPAD_MOCK_CLOCK=1` enabled the temporary provider for local tests. Normal builds then rejected weighted buys with `ClockUnavailable`. The fixture has a separate actor address and no public time-setting message. Replace the provisional interface when a verified LEZ timestamp-read API is available. The future capability is a project assumption, not implemented platform behavior.

This earlier design removed the buyer's choice of pricing time. It made weighted buys depend on an API that was not yet available on that platform revision. Its local host and guest tests used a fake clock. They do not establish real clock support, freshness, private execution or deployment readiness. See the [workspace guide](../architecture/workspace-guide.md#weighted-pricing-clock).

## Upstream clock amendment: 2026-11-09

This amendment replaces the temporary provider in the 5 November implementation. Use LEZ revision `cb075d5777cb812ba44216429d4f2837ea8518bd` from `dev`, selected on 16 November 2026. It is a development pin. The upstream clock PR merged into `dev` on 10 November 2026.

Send `ReadTimestamp { granularity: 1 }` to `clock_core::clock_actor()`. Verify both sender addresses. Accept only `Reply::Timestamp` during the pending read. Decode fallibly and reject a block-ID reply. Values are Unix milliseconds. Divide by 1000 only for weighted pricing. Each buy and poke reads once. Buys use the same timestamp and weight calculation for pricing and the observer checkpoint. Keep participant limits and sale inclusion bounds.

The merged clock API and block application code were checked on 16 November 2026. The clock ticks at the end of each block. Programs read the previous block's saved time. The API does not expose the current inclusion-block timestamp. Local guest ticks can check state updates. They do not prove sequencer ordering or timestamp trust. Factory setup and pool activation replace removed `AssertTimestamp` calls with authenticated replies and the same inclusive millisecond comparisons. Keep their inclusion bounds.

A prepared private proof can require an exact return message. The local native-token probe at the earlier feature-branch pin found that a changed public clock reply alone can pass when the private return stays fixed. A return that changes from 100 to 99 fails with `CrossMessageMismatch` even when the minimum is 98. A fresh proof succeeds. Slippage does not change that proof requirement. This probe uses development receipts and does not complete the launchpad private route. It has not been repeated on the current `dev` pin.

The temporary guest, mock build flag and normal-build availability error are removed. Poke and observer checkpoints use the same upstream adapter. Same-second observations emit no duplicate checkpoint event. Failed calls commit no checkpoint or event changes. Poke remains available at the schedule endpoint and after settlement; buy time bounds do not restrict observation calls.

## Earlier window decision (superseded for weighted buy pricing)

This amendment supersedes the earlier rule that each buy uses weights from an exact trusted execution timestamp. It also supersedes the timestamp-keyed buy and poke checkpoint rules. Weighted buys use a buyer-selected time window and the least favourable scheduled weights for the buyer within that window.

Each weighted buy supplies `valid_from` and `valid_until` in Unix seconds. The window includes `valid_from` and excludes `valid_until`. Require `sale_start <= valid_from < valid_until <= sale_end`. Reject an invalid window. Do not shorten it without the buyer's instruction. Use the canonical clock to check `valid_from * 1000 <= clock_time < valid_until * 1000`. Convert bounds with checked arithmetic. Also attach the same half-open interval as a response validity window. LEZ must check it against the inclusion-block timestamp. Saved clock data alone cannot enforce the sale boundaries. Intersect added bounds with existing response bounds. A failed check fails the whole transaction.

Use immutable linear weight endpoints with a fixed total of `10^18`. Each token weight must be at least `10^16` (1%) and at most `99 * 10^16` (99%). Store normalized integer weights, not whole percentages. This gives smooth schedules without whole-percentage steps. The internal math scales remain `2^52` and `2^62`, as specified in [ADR 0011](0011-weighted-precision.md). This change does not adopt Balancer's power implementation.

Use an explicit scheduled start for weighted creation. Check that the canonical clock is at or before this start when creation executes. Also require creation inclusion at or before the scheduled start; convert its inclusive millisecond bound to an exclusive upper bound with checked addition. Do not provide a start-now option until the clock can return a trusted time. Every incomplete setup stage and pool activation must enter before the sale end. Enforce this with an inclusion-time upper bound, even when the saved clock is stale. See [ADR 0031](0031-shared-launch-factory.md) for factory setup.

Use these endpoints for interpolation. Floor the collateral weight's interpolation and calculate the token weight as its exact complement. Evaluate the pair at `valid_from` and `valid_until - 1`, the last included Unix second. Select the smaller collateral weight and its token complement. This pair gives the least token output for exact input and the greatest collateral input for exact output. Linear schedules are monotone, so no interior timestamp gives a worse pair. Apply the same rule to rising, falling and constant schedules.

Price each buy with this pair and the current recorded pricing reserves. Check minimum output or maximum input against that result. Fee incidence is superseded by [ADR 0031](0031-shared-launch-factory.md); pricing collateral follows [ADR 0030](0030-lbp-pricing-collateral.md). The buyer selects the window and price limits. Do not replace them during execution.

A buy does not require a poke or checkpoint. A stored pair must not select a later price. Poke and observer checkpoints are implemented in the clock-read amendment above.

The [namespace emergency freeze](0028-token-custody-and-freeze.md) stops buys. It does not stop the schedule or extend a validity window. The original sale end still closes trading. Fixed-price and bonding-curve time rules are unchanged. For weighted-sale weights, this record supersedes the execution-time rule in [ADR 0017](0017-quotes-and-slippage.md).

## Consequences

A wider window gives more time to execute but can give the buyer a worse scheduled price. This replaces exact execution-time pricing. The window protects the sale from a favourable historical pair selected by the buyer. Slippage limits protect the buyer from an unacceptable reserve change.

Host tests cover window bounds, time conversion, both buy forms, both schedule directions, constant weights, price limits and current reserves. Seeded guest tests cover successful buys, validity windows and rollback. The earlier checks did not cover clock updates or observations. Current local clock and observation evidence is in the workspace guide. Full private-route checks remain pending. Failed buys must commit no reserve, fee or transfer changes.

## Sources

- [Selected LEZ clock interface](https://github.com/logos-blockchain/logos-execution-zone/blob/cb075d5777cb812ba44216429d4f2837ea8518bd/lez/programs/clock/core/src/lib.rs)
- [Selected LEZ clock handler](https://github.com/logos-blockchain/logos-execution-zone/blob/cb075d5777cb812ba44216429d4f2837ea8518bd/lez/programs/clock/src/main.rs)
- [Upstream clock proposal](https://github.com/logos-blockchain/logos-execution-zone/pull/988): supplies timestamp reads on the selected development revision; it merged into `dev` on 10 November 2026.
- [RFP-016 fixed-price and weighted sales](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md)
