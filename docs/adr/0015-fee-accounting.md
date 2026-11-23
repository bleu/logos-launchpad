# ADR 0015: Collateral fees and current-treasury collection

Status: Accepted internally; fixed-price and weighted-sale RFP departures: To be reviewed by Logos team

## Decision

### Sale rates and collection

All engines charge namespace fees in collateral: buy input or sell output. Curve factory collateral is token1. Every trade reads the current namespace rate. A namespace fee update applies to future trades on existing sales as well as new sales. SDK quotes use a current namespace snapshot and trades enforce the participant’s slippage limit. The namespace administrator sets a rate from 0 to 9,999 basis points. Treasury and freeze are also live namespace settings.

Track accrued namespace fees separately from reserves and creator proceeds. Anyone can collect the full amount at any time. Collection is idempotent and pays the current namespace treasury, including fees accrued before a treasury change. The caller cannot redirect payment. Terminal settlement collects the remainder atomically with the relevant payout.

If the curve pool is the treasury, record and clear the fee liability without self-transfer. Reserve withdrawal leaves those collected tokens in the vault. Do not treat them as liquidity. Fee-rate and treasury updates remain separate [administration operations](0006-namespaces-and-authority.md).

### Buy fees and fixed-price formulas

Price buys from net collateral after namespace fees. Exact-output buys gross up the required net collateral. For fixed-price exact input, use `fee = ceil(gross * bps / 10_000)` and `net = gross - fee`; round token output at the saved price down. Spend the full gross input without a rounding refund.

For fixed-price exact output, round the saved-price collateral requirement up to obtain `net`, then use `gross = ceil(net * 10_000 / (10_000 - bps))` and `fee = gross - net`. The maximum-input limit caps gross payment. Use checked arithmetic; reject zero net input, zero output and inventory or slippage violations. Record net collateral as creator proceeds and fees separately. Unsolicited transfers remain surplus. At positive rates, a fixed gross input buys fewer tokens and exact output costs more gross collateral. Quotes must use those amounts.

Weighted seed-inclusive accounting follows [ADR 0030](0030-lbp-pricing-collateral.md). Fixed-price recorded proceeds follow [ADR 0033](0033-fixed-price-lbp-recorded-collateral.md).

### Retained sell fees

All three sale types accept an immutable creator-selected `sell_fee_bps` in `0..=9_999`, defaulting to zero. Calculate the namespace and retained sell fees separately as `ceil(gross_output * rate / 10_000)`. Pay gross output minus both fees. See [curve pricing](0009-curve-arithmetic.md) for the effect on the curve constant.

Before each sell, require the current namespace rate plus the saved retained sell rate to be below 10,000 basis points. Reject rounding that leaves no net output. Buys only use the namespace rate. Retained collateral stays in reserves or recorded proceeds and is excluded from the treasury claim.

## Reason and limits

Fixed-price and weighted sales charge traders because both support `BuyAndSell`. Deducting the buy fee before pricing leaves net collateral to back the purchased tokens and records the treasury claim separately. With creator-borne buy fees, a fixed-price buyer could pay 100 units for 100 tokens while a 1-unit treasury claim leaves only 99 units for a full-price sell back. That model needs a creator subsidy, different sell terms or a buy-only restriction. Net-input pricing avoids this conflict without a separate subsidy. Creators receive net trading proceeds; buyers receive fewer tokens for the same gross input. Exact-output buyers pay the grossed-up amount. These effects must appear in quotes and confirmation views.

The proposed buy-only model can support creator-borne fees. This choice retains bidirectional sales and makes each trade pay its own namespace fee. It is a product change, not a consequence of shared code. [Balancer swap fees](https://balancer.gitbook.io/balancer-v2/concepts/fees) provide a precedent for trader-paid fees, but do not establish that this project's economics are externally approved. Fee burden does not require live rates or early collection; those are separate decisions.

Separate fees from backing so collection cannot change pricing or spend remaining obligations. Vault coverage, rounding and production-input evidence remain required; see [execution limits](../architecture/workspace-guide.md#execution-and-evidence-limits).

RFP-016 F.2 and F.10 specify a creation-time rate snapshot. A live rate gives all engines one rule and lets the administrator change fees for the whole namespace. Creators give up a guaranteed rate for the life of a sale. A sufficiently high namespace rate can block sells when combined with the saved retained sell fee. Anytime collection, trader-borne fees and payouts under the selected ending policy also differ from the RFP. See the [review register](../product/proposal-departures.md).

## Sources

- [Curve revised fee decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0005-dual-input-fees-and-monotonic-reserve-product.md)
- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
