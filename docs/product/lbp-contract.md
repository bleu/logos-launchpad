# LBP mechanics contract

This is the M0 design contract for the independent LBP factory and pool. Use the terms in [CONTEXT.md](../../CONTEXT.md). The weighted-math kernel exists and is unaudited. The LBP factory, custody, fixed-price handlers and lifecycle remain future work. The acceptance cases below are requirements for those handlers, not passing program tests.

## Inputs and creation

The creator selects one immutable mode: weighted or fixed price. Both modes bind the sale to its creation namespace, launch-token definition, collateral-token definition, original start and end, and creator authority role. Token definitions must differ. The current creator authority holder controls permitted sale actions; namespace administration does not grant that role. Authority-NFT and custody compatibility still need platform evidence under [ADR 0028](../adr/0028-token-custody-and-freeze.md).

Use nonnegative integer timestamps that fit `u64`, all in the same verified clock unit. The target clock's encoding, unit, access checks and progression remain unverified. Convert without loss; do not read client time or silently change units. Resolve `start-now` to trusted creation-execution time. A scheduled start must be at or after that time. Require `end > start`. There is no contract minimum duration. Keep both times immutable.

Creation validates inputs, token accounts, funding and authority before the sale can trade. A weighted sale needs positive initial launch-token and collateral reserves. A fixed-price sale needs positive launch-token inventory and a positive price; it does not need collateral liquidity to calculate its price. Creation and funding instruction layout still need platform verification. Do not assume the curve's staged creation applies to an LBP.

The allowlist product rules in [ADR 0021](../adr/0021-modes-and-open-limits.md) remain in force. When enabled, both buy forms and both participation routes share one successful purchase allowance and one common collateral-input limit. Membership proof design and implementation remain open. This contract does not select a proof scheme or make the private route work.

## Clock and linear weights

Trading is allowed only in `[start, end)`, while unpaused and funded. Reaching `end` closes trading without a final trade, poke or creator action. Inventory exhaustion does not permit early creator withdrawal. There is no collateral fundraising target. See [ADR 0025](../adr/0025-sale-closing.md).

Weighted inputs contain collateral/token weight pairs `(c0, t0)` and `(c1, t1)` for the start and end. Every raw weight must be in `[1, 2^64]`. Each pair must satisfy `c <= 99*t` and `t <= 99*c`. Require one fixed total `N = c0 + t0 = c1 + t1`. Do not normalize raw weights to a percentage with loss of precision. Equal totals give both weights one common scale and prevent independent rounding from changing their sum.

For a trusted timestamp `now`, let `d = end - start` and `e = clamp(now - start, 0, d)`, with the clamp performed before an unsigned subtraction. Calculate:

```text
c(now) = floor((c0 * (d - e) + c1 * e) / d)
t(now) = N - c(now)
```

Use checked `u128` arithmetic. The numerator is at most `2^64 * (2^64 - 1)`, because timestamps fit `u64` and `c0,c1 <= 2^64`. The sum `N` is at most `2^65`. The result is between the integer endpoint collateral weights, so its exact complement stays between the token endpoints. Thus endpoint bounds also hold at every timestamp. Revalidate the computed pair before invoking the kernel. Rising, falling and constant weights use the same equation. Constant weights still give reserve-dependent pricing; they do not select fixed-price mode.

For example, with `start=100`, `end=200`, start weights `(80,20)` and end weights `(20,80)`, the pairs at times `100`, `125`, `150`, `175` and `200` are `(80,20)`, `(65,35)`, `(50,50)`, `(35,65)` and `(20,80)`. At `200` the final pair is observable, but buying is closed. With endpoints `(3,7)` and `(7,3)`, duration `3` and elapsed time `1`, the pair is `(4,6)`. With the endpoints reversed it is `(5,5)`; flooring the same convex formula also defines falling-weight rounding.

Weighted creation stores its trusted timestamp and start pair as the initial checkpoint. A permissionless weighted-mode `poke` records the weights for trusted time only when its timestamp differs from the checkpoint. At the same timestamp it succeeds without another checkpoint write or event. Before the start it records the start pair; at or after the end it records the end pair. It cannot change sale terms, reserves, pause state or settlement records. Pricing does not require a prior poke.

A successful buy calculates the execution-time pair. It saves that timestamp and pair only when the timestamp differs from the last checkpoint. At the same timestamp it can reuse the pair and leaves the checkpoint unchanged. Every buy still uses the latest reserves and emits its own trade event. Compare timestamps, not block numbers: two transactions in one block can have the same timestamp, but block identity is not the clock rule. A failed buy changes no reserves, fee records, checkpoint, allowance or event history.

## Pause and resume

Only the current creator authority holder can pause or resume before `end`. These actions may occur before `start`. Check authority even when the requested state is already set. An authorized repeated call succeeds without a state change or another pause/resume event. There is no delegated pauser or implicit namespace-administrator sale control.

Pause stops buys. It does not stop weights, move the start or end, or block permissionless fee collection. A resume uses the current scheduled weights. At or after `end`, pause/resume returns `InvalidSaleState`; resume cannot reopen the sale. Platform freeze integration remains separate and unverified. Required settlement and recovery paths keep the rules in [ADR 0028](../adr/0028-token-custody-and-freeze.md).

## Buy forms and price limits

Both modes expose these participant-visible forms, consistent with the curve:

| Form | Supplied amount | Supplied limit | Successful result |
| --- | --- | --- | --- |
| Exact input | Full positive collateral input `a` | Minimum launch-token output | Spend all `a`; output must meet the minimum |
| Exact output | Positive launch-token output `q` | Maximum collateral input | Pay the rounded-up required input, no more than the maximum |

Get a fresh quote and apply the participant's selected slippage limit under [ADR 0017](../adr/0017-quotes-and-slippage.md). A quote does not reserve state or time. The maximum exact-output input is the full buyer payment. Creator-borne protocol fees are not added on top of it.

Reject zero output, unsupported amounts, insufficient inventory, failed accuracy guards and exceeded limits. Reject an oversized request in full. Do not partially fill it. Exact-input rounding does not produce an unused-input refund. Exact-output spends only the required payment; input above that payment is not part of the trade. The separate private-route return rules still apply to unused input and gas.

### Weighted mode

Let `C` be pricing collateral, `T` be the remaining launch-token reserve, and `(c,t)` be the execution-time raw weights. The continuous reference equations are:

```text
exact input:  output  = T * (1 - (C / (C + a))^(c / t))
exact output: payment = C * ((T / (T - q))^(t / c) - 1)
```

The integer reference floors output and ceils payment. The kernel conservatively approximates these values. Validate its supported domain and the separate trade-relative shortfall/excess-payment limits before a buy can commit. Check `C > 0`, `T > 0`, representable `C + a`, valid weights and nonzero result. Exact output also requires `q <= floor(3*T/10)` under the current kernel. This reserve cap is not an accuracy guarantee.

For exact output, use the certified payment interval to check both payment fit and resulting `C + payment` and `V + payment` fit before calling the raw kernel. A payment that fits alone is not enough. Check fee and counter updates too. A valid weight pair and reserve-drain cap can still produce an unrepresentable payment or post-buy reserve. Reject those cases with a stable checked error; do not let a ceiling carry wrap to zero or use a raw-kernel panic as the handler's response.

The internally selected accuracy target is at most one basis point (`1/10000`) of extra arithmetic loss against the independently rounded ideal trade. It covers exact-input payout shortfall and exact-output payment excess separately. Slippage, protocol fees and natural raw-unit rounding are separate. Apply the rational guards and equal-weight tightening in the package's [error analysis](../../packages/weighted-math/docs/error-analysis.md). Reject zero output and any trade that cannot meet its guard; do not widen the target for dust or ill-conditioned inputs.

Use public scale 52 and internal scale 62. Raw reserves and amounts remain `u128`, each weight is in `[1,2^64]`, and the ratio is in `[1/99,99]`. There is no single-decimal rule or artificial small-pool cap. [Qualification evidence](../../packages/weighted-math/docs/qualification.md) covers mixed decimals, large reserves, small buys, accepted cases and explicit rejections. The 30% exact-output cap alone does not qualify a trade. [Overflow analysis](../../packages/weighted-math/docs/overflow-proof.md) specifies representability guards. Hand-derived bounds still need mathematical review; the checked quote/handler implementation, clock and custody verification remain future work. Fixture agreement and a standalone guest proof do not make this a production-ready LBP.

### Fixed-price mode

Store a reduced positive rational `P/Q`: collateral raw units per launch-token raw unit. There are no weights, logarithms or powers in fixed-price pricing.

```text
exact input:  output  = floor(a * Q / P)
exact output: payment = ceil(q * P / Q)
```

Use checked wide multiplication/division or a proved exact reduction. A product exceeding native `u128` does not itself make a representable quotient invalid. The result and resulting balances must fit `u128`. Do not divide first if that changes the prescribed floor or ceiling. Reject a positive exact-input amount whose output is zero. Reject either form when its output exceeds inventory. Selling the final inventory does not move the original withdrawal time.

If a displayed price is the exact rational `A/B` collateral whole tokens per launch-token whole token, with collateral decimals `dc` and launch-token decimals `dt`, convert it exactly:

```text
P/Q = (A * 10^dc) / (B * 10^dt), reduced by common factors
```

Keep both tokens' decimals. Cancel factors before multiplication where needed. Reject an unsupported or unrepresentable exact price rather than rounding it silently. Decimals are display metadata; execution amounts remain raw integers. Fixed-price results equal their floor/ceiling reference exactly. Integer price granularity must still be visible in the quote.

For `P=3,Q=2`, input `10` buys `6` raw launch tokens and spends all `10`; the fractional output is discarded. Exact output `6` costs `9`, while exact output `7` costs `11`. With only `6` tokens left, the `7`-token request fails in full. With collateral decimals `6`, launch-token decimals `18` and displayed price `3/2`, the reduced raw price is `P=3,Q=2,000,000,000,000`. Input `10` collateral raw units buys `6,666,666,666,666` launch-token raw units. These are exact integer formulas, not floating-point conversions.

## Pricing collateral, custody and fees

The [shared fee contract](namespace-fee-contract.md) and [ADR 0016](../adr/0016-lbp-live-fees.md) remain authoritative for live rates, fee currency, collection and the original-end withdrawal gate. Live LBP rates and anytime LBP collection are accepted internally with external status `To be reviewed by Logos team`. This contract does not change that status.

Keep these records separate:

| Record | Meaning | Buy | Fee collection |
| --- | --- | --- | --- |
| `C` | Weighted pricing collateral; initial seed plus full successful buyer inputs | Add full input | Unchanged |
| `V` | Actual collateral in the vault, including separately recorded surplus | Add full input | Subtract transferred fee |
| `F` | Outstanding collateral fees | Add this trade's fee | Clear to zero |
| `K` | Lifetime collected collateral fees | Unchanged | Add transferred fee |
| `T` | Remaining launch-token reserve/inventory | Subtract output | Unchanged |

Fixed price does not use `C` to set its price. It can retain the same gross-input accounting without making that record a price reserve. Extra deposits are surplus, not fees or pricing liquidity. Track them separately and do not let them change the quoted weighted price.

For full successful input `a`, accrue `f = ceil(a * rate / 10,000)` at the namespace rate read for that trade. Rates are integers in `[0,9999]`. Compute without overflow; quotient/remainder decomposition is valid for this fee formula. Because `f <= a`, a valid fee cannot require more than the input that funds it. Rounding can still leave the creator no net proceeds on a small buy. Never subtract `f` from the buyer's pricing input.

Before withdrawal, with no unrelated collateral transfers, maintain `V = C - K + S`, where `S` is separately recorded surplus, and require `V - S >= F`. Collection reduces `V` and `F`, increases `K`, and leaves `C,T,S` unchanged. Therefore collection alone cannot change weighted pricing. A later buy or a new timestamp can still change the price. The weighted invariant is evaluated against pricing reserves, not the fee-reduced vault. These rules depend on buy-only LBP trading; they do not permit spending unbacked pricing collateral on a sell or another payout. See [ADR 0030](../adr/0030-lbp-pricing-collateral.md).

At or after the original end, the creator can withdraw permitted collateral and unsold launch tokens. Collect outstanding fees to the current treasury in the same transaction as the related proceeds payment. Creator collateral is `V - S` after that collection, not `C`: initial seed plus gross buyer inputs minus all trade fees, less any completed collateral payout. Surplus recovery is separate and is not selected here. Check custody for fees and all remaining payouts. A failed settlement stage commits nothing from that stage. Record completed transfers so a retry cannot pay again; earlier completed stages remain complete.

### Worked weighted fee ledger

These are raw integer accounting examples for a constant `(1,1)` schedule, not LBP program executions. Initial tracked collateral is `1000`; launch-token inventory is `1,000,000`. The weighted kernel outputs for the three example exact-input buys are `499,999`, `250,000` and `62`. Numerical acceptance is still subject to the qualification guard. Treasury changes redirect outstanding fees; rate changes do not recalculate old fees. This example combines final fee collection and both creator payouts in one settlement stage; the implementation can use separate safe stages.

| Step | Input / rate / fee | Pricing `C` | Vault `V` | Owed `F` | Collected `K` | Launch tokens `T` | Transfer |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| Create | Seed `1000` | 1000 | 1000 | 0 | 0 | 1,000,000 | Creator funds both vaults |
| Buy 1 | `1000 / 250 / 25` | 2000 | 2000 | 25 | 0 | 500,001 | Buyer gets 499,999 tokens |
| Collect | Treasury A | 2000 | 1975 | 0 | 25 | 500,001 | A receives 25 collateral |
| Repeat collection | No new fees | 2000 | 1975 | 0 | 25 | 500,001 | No transfer |
| Change rate | `250` to `100` | 2000 | 1975 | 0 | 25 | 500,001 | No transfer |
| Buy 2 | `2000 / 100 / 20` | 4000 | 3975 | 20 | 25 | 250,001 | Buyer gets 250,000 tokens |
| Change treasury | A to B | 4000 | 3975 | 20 | 25 | 250,001 | No transfer |
| Collect | Treasury B | 4000 | 3955 | 0 | 45 | 250,001 | B receives 20 collateral |
| Change rate | `100` to `9999` | 4000 | 3955 | 0 | 45 | 250,001 | No transfer |
| Buy 3 | `1 / 9999 / 1` | 4001 | 3956 | 1 | 45 | 249,939 | Buyer gets 62 tokens; creator earns no net collateral on this buy |
| Settle after end | Collect then pay | 4001 | 0 | 0 | 46 | 0 | B receives 1; creator receives 3955 collateral and 249,939 unsold tokens |
| Retry completed settlement | No new fees or payouts | 4001 | 0 | 0 | 46 | 0 | No second payment |

Total buyer input is `3001`. Total fees are `46`. The creator receives seed `1000` plus net proceeds `2955`, for `3955` collateral. Immediately before settlement, `V=3956` covers the remaining fee `1` and creator payment `3955`. After settlement, `C` is historical pricing state and cannot authorize another payout. If the final stage fails after calculating or attempting either transfer, that transaction leaves `V=3956,F=1,K=45,T=249939` and no payout from that stage.

For fixed price `P=3,Q=2` at rate `250`, exact input `10` gives `6` tokens and accrues fee `1`; the creator retains `9` collateral before collection. Exact output `6` instead costs `9`, accrues fee `1`, and leaves creator net proceeds `8`. The fee is based on the actual payment, not the supplied maximum input.

## Future-handler validation and errors

Validate account/namespace/token/authority proofs and the trusted clock before calculation. Check schedule and pause state, positive amounts, reserve and inventory limits, weight or rational-price inputs, representability and the numerical allowance before invoking a raw kernel path that can panic. Check participant price limits, any allowlist input limit, and vault coverage before committing transfers and all resulting records together. The checked wrapper and its exact program error encoding remain implementation work. Catching a kernel panic is not normal validation.

Use the shared [error categories](namespace-fee-contract.md#events-and-errors). Invalid creation schedules, ended or paused buys, and post-end pause/resume use `InvalidSaleState`. Zero amounts, zero output and requests outside qualified trade bounds use `InvalidTradeAmount`. A failed participant limit uses `SlippageExceeded`. Failed representability or checked record updates use `ArithmeticFailure`. Missing inventory or collateral backing uses `InsufficientCustody`. Do not report an arithmetic failure as a successful zero trade.

Trade events contain mode, namespace, sale, trusted timestamp, actual input/output, execution weights when applicable, live rate, fee and resulting fee records. Checkpoint events identify their timestamp and pair. Same-timestamp buys still emit distinct trade events. Pause/resume events occur only for an actual authorized state change. Settlement records identify completed payments without making a retry another payment. Event encoding and clock integration remain unverified.

## Acceptance cases for future LBP handlers

| Case | Required result |
| --- | --- |
| Start-now creation | Store creation-execution time; validate a later end |
| Scheduled start before creation time, or `end <= start` | Reject creation with no funded sale left behind |
| Duration of one supported clock unit | Valid schedule; no product minimum added |
| Endpoint totals differ, invalid weight, or ratio outside `[1/99,99]` | Reject weighted creation |
| Rising, falling or constant valid schedule | Exact interpolation and complement; no fixed-price substitution |
| Buy before start or exactly at end | Reject; no record, transfer, checkpoint or allowance change |
| Buy exactly at start | Use the start pair and current reserves |
| Buy without any poke | Use execution-time weights; save checkpoint if timestamp changed |
| Two successful buys at one timestamp | One checkpoint value; two prices from successive reserves and two trade events |
| Poke followed by buy at the same timestamp | Buy uses that pair but current reserves; no extra checkpoint write |
| Failed buy after weights were calculated | No checkpoint, reserve, fee, transfer, allowance or completed-trade event change |
| Pause across a weight change | No buys; schedule continues; resume uses current weights and original end |
| Authorized repeated pause/resume before end | Success with no new state change or pause/resume event |
| Old creator holder or namespace administrator attempts pause | `Unauthorized`, including an otherwise repeated call |
| Resume at or after end | `InvalidSaleState`; sale stays closed |
| Either buy form exceeds its participant limit | Full rejection, no fallback or partial fill |
| Fixed-price exact input leaves a fractional output | Spend full input; floor output; no remainder refund |
| Fixed-price requested output exceeds inventory | Full rejection; no partial fill |
| Fixed price sells final inventory before end | Further buys fail; creator withdrawal still waits for end |
| Weighted dust, excessive shortfall/overcharge, unrepresentable payment or post-buy reserve | Stable checked rejection before a raw-kernel panic; payment fit alone is insufficient |
| Mixed token decimals | Same raw equations and exact price conversion; no floating-point or shared-decimals assumption |
| Fee rate changes between quote and execution | Use execution rate; buyer pricing input remains full; old fees remain fixed |
| Fee collection during a weighted sale | `C,T` and same-time quote unchanged; `V,F,K` updated once |
| Treasury changes while fees remain outstanding | Full collection goes to the new current treasury |
| Zero or repeated collection | Success without another transfer or fee-record change |
| Paused sale with fees | Permissionless collection still works; no early creator withdrawal |
| End reached without a final transaction | Trading is closed and authorized settlement is available |
| Settlement stage fails | That stage leaves balances, records and payouts unchanged |
| Settlement retry after a completed payout | No duplicate payment; incomplete permitted stages can continue |
| Enabled allowlist purchase fails or succeeds | Failure preserves allowance; success consumes it atomically across both routes |

Selected-toolchain math parity, a real standalone math proof and these acceptance requirements do not establish a working LBP handler, custody integration, private route or full-operation cost. Keep those evidence gaps visible under [ADR 0022](../adr/0022-verification-boundaries.md).
