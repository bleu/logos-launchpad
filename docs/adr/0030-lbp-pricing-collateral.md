# ADR 0030: Fee-neutral weighted-sale pricing collateral

Status: Accepted internally; host and seeded buy/fee-collection guest checks passed. Weighted setup and rollback guest checks passed. Weighted settlement and payout rollback have local guest checks; real proofs remain pending.

Date: 2026-10-02

## Context

Collecting namespace fees from the vault must not change the next weighted buy's price. [ADR 0031](0031-shared-launch-factory.md) supersedes the original creator-borne incidence and full-input pricing rule.

## Decision

Keep pricing collateral separate from actual vault collateral. Weighted buys deduct namespace fees before pricing; exact output grosses up the required net collateral. During trading, pricing collateral equals recorded collateral: the initial seed plus net successful buyer inputs. Require `vault >= recorded collateral + accrued fees`. Collection reduces vault collateral and accrued fees together, leaving pricing and recorded collateral unchanged. Surplus does not enter either record. Settlement pays recorded collateral, including the seed, once; pricing collateral grants no separate payout right. See [ADR 0033](0033-fixed-price-lbp-recorded-collateral.md). This applies to buy-only weighted sales; it adds no sells or retained fees.

## Consequences

Code and tests must check vault coverage, prevent duplicate payouts and keep pricing unchanged by collection. Fee policy and pending Logos review remain under [ADR 0016](0016-lbp-live-fees.md).

## Sources

- [Collateral-denominated fees](0015-fee-accounting.md)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
