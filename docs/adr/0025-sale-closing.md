# ADR 0025: Shared sale ending rules

Status: Accepted internally; deferral of the minimum curve duration needs Logos review.

## Decision

Check an immutable ending policy at the pool boundary. [ADR 0031](0031-shared-launch-factory.md) defines shared policy ownership. No sale ends on a collateral-raised target.

| Policy | Trading and settlement | Supported creation |
| --- | --- | --- |
| `Manual` | No automatic ending; owner close rights remain engine-specific | Direct curve pools only; no end timestamp |
| `Scheduled` | Ends at the configured end; depletion does not permit early settlement | Factory curve, fixed-price and weighted sales |
| `Depletion` | Ends when the selected recorded reserve is exhausted | Factory curve and fixed-price sales; no end timestamp |
| `Either` | Ends at selected depletion or the configured end, whichever occurs first | Factory curve, fixed-price and weighted sales |

Weighted sales require their schedule end for interpolation and as a settlement fallback, including with `Either`. They reject `Manual` and pure `Depletion`. Curve and fixed-price factory sales reject `Manual`. A creator cannot close a factory sale early. No final trade or separate close transaction is required for an automatic ending. A factory curve sale with a configured expiry must be created and funded in one transaction before that expiry. The transaction validity window excludes the expiry timestamp.

Selected depletion is final: later trades cannot reopen that sale. `Scheduled` ignores depletion for ending decisions. Empty inventory still prevents buys, but does not permit early proceeds or `AfterEnd` allocation payouts. Only recorded trading depletion counts; settlement payouts cannot create depletion evidence. Pause and namespace freeze block trades without moving the schedule or changing settlement rights.

Public application, SDK and CLI timestamps remain Unix seconds. Programs convert stored schedule bounds to LEZ milliseconds with checked arithmetic and attach response validity windows. LEZ checks these against the inclusion-block timestamp: starts are inclusive and ends are exclusive. Buys must enter before a configured end; scheduled payouts must enter at or after it. Saved clock data can be from the previous block and cannot enforce these deadlines. Caller `now` values remain hints; any lifecycle branch they select must have the matching inclusion-time bound. Selected depletion needs no scheduled-end lower bound, including for weighted `Either` payouts and retries. See the [time research note](../research/lez-clock-and-transaction-time.md) for upstream sources and producer-timestamp limits.

All three engines settle unsold inventory, recorded collateral and the separate graduation reserve `R` through the saved [payout route](0020-settlement-and-graduation.md). Manual settlement requires current creator authority and no configured adapter. Preserve the unsold count and record completion separately. Partial withdrawals and duplicate payouts are unsupported. Pricing collateral grants no separate payout right.

The separate `AfterEnd` allocation follows the selected ending policy and can be claimed before or after proceeds settlement. Keep `AfterSetup` behavior unchanged. Fee collection, settlement and allocation claims remain available while frozen. Controls reject ended sales; repeated authorized pause or resume calls otherwise succeed without another change event. Resume cannot clear namespace freeze.

## Reason and limits

The submitted curve proposal lists an enforced minimum duration under Soft-3. RFP-015 calls for enough time for the private route. Defer the minimum until the private route works and we can measure its proof and inclusion latency. The previous five-minute value had no measured basis. Short public sales are possible in the interim. Creator pause and depletion can also shorten trading.

Trading and payout eligibility must follow the same saved policy. Local guest checks cover transfers, burns, rollback and retries, not live clock support, private execution or production readiness. See [workspace evidence](../architecture/workspace-guide.md#execution-and-evidence-limits) and [fee policy](0015-fee-accounting.md).

## Sources

- [RFP-015 bonding-curve launchpad](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-015-bonding-curve-launchpad.md)
- [RFP-016 fixed-price and weighted sales](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md)
