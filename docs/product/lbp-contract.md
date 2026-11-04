# Fixed-price and weighted sale mechanics

The fixed-price sale has a four-transaction setup path, public exact-input and exact-output buys, permissionless fee collection, creator proceeds settlement and a separate creator-allocation claim on the actor interface. Setup execution, settlement token execution, real proofs and deployment readiness remain pending. Settlement tests cover public host handlers, not actual transfers, burns or transaction rollback. See [ADR 0032](../adr/0032-fixed-price-lbp-setup.md), [ADR 0033](../adr/0033-fixed-price-lbp-recorded-collateral.md) and the [workspace guide](../architecture/workspace-guide.md#execution-and-evidence-limits) for checks and limits.

## Fixed-price setup

The shared launch factory can create a new launch token, set positive sale inventory and an optional creator allocation, bind the sale to the curve-owned canonical `NamespaceConfig`, and validate setup. Creation saves an immutable `AfterSetup` or `AfterEnd` allocation release choice. The pool starts with zero recorded collateral. The price is a positive exact ratio in smallest units; token decimals are display data. The start uses the inclusion-time bounds defined in [ADR 0032](../adr/0032-fixed-price-lbp-setup.md). The end time is optional. Without an end time, setup and activation have no time deadline, and the sale stays open until it sells out. With an end time, setup and activation must finish before it, and buys stop at it. Sellout closes the sale sooner. The per-sale creator role initializes with the creator signer in the creation claim. Final setup funds sale inventory into the fixed-price sale vault. `AfterSetup` pays the creator allocation only after successful final setup; `AfterEnd` keeps it in escrow until the end or sellout makes it eligible. Its separate claim follows [ADR 0025](../adr/0025-sale-closing.md). Unsolicited collateral remains surplus and does not block activation or enter sale accounting. See [ADR 0033](../adr/0033-fixed-price-lbp-recorded-collateral.md). [ADR 0032](../adr/0032-fixed-price-lbp-setup.md) defines setup rules and review limits.

## Settlement

A sale closes at its optional end or sellout without a final trade or separate close call. Only the current creator-role holder can authorize payouts, and that creator is the recipient. Proceeds settlement pays all recorded net proceeds, collects remaining fees to the current namespace treasury and burns unsold sale inventory. Donations remain surplus. The separate `AfterEnd` allocation claim can run before or after proceeds settlement. Successful retries do not pay or burn twice. See [ADR 0025](../adr/0025-sale-closing.md).

## Weighted setup and buys

The factory creates weighted sales in the same four setup stages. Typed weighted inputs set a positive collateral seed, normalized weight endpoints, and an explicit start and end. Final setup transfers the full seed from the creator. A vault donation stays surplus. Weighted public buys use a buyer-selected time window and the least favourable scheduled pair in that window. Fees do not reduce pricing input. See [ADR 0026](../adr/0026-lbp-clock-and-poke.md), [ADR 0030](../adr/0030-lbp-pricing-collateral.md) and [ADR 0031](../adr/0031-shared-launch-factory.md).

## Scope

The implemented scope includes creation, funding, public buys, permissionless fee collection and creator payouts. Fixed-price sales are buy-only and use fee-first pricing under [ADR 0035](../adr/0035-fixed-price-swap-fees.md). Trader-borne fixed-price fees are an additional RFP-016 departure with external status `To be reviewed by Logos team`. End hooks, delayed settlement, cancellation and expired-recovery handlers remain future work. Weighted virtual accounting, private fixed-price or weighted sales, allowlists, and full SDK and CLI flows are also out of scope.

## Future work

Weighted observation records and lifecycle handlers remain future work. The pricing and accounting decisions are in [ADR 0021](../adr/0021-modes-and-open-limits.md), [ADR 0026](../adr/0026-lbp-clock-and-poke.md) and [ADR 0030](../adr/0030-lbp-pricing-collateral.md). The weighted-math kernel is unaudited and does not establish pool behavior.
