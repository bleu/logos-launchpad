# Namespace, custody and fee overview

These rules apply to fixed-price, bonding-curve and weighted sales. The linked ADRs define the contract. Use the terms in [CONTEXT.md](../../CONTEXT.md); see the [workspace guide](../architecture/workspace-guide.md#execution-and-evidence-limits) for implementation evidence.

## Roles

Anyone can create a namespace with an administrator, fee rate and treasury. Each sale keeps its creation namespace. Namespace administration does not grant creator rights.

| Action | Authority |
| --- | --- |
| Change fee or treasury in separate operations | Current namespace administrator |
| Transfer administration | Current administrator and accepting successor |
| Renounce administration permanently | Current administrator |
| Create a sale or authorize manual payouts without an adapter | Sale creator role |
| Execute a configured graduation hook | Anyone, after the selected ending condition |
| Collect namespace fees | Anyone; payment goes to the current treasury |

See [role authority](../adr/0006-namespaces-and-authority.md) and [settlement routes](../adr/0020-settlement-and-graduation.md). Namespace, creator and direct pool-owner roles remain separate.

## Funds

Account checks cover the expected address, program owner, token definition and permission. Curve custody must cover real reserves and accrued fees. During trading, fixed-price and weighted vaults must satisfy `vault >= recorded collateral + accrued fees`. Weighted pricing collateral includes the seed but excludes namespace fees and surplus. Donations do not increase payout rights.

See [custody and freeze](../adr/0028-token-custody-and-freeze.md), [curve arithmetic](../adr/0009-curve-arithmetic.md), [fixed-price recorded collateral](../adr/0033-fixed-price-lbp-recorded-collateral.md) and [weighted pricing collateral](../adr/0030-lbp-pricing-collateral.md).

## Fees and settlement

Namespace fees use collateral buy input or sell output. Buys price net collateral after fees; exact-output buys gross up the required net amount. Retained sell fees stay in reserves or recorded proceeds and are separate from collectible namespace fees. [ADR 0015](../adr/0015-fee-accounting.md) defines rates, rounding, collection and treasury aliases.

Fixed-price and weighted sales use their creation-time rate snapshot. Namespace fee updates affect new sales; freeze and treasury remain live. Curve sales continue to use the current namespace rate. Fixed-price and weighted-sale fee departures are accepted internally but remain `To be reviewed by Logos team` externally. Local execution does not establish external approval.

The selected ending policy controls creator payouts and `AfterEnd` allocations. Fee collection does not wait for settlement. A failed settlement transaction must roll back its effects; earlier completed stages stay complete. Recorded completion prevents duplicate payouts. See [ending rules](../adr/0025-sale-closing.md) and [settlement](../adr/0020-settlement-and-graduation.md).

## Events and errors

[ADR 0031](../adr/0031-shared-launch-factory.md#program-events) defines committed sale events. Source declarations and the [generated actor schema](../../idl/launchpad.json) define payloads and error types. Event emission does not establish indexer delivery or history. Apps read the existing indexer through the backend module API under [ADR 0027](../adr/0027-events-and-history.md).
