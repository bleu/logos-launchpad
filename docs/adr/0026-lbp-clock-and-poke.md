# ADR 0026: Weighted-sale clock reads and pricing

Status: Accepted; upstream timestamp reads on the selected LEZ development revision.

## Decision

### Pricing clock

Each weighted buy, sell and permissionless poke requests one timestamp with `ReadTimestamp { granularity: 1 }` from `clock_core::clock_actor()`. Accept only `Reply::Timestamp` during the pending read, after checking both sender addresses. Decode fallibly and reject block-ID replies. The trader cannot choose the clock actor or supply pricing time. Trade messages carry amounts and price limits, not buyer-selected pricing windows.

The reply contains Unix milliseconds. Divide by 1000 for the pricing second. The clock ticks at block end, so programs read saved previous-block time. For trades, require the pricing second to be at or after scheduled start and before scheduled end. Interpolate collateral weight at that second, flooring the interpolation, and use the exact token-weight complement. Use current recorded pricing reserves, qualified math limits and participant price limits. [ADR 0015](0015-fee-accounting.md) defines fees; [ADR 0030](0030-lbp-pricing-collateral.md) defines pricing collateral.

Attach immutable sale start and end as inclusion-time bounds. Saved clock time cannot enforce deadlines when stale. LEZ checks validity against the inclusion-block timestamp; starts are inclusive and ends exclusive. Fixed-price and curve rules are unchanged. See [ending rules](0025-sale-closing.md).

### Setup weights and bounds

Store immutable linear endpoints as normalized integer weights with fixed total `10^18`. Each side must be in `10^16..=99 * 10^16` (1% through 99%). Do not store whole percentages. Math scales remain `2^52` and `2^62` under [ADR 0011](0011-weighted-precision.md); this does not adopt Balancer's power implementation.

Require an explicit scheduled start and a later schedule end. At creation, read canonical clock time and require it to be at or before start. Also require inclusion at or before start. Convert the inclusive millisecond limit to an exclusive upper bound with checked addition. There is no weighted start-now option. Every incomplete setup stage and activation must enter before end, even if saved clock time is stale. Convert with checked arithmetic and intersect response windows; never erase an earlier bound.

Factory setup and pool activation use authenticated timestamp replies with inclusive saved-time comparisons. The clock adapter defers setup effects until those comparisons pass. Keep inclusion-time bounds on the resumed response.

### Observer checkpoints

Permissionless poke reads the pricing clock and records the pricing second and token weight. Successful trades update that observation when the trusted timestamp changes. Changed checkpoints emit the same event for trades and pokes; same-timestamp observations emit no checkpoint event. Use the same weight calculation for pricing and observations. Failed trades and pokes commit no checkpoint, event or state change.

No checkpoint or poke is needed for pricing or settlement. A checkpoint grants no payout rights. Namespace freeze stops trades, not the schedule or poke, and cannot extend the end. Poke remains available at the schedule endpoint and after settlement.

## Reason and limits

One trusted timestamp removes trader choice of pricing time. Saved previous-block time differs from inclusion-block time and does not prove freshness or sequencer ordering. Normal builds use the upstream clock; there is no mock guest, mock build flag or clock-availability error. Exact pins, commands and execution evidence belong in the [workspace guide](../architecture/workspace-guide.md#weighted-pricing-clock).

A prepared private proof can require an exact return message. A price limit does not permit a different private return: changed outputs can require a fresh proof. The earlier native-token probe and its limits are recorded in the workspace guide. It does not complete the launchpad private route. Live sequencer timing, full private routes and real proofs remain unverified.

## Sources

- [Selected LEZ clock interface](https://github.com/logos-blockchain/logos-execution-zone/blob/cb075d5777cb812ba44216429d4f2837ea8518bd/lez/programs/clock/core/src/lib.rs)
- [Selected LEZ clock handler](https://github.com/logos-blockchain/logos-execution-zone/blob/cb075d5777cb812ba44216429d4f2837ea8518bd/lez/programs/clock/src/main.rs)
- [Upstream clock change](https://github.com/logos-blockchain/logos-execution-zone/pull/988): merged into `dev` on 10 November 2026.
- [RFP-016 fixed-price and weighted sales](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md)
