# ADR 0030: Fee-neutral weighted-sale pricing collateral

Status: Accepted internally; local execution evidence only; real proofs pending.

## Decision

Keep weighted pricing collateral separate from actual vault collateral. During trading, pricing collateral equals recorded collateral: initial seed plus net buyer inputs minus gross sell outputs plus retained sell collateral. Buys price net input; exact output grosses up required net collateral under [ADR 0015](0015-fee-accounting.md).

Require `vault >= recorded collateral + accrued fees`. Collection reduces vault and accrued fees together, leaving pricing and recorded collateral unchanged. Surplus enters neither record.

Under `BuyAndSell`, checked reverse quotes cap gross sell output at recorded buyer proceeds above the saved seed. This gross budget excludes seed, namespace fees and surplus even if retained fees reduce the debit. Debit pricing and recorded collateral by `gross_output - retained_fee`. Apply [trade limits](0017-quotes-and-slippage.md).

Settlement pays recorded collateral, including seed, once under the selected [route](0020-settlement-and-graduation.md). Pricing collateral grants no separate payout right. [ADR 0033](0033-fixed-price-lbp-recorded-collateral.md) defines the distinct fixed-price accounting boundary.

## Reason and limits

Fee collection and unsolicited transfers must not change the next weighted price. Tests must check vault coverage, price neutrality and duplicate-payout protection. [Local execution evidence](../architecture/workspace-guide.md#execution-and-evidence-limits) does not establish real proofs or production readiness. Fee departures still need Logos review under [ADR 0015](0015-fee-accounting.md).

## Sources

- [Collateral-denominated fees](0015-fee-accounting.md)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
