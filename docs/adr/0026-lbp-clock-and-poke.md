# ADR 0026: Weighted-sale clock reads and pricing

Status: Accepted local design; temporary fake clock; real timestamp reads and freshness unverified.

## Decision

### Pricing clock

Each weighted trade requests one timestamp through a separate read-and-reply interface. Accept a reply only from the configured clock actor during the pending clock-read phase. The trader cannot choose that actor or provide its time. Messages carry amounts and price limits, not buyer-selected pricing windows.

The temporary local provider returns Unix milliseconds from fixture state. Divide by 1000 for the pricing second. Model previous-block time, not exact inclusion time. Require the second to be at or after scheduled start and before scheduled end. Interpolate collateral weight at that second, flooring the interpolation, and use the exact token-weight complement. Use current recorded pricing reserves, qualified math limits and participant price limits. [ADR 0015](0015-fee-accounting.md) defines fees; [ADR 0030](0030-lbp-pricing-collateral.md) defines pricing collateral.

Attach immutable sale start and end as inclusion-time bounds. Saved clock time cannot enforce deadlines when stale. LEZ checks validity against the inclusion-block timestamp; starts are inclusive and ends exclusive. Keep setup's canonical clock assertions separate. Fixed-price and curve rules are unchanged. See [ending rules](0025-sale-closing.md).

### Setup weights and bounds

Store immutable linear endpoints as normalized integer weights with fixed total `10^18`. Each side must be in `10^16..=99 * 10^16` (1% through 99%). Do not store whole percentages. Math scales remain `2^52` and `2^62` under [ADR 0011](0011-weighted-precision.md); this does not adopt Balancer's power implementation.

Require an explicit scheduled start and a later schedule end. At creation, assert canonical clock time at or before start and require inclusion at or before start. Convert the inclusive millisecond limit to an exclusive upper bound with checked addition. No weighted start-now option is available without trusted timestamp reads. Every incomplete setup stage and activation must enter before end, even if saved clock time is stale. Convert with checked arithmetic and intersect response windows; never erase an earlier bound.

### Observer checkpoints

Permissionless poke reads the pricing clock and records the pricing second and token weight. Successful buys update that observation when the trusted timestamp changes. Changed checkpoints emit the same event for buys and pokes; same-timestamp buys emit no checkpoint event. Use the same weight calculation for pricing and observations. Failed buys and pokes commit no checkpoint, event or state change.

No checkpoint or poke is needed for pricing or settlement. A checkpoint grants no payout rights. Namespace freeze stops trades, not the schedule or poke, and cannot extend the end.

## Reason and limits

One trusted timestamp removes trader choice of pricing time. The selected canonical LEZ clock checks intervals but cannot return time; its saved timestamp is updated at block end and can describe the previous block. That differs from inclusion-block validity checks.

`LAUNCHPAD_MOCK_CLOCK=1` enables only the temporary local provider. Its actor address is separate and has no public time-setting message. Normal builds reject weighted trades with `ClockUnavailable`. Replace the provisional interface when a verified timestamp-read API exists. That future API is an assumption, not platform behavior. Host and guest fixtures do not prove real clock support, freshness, private execution or deployment readiness. See the [temporary clock guide](../architecture/workspace-guide.md#temporary-weighted-pricing-clock).

## Sources

- [Selected LEZ clock interface](https://github.com/logos-blockchain/logos-execution-zone/blob/578ce58a71b79c49d48e97e51c520bf71f0252d9/lez/programs/clock/core/src/lib.rs)
- [Selected LEZ clock handler](https://github.com/logos-blockchain/logos-execution-zone/blob/578ce58a71b79c49d48e97e51c520bf71f0252d9/lez/programs/clock/src/main.rs)
- [Upstream clock proposal](https://github.com/logos-blockchain/logos-execution-zone/pull/988): proposes configurable assertion granularity; it is not the selected dependency.
- [RFP-016 fixed-price and weighted sales](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md)
