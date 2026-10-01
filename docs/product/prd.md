# Product goals and use cases

This brief describes the planned launchpad. It uses the terms in [CONTEXT.md](../../CONTEXT.md) and the decisions in the [ADR index](../adr/README.md). It is not a list of shipped features.

## Goal

Give creators a choice of bonding-curve or LBP sales on LEZ. Give participants clear quotes, explicit trade limits and access to sale history. Let other operators use these tools within independent namespaces, without building a separate trading client.

The product responds to [RFP-015](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-015-bonding-curve-launchpad.md) and [RFP-016](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md), with [curve](https://github.com/logos-co/rfp/issues/118) and [LBP](https://github.com/logos-co/rfp/issues/119) proposals. Current decisions and RFP differences are recorded in the ADRs linked below.

## Users and interfaces

The admin mini-app serves namespace administrators only. The configured-sales mini-app serves creators and participants within its configured namespace. These are separate authority roles, even when one person holds more than one role. App placement does not grant or merge rights. A non-UI shared core supports both apps. Both apps request indexed listings, history and relevant monitoring through a backend module read API, not direct indexer HTTP calls. Indexed data can lag; current chain state remains authoritative for a quote, transaction and contract execution. The CLI lets developers and operators use the same SDK logic. See [ADR 0019](../adr/0019-product-packages.md).

| Actor | Goal | Interface | Expected outcome | Rule or open detail |
| --- | --- | --- | --- | --- |
| Namespace administrator | Manage a launchpad namespace | Admin app or CLI | Change the fee rate and current treasury through separate operations; manage admin authority | AccountId administration; creator rights remain separate ([0006](../adr/0006-namespaces-and-authority.md)) |
| Creator | Create a curve sale | Configured-sales app or CLI | Submit typed curve inputs to the curve factory, fund allocations `D` and `R`, and optionally set expiry | No early close; settlement burns unsold `D` after expiry ([0025](../adr/0025-sale-closing.md)) |
| Creator | Create an LBP | Configured-sales app or CLI | Submit LBP inputs to the independent LBP factory, choose weighted or fixed-price mode, fund the sale, and set its schedule | Pause controls do not stop the clock; LBP authority for those controls and allowlist design remain open ([0021](../adr/0021-modes-and-open-limits.md), [0026](../adr/0026-lbp-clock-and-poke.md)) |
| Participant | Find a sale and trade | Configured-sales app or CLI | Inspect the sale, request a fresh quote and submit a buy with an explicit slippage limit; sell back to a curve when its mode permits | Execution uses current state; a quote does not reserve a price ([0017](../adr/0017-quotes-and-slippage.md)) |
| Participant | Use the private route | Configured-sales app or CLI through the SDK | Deshield, trade through a fresh public account and re-shield within one transaction | Public effects stay visible; provider integration and failure behavior remain unverified. Private curve sells and private LBP details remain open ([0018](../adr/0018-private-participation.md)) |
| Any caller | Collect accrued protocol fees | SDK or CLI | Transfer all accrued collateral fees to the current treasury, at any time | Caller cannot redirect funds; zero accrued fees cause no transfer ([0015](../adr/0015-fee-accounting.md), [0016](../adr/0016-lbp-live-fees.md)) |
| Creator | Settle a closed sale | Configured-sales app or CLI | Collect remaining fees atomically with the relevant reserve or proceeds transfer; complete permitted allocation payouts | Closure and settlement are separate; stages can span transactions; no automatic DEX graduation ([0020](../adr/0020-settlement-and-graduation.md)) |
| Participant or operator | Inspect history | App history or existing explorer tools | Request indexed trades and sale events through the backend module read API without exposing persistent private participant identifiers | History and monitoring remain in scope; a separate analytics view does not ([0027](../adr/0027-events-and-history.md)) |

## Sale journey

This planned journey shows user actions and sale state, not transaction or account schemas. Namespace administration does not grant the creator's withdrawal rights.

```mermaid
flowchart TB
    Namespace[Administrator configures namespace] --> Create[Creator configures and funds sale in configured-sales app]
    Create --> Trade[Participant inspects, quotes and trades]
    Trade --> Mechanism{Sale mechanism}
    Mechanism --> Curve[Curve: all D sold or optional expiry]
    Mechanism --> LBP[LBP: scheduled end]
    Curve --> Closed[Trading closed]
    LBP --> Closed
    Closed --> Settle[Creator settles permitted proceeds and allocations]
```

Trading can repeat while the sale is open and its mode permits it. Expiry does not depend on a participant making a final trade. Fee collection can happen before or after closure; it is not blocked on creator settlement.

See the planned [curve pool creation](../architecture/sequence-diagrams/create-curve-pool.md) and [LBP pool creation](../architecture/sequence-diagrams/create-lbp-pool.md) sequences. They show logical stages only. A curve pool retains staged creation, a pending trading guard and factory custody. The LBP sequence does not assign those curve details to LBP creation.

## Mechanism rules

| Rule | Curve | LBP |
| --- | --- | --- |
| Price | Constant-product pricing with virtual reserves and real-reserve limits | Time-derived weights, or fixed-price mode |
| Close | All `D` sold, or optional expiry, whichever occurs first | Scheduled end, using the weight schedule's clock |
| Minimum duration | No contract minimum for optional expiry; a frontend rule is undecided | Curve expiry policy does not change the LBP schedule |
| Protocol fee | Collateral deducted from buy input or sell output | Creator-borne collateral claim; full buyer input enters pricing |
| Fee collection | Permissionless, anytime, to current treasury | Permissionless, anytime, to current treasury |

`R` is the curve's DEX-seed reserve. It stays untouched until close. The creator can withdraw it after close when it is not used for graduation.

## Scope limits and open work

- Private curve sells, private LBP buys and allowlist proofs remain required work with unfinished specifications. The [private purchase sequence](../architecture/sequence-diagrams/private-purchase.md) is a desired route, not an implementation or wallet-support claim.
- Integrate the LP-0021 Wallet Provider SDK once developed. Private-route support still needs verification; see [ADR 0029](../adr/0029-wallet-integration.md).
- The per-block allocation ceiling is deferred. Live LBP fee rates, anytime LBP collection and omission of curve contract minimum-duration validation differ from the RFPs. The relevant ADRs are marked `To be reviewed by Logos team`.
- Automatic DEX graduation is inactive. Direct neutral pools are internal primitives, not a separate sale product.
- Use the existing indexer and explorer tools. Do not add a separate indexer or the excluded derived analytics view.
- Platform compatibility, precise input bounds and private-route details need further specification and tests. This brief does not choose new limits or authority rules.

See the [namespace, custody, and fee contract](namespace-fee-contract.md) for shared account checks, balance examples, collection behavior, and acceptance cases. See [architecture and interfaces](../architecture/system.md) for component responsibilities and [repository layout](../architecture/repository-layout.md) for packages and checks.
