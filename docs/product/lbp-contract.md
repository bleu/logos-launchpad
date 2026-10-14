# Fixed-price and weighted sale mechanics

Fixed-price and weighted sale pools do not yet provide full product behavior. Fixed-price sale setup has a four-transaction implementation. Locked local verification passed using development receipts; real proofs and deployment readiness remain pending. See [ADR 0032](../adr/0032-fixed-price-lbp-setup.md).

## Fixed-price setup

The shared launch factory can create a new launch token, set a positive sale inventory and optional creator allocation, bind the sale to the curve-owned canonical `NamespaceConfig`, and validate setup. Creation saves an immutable `AfterSetup` or `AfterEnd` allocation release choice. The pool starts with zero recorded collateral. The price is a positive exact ratio in smallest units; token decimals are display data. The schedule uses trusted time. The per-sale creator role initializes with the creator signer in the creation claim. Final setup funds sale inventory into the fixed-price sale vault. `AfterSetup` pays the creator allocation only with successful final setup; `AfterEnd` retains it in escrow. Its later settlement or payout handler is future work. Unsolicited collateral remains surplus and does not block activation or enter sale accounting. See [ADR 0033](../adr/0033-fixed-price-lbp-recorded-collateral.md). [ADR 0032](../adr/0032-fixed-price-lbp-setup.md) defines these rules and their review limits.

## Scope

The implemented scope stops at creation and funding. It does not add buys, sells, weighted virtual accounting, fees, close, end payout, end hooks, delayed settlement, cancellation or expired-recovery handlers, private fixed-price or weighted sales, allowlists, or full SDK and CLI flows.

## Future work

Weighted-sale pricing, trading, clock checkpoints, pricing collateral, fees and lifecycle work remain separate. Their target design is recorded in [ADR 0021](../adr/0021-modes-and-open-limits.md), [ADR 0026](../adr/0026-lbp-clock-and-poke.md) and [ADR 0030](../adr/0030-lbp-pricing-collateral.md). The weighted-math kernel is unaudited and does not establish pool behavior.
