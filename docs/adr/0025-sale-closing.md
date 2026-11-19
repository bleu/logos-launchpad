# ADR 0025: Shared sale ending rules

Status: Accepted internally; curve expiry minimum implemented; uncalibrated 60-second value: To be reviewed by Logos team

## Decision

Check an immutable ending policy at the pool boundary. [ADR 0031](0031-shared-launch-factory.md) defines shared policy ownership. No sale ends on a collateral-raised target.

| Policy | Trading and settlement | Supported creation |
| --- | --- | --- |
| `Manual` | No automatic ending; owner close rights remain engine-specific | Direct curve pools only; no end timestamp |
| `Scheduled` | Ends at the configured end; depletion does not permit early settlement | Factory curve, fixed-price and weighted sales |
| `Depletion` | Ends when the selected recorded reserve is exhausted | Factory curve and fixed-price sales; no end timestamp |
| `Either` | Ends at selected depletion or the configured end, whichever occurs first | Factory curve, fixed-price and weighted sales |

Weighted sales require their schedule end for interpolation and as a settlement fallback, including with `Either`. They reject `Manual` and pure `Depletion`. Curve and fixed-price factory sales reject `Manual`. A creator cannot close a factory sale early. No final trade or separate close transaction is required for an automatic ending. Factory curve sales with a configured expiry require at least 60 seconds remaining at creation and any activation that can open trading. Setup callbacks carry an inclusion-time upper bound of `end - 60 seconds`, inclusive. Caller time hints cannot bypass this bound. After expiry, an existing setup can instead finish under an inclusion-time lower bound of `end`. Activation then closes the pool and preserves post-close withdrawal. It cannot admit a short live sale. The recovered sale then settles like a sale with no buyers through its saved payout route, including a configured graduation hook. New creation after expiry is rejected. Direct internal pools are outside this factory sale policy.

Selected depletion is final: later trades cannot reopen that sale. `Scheduled` ignores depletion for ending decisions. Empty inventory still prevents buys, but does not permit early proceeds or `AfterEnd` allocation payouts. Only recorded trading depletion counts; settlement payouts cannot create depletion evidence. Pause and namespace freeze block trades without moving the schedule or changing settlement rights.

Public application, SDK and CLI timestamps remain Unix seconds. Programs convert stored schedule bounds to LEZ milliseconds with checked arithmetic and attach response validity windows. LEZ checks these against the inclusion-block timestamp: starts are inclusive and ends are exclusive. Buys must enter before a configured end; scheduled payouts must enter at or after it. Saved clock data can be from the previous block and cannot enforce these deadlines. Caller `now` values remain hints; any lifecycle branch they select must have the matching inclusion-time bound. Selected depletion needs no scheduled-end lower bound, including for weighted `Either` payouts and retries. See the [time research note](../research/lez-clock-and-transaction-time.md) for upstream sources and producer-timestamp limits.

All three engines settle unsold inventory, recorded collateral and the separate graduation reserve `R` through the saved [payout route](0020-settlement-and-graduation.md). Manual settlement requires current creator authority and no configured adapter. Preserve the unsold count and record completion separately. Partial withdrawals and duplicate payouts are unsupported. Pricing collateral grants no separate payout right.

The separate `AfterEnd` allocation follows the selected ending policy and can be claimed before or after proceeds settlement. Keep `AfterSetup` behavior unchanged. Fee collection, settlement and allocation claims remain available while frozen. Controls reject ended sales; repeated authorized pause or resume calls otherwise succeed without another change event. Resume cannot clear namespace freeze.

## Reason and limits

The minimum expiry duration prevents creators from configuring or activating an arbitrarily short factory sale. Sixty seconds is an operational floor, not a measured privacy guarantee. Complete private-route proof and inclusion latency must be measured before calibrating the production value. Selected depletion may still close an `Either` sale early. Pause does not extend expiry, so a creator with pause rights can reduce the trading time below the minimum. The minimum is a trading-time guarantee only for sales with `PausePolicy::Disabled`; participants must treat pause rights as creator trust. This restores the minimum-duration mechanism promised in RFP-015 without claiming equal private and public execution latency. The 60-second value departs from the RFP intent until measured latency supports it, so it keeps external status `To be reviewed by Logos team`.

Trading and payout eligibility must follow the same saved policy. Local guest checks cover transfers, burns, rollback and retries, not live clock support, private execution or production readiness. See [workspace evidence](../architecture/workspace-guide.md#execution-and-evidence-limits) and [fee policy](0015-fee-accounting.md).

## Sources

- [RFP-015 bonding-curve launchpad](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-015-bonding-curve-launchpad.md)
- [RFP-016 fixed-price and weighted sales](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md)
