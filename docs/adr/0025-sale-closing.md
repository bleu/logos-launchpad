# ADR 0025: Shared sale ending rules

Status: Accepted internally; expiry-validation departure: To be reviewed by Logos team

Date: 2026-09-29; weighted controls and settlement evidence: 2026-11-06; shared-ending amendment: 2026-11-13

## Decision

[ADR 0031](0031-shared-launch-factory.md) replaces mechanism-specific closing with an immutable ending policy checked at the pool boundary. This amends the earlier closing descriptions in [ADR 0008](0008-curve-allocation-and-close.md) and [ADR 0021](0021-modes-and-open-limits.md). No sale ends on a collateral-raised target.

| Policy | Trading and settlement | Supported creation |
| --- | --- | --- |
| `Manual` | No automatic ending; owner close rights remain engine-specific | Direct curve pools only; no end timestamp |
| `Scheduled` | Ends at the configured end; depletion does not permit early settlement | Factory curve, fixed-price and weighted sales |
| `Depletion` | Ends when the selected recorded reserve is exhausted | Factory curve and fixed-price sales; no end timestamp |
| `Either` | Ends at selected depletion or the configured end, whichever occurs first | Factory curve, fixed-price and weighted sales |

Weighted sales require their schedule end for interpolation and as a settlement fallback, including with `Either`. They reject `Manual` and pure `Depletion`. Curve and fixed-price factory sales reject `Manual`. A creator cannot close a factory sale early. No final trade or separate close transaction is required for an automatic ending. The curve contract enforces no minimum expiry duration, an accepted internal departure from RFP-015 with external status `To be reviewed by Logos team`.

Selected depletion is final: later trades cannot reopen that sale. `Scheduled` ignores depletion for ending decisions. Empty inventory still prevents buys, but does not permit early proceeds or `AfterEnd` allocation payouts. Only recorded trading depletion counts; settlement burns cannot create depletion evidence. Pause and namespace freeze block trades without moving the schedule or changing settlement rights.

Public application, SDK and CLI timestamps remain Unix seconds. Programs convert stored schedule bounds to LEZ milliseconds with checked arithmetic and attach response validity windows. LEZ checks these against the inclusion-block timestamp: starts are inclusive and ends are exclusive. Buys must enter before a configured end; scheduled payouts must enter at or after it. Saved clock data can be from the previous block and cannot enforce these deadlines. Caller `now` values remain hints; any lifecycle branch they select must have the matching inclusion-time bound. Selected depletion needs no scheduled-end lower bound, including for weighted `Either` payouts and retries. See the [time research note](../research/lez-clock-and-transaction-time.md) for upstream sources and producer-timestamp limits.

Curve settlement burns unsold `D` once after scheduled ending; it cannot become creator allocation. `R` and the separate creator allocation retain their [post-close payout rules](0020-settlement-and-graduation.md). Retries must not burn or pay twice.

Only the current creator-role holder can authorize fixed-price or weighted proceeds settlement. Pay all recorded collateral to that creator, collect remaining fees to the current namespace treasury and burn exactly the unsold sale inventory. Weighted recorded collateral includes the initial seed plus net buyer proceeds. Exclude surplus deposits; pricing collateral does not grant a separate payout right. Partial withdrawals are not supported. Preserve the unsold count and record completed settlement separately. Successful retries cannot pay or burn twice.

The separate `AfterEnd` allocation follows the selected ending policy and can be claimed before or after proceeds settlement. Keep `AfterSetup` behavior unchanged. Fee collection, settlement and allocation claims remain available while frozen. Controls reject ended sales; repeated authorized pause or resume calls otherwise succeed without another change event. Resume cannot clear namespace freeze.

Signed fixed-price guest tests cover settlement transfers, burns and rollback. Local [guest tests](../../methods/tests/actor_sale.rs) also cover weighted controls, scheduled close, both payout orders, frozen payouts and retries. Forced fee-credit, burn, proceeds-credit and allocation-credit failures preserve full state and signer nonce. Repaired retries pay and burn once. These checks use the temporary pricing clock for setup trades. They do not prove live clock support, private execution or production readiness. See the [workspace evidence](../architecture/workspace-guide.md#execution-and-evidence-limits) and [fee policy](0016-lbp-live-fees.md).

## Sources

- [RFP-015 bonding-curve launchpad](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-015-bonding-curve-launchpad.md)
- [RFP-016 fixed-price and weighted sales](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md)
