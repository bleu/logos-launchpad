# Product goals and use cases

This brief describes the planned launchpad. It is not a list of shipped features. Use the terms in [CONTEXT.md](../../CONTEXT.md), the decisions in the [ADR index](../adr/README.md) and the current [execution limits](../architecture/workspace-guide.md#execution-and-evidence-limits).

## Goal

Give creators a choice of fixed-price, bonding-curve or weighted sales on LEZ. Give participants fresh quotes, explicit trade limits and sale history. Let operators use these tools within independent namespaces.

The product responds to [RFP-015](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-015-bonding-curve-launchpad.md) and [RFP-016](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md), with [bonding-curve](https://github.com/logos-co/rfp/issues/118) and [fixed-price and weighted sale](https://github.com/logos-co/rfp/issues/119) proposals. Internal acceptance does not establish external approval. The ADR index keeps RFP departures marked `To be reviewed by Logos team` visible.

## Users and interfaces

The admin app handles namespace administration. The configured-sales app handles creator and participant actions within its configured namespace. The apps are separate packages with separate authority roles. They use a non-UI shared core; the CLI uses the same SDK logic. See [ADR 0019](../adr/0019-product-packages.md).

| Actor | Goal | Rule |
| --- | --- | --- |
| Namespace administrator | Set the fee rate and treasury; manage administration | Administration does not grant creator rights ([0006](../adr/0006-namespaces-and-authority.md)) |
| Creator | Configure and fund a sale | Immutable typed inputs and shared setup policies; separate pricing and custody ([0031](../adr/0031-shared-launch-factory.md)) |
| Participant | Inspect, quote and trade | A fresh quote does not reserve a price. Use an explicit slippage limit; sell only when the trade mode permits ([0017](../adr/0017-quotes-and-slippage.md), [0021](../adr/0021-modes-and-open-limits.md)) |
| Participant | Use a sale allowlist | Prove membership and control of an approved public trader. Wallet preparation and private execution remain pending ([allowlist design](../architecture/allowlist-proof.md)) |
| Participant | Use and recover a private trade | Complete deshield, public trade and private returns in one transaction. An unknown outcome blocks replacement ([private participation](../architecture/private-participation.md)) |
| Any caller | Collect namespace fees | Pay all accrued fees to the current treasury; the caller cannot redirect them ([0015](../adr/0015-fee-accounting.md)) |
| Creator | Settle manually without an adapter | Receive unsold inventory, recorded collateral and `R` after the selected ending condition; allocation claims remain separate ([0020](../adr/0020-settlement-and-graduation.md)) |
| Any caller | Execute or retry a configured graduation hook | Use the saved adapter and post-close settlement route. Callback success does not prove DEX liquidity ([0020](../adr/0020-settlement-and-graduation.md)) |
| Participant or operator | Inspect listings and history | Read the existing indexer through a backend module API. Chain state governs execution ([0027](../adr/0027-events-and-history.md)) |

## Sale types

| Rule | Fixed-price sale | Bonding-curve sale | Weighted sale |
| --- | --- | --- | --- |
| Price | Positive exact ratio in smallest units | Constant product with virtual reserves and real-reserve limits | Balances and scheduled weights |
| Buy forms | Exact input or exact output | Exact input or exact output | Exact input or exact output |
| Ending policy | `Scheduled`, `Depletion` or `Either` | `Scheduled`, `Depletion` or `Either` | `Scheduled` or `Either`; schedule end required |
| Overview | [Fixed-price and weighted sales](lbp-contract.md) | [Bonding-curve sale](curve-contract.md) | [Fixed-price and weighted sales](lbp-contract.md) |

All three factory engines support `BuyOnly` or `BuyAndSell`. Permitted sells use exact input. Namespace fees use collateral buy input or sell output. Selected depletion is final; `Scheduled` waits for its end even when inventory is empty. See [trade modes](../adr/0021-modes-and-open-limits.md), [fees](../adr/0015-fee-accounting.md) and [ending rules](../adr/0025-sale-closing.md).

See the architecture [creation and private-purchase sequences](../architecture/sequence-diagrams/README.md) for flows. Fee collection can run before settlement. A sale does not need a final trade to reach its scheduled end.

## Scope limits

The QML apps and shared core remain status placeholders. Private routes return explicit unsupported errors. Normal weighted trades fail with `ClockUnavailable` until a supported timestamp provider replaces the temporary test clock. Weighted math is unaudited. See the [workspace guide](../architecture/workspace-guide.md#execution-and-evidence-limits) for current checks and evidence gaps; no project deployment is claimed.

Allowlists apply to buys in all three engines. Approval permits repeated buys without a purchase cap and does not prove human uniqueness. The wallet should prefer an unused approved address and warn before deliberate reuse. Fresh addresses do not guarantee unlinkability. See [ADR 0021](../adr/0021-modes-and-open-limits.md) and the [public execution evidence](../architecture/allowlist-proof.md#public-execution-evidence).

The per-block allocation ceiling is deferred. Fixed-price and weighted-sale fee departures and the curve minimum-duration-validation departure still need external review. Wallet integration awaits LP-0021 under [ADR 0029](../adr/0029-wallet-integration.md). Privacy warnings are required before approval; acknowledgement frequency remains undecided.

Direct neutral pools are internal primitives. Use the existing indexer and explorer; a separate indexer and derived analytics view are outside scope. See [verification boundaries](../adr/0022-verification-boundaries.md) and [repository layout](../architecture/repository-layout.md).
