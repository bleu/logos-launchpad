# ADR 0015: Collateral fees and current-treasury collection

Status: Accepted internally; fixed-price and weighted-sale RFP departures: To be reviewed by Logos team

## Decision

### Live rates and collection

All engines charge namespace fees in collateral: buy input or sell output. Curve factory collateral is token1. Use the namespace rate at execution, not a creation snapshot. Rates at creation and update must be integers in `0..=9_999` basis points. Reject 10,000 or more. Rounding can still consume a small trade, so trade guards remain necessary.

Track accrued namespace fees separately from reserves and creator proceeds. Anyone can collect the full amount at any time. Collection is idempotent and pays the current namespace treasury, including fees accrued before a treasury change. The caller cannot redirect payment. Terminal settlement collects the remainder atomically with the relevant payout.

If the curve pool is the treasury, record and clear the fee liability without self-transfer. Reserve withdrawal leaves those collected tokens in the vault. Do not treat them as liquidity. Fee-rate and treasury updates remain separate [administration operations](0006-namespaces-and-authority.md).

### Buy fees and fixed-price formulas

Price buys from net collateral after namespace fees. Exact-output buys gross up the required net collateral. For fixed-price exact input, use `fee = ceil(gross * bps / 10_000)` and `net = gross - fee`; round token output at the saved price down. Spend the full gross input without a rounding refund.

For fixed-price exact output, round the saved-price collateral requirement up to obtain `net`, then use `gross = ceil(net * 10_000 / (10_000 - bps))` and `fee = gross - net`. The maximum-input limit caps gross payment. Use checked arithmetic; reject zero net input, zero output and inventory or slippage violations. Record net collateral as creator proceeds and fees separately. Unsolicited transfers remain surplus. At positive rates, a fixed gross input buys fewer tokens and exact output costs more gross collateral. Quotes must use those amounts.

Weighted seed-inclusive accounting follows [ADR 0030](0030-lbp-pricing-collateral.md). Fixed-price recorded proceeds follow [ADR 0033](0033-fixed-price-lbp-recorded-collateral.md).

### Retained sell fees

Creation fixes creator-selected `sell_fee_bps` in `0..=9_999`, defaulting to zero. For each sell, calculate namespace fee and retained fee independently as `ceil(gross_output * rate / 10_000)`. Do not round their combined rate. Pay gross output minus both fees.

Before each sell, require the live namespace rate plus saved `sell_fee_bps` to be below `10_000`. Reject rounding that leaves no net output. This combined check does not apply to buys. Debit reserves or recorded collateral by `gross_output - retained_fee`. Retained collateral stays in reserves or recorded proceeds. It is not collectible debt and grants no early creator payout. Fixed-price pricing and settlement timing stay unchanged.

## Reason and limits

Fixed-price and weighted sales charge traders because both support `BuyAndSell`. Deducting the buy fee before pricing leaves net collateral to back the purchased tokens and records the treasury claim separately. With creator-borne buy fees, a fixed-price buyer could pay 100 units for 100 tokens while a 1-unit treasury claim leaves only 99 units for a full-price sell back. That model needs a creator subsidy, different sell terms or a buy-only restriction. Net-input pricing avoids this conflict without a separate subsidy. Creators receive net trading proceeds; buyers receive fewer tokens for the same gross input. Exact-output buyers pay the grossed-up amount. These effects must appear in quotes and confirmation views.

The proposed buy-only model can support creator-borne fees. This choice retains bidirectional sales and makes each trade pay its own namespace fee. It is a product change, not a consequence of shared code. [Balancer swap fees](https://balancer.gitbook.io/balancer-v2/concepts/fees) provide a precedent for trader-paid fees, but do not establish that this project's economics are externally approved. Fee burden does not require live rates or early collection; those are separate decisions.

Separate fees from backing so collection cannot change pricing or spend remaining obligations. Vault coverage, rounding and production-input evidence remain required; see [execution limits](../architecture/workspace-guide.md#execution-and-evidence-limits).

RFP-016 F.2 and F.10 require creation-time fee snapshots; F.10 bars collection before scheduled end, and F.5 gates creator withdrawal on that end. This design uses live rates, anytime collection and payouts when the [selected ending policy](0025-sale-closing.md) permits them. Trader-borne fixed-price fees are an additional departure. Fixed-price and weighted-sale departures have external status `To be reviewed by Logos team`. Internal acceptance and local execution do not establish external approval.

## Sources

- [Curve revised fee decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0005-dual-input-fees-and-monotonic-reserve-product.md)
- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
