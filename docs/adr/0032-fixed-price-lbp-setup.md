# ADR 0032: Define fixed-price sale creation and funding

Status: Accepted; four-stage setup implemented with local signed execution evidence.

## Decision

Use curve-owned `NamespaceConfig` as canonical namespace configuration for fixed-price validation. Namespace administration is separate from the sale creator role. Initialize that role with the creator signer in the same creation claim, with no public claim window. Readonly namespace data exposes its format version, not duplicate administrator or hidden authority bytes.

Create a new launch token with positive sale inventory and optional separate creator allocation. There is no curve `R`. Recorded collateral starts at zero under [ADR 0033](0033-fixed-price-lbp-recorded-collateral.md). Save a positive exact rational price in collateral and launch-token smallest units; decimals affect display only.

Public timestamps use Unix seconds. `SaleStart::Now { timestamp }` saves the supplied value verbatim, including zero, and requires creation inclusion at or after it; there is no freshness check. Scheduled start cannot precede the creation block timestamp. Convert its inclusive millisecond limit to exclusive upper bound `start_ms + 1`.

`Scheduled` and `Either` require an end after saved start; `Depletion` has none. Every incomplete stage and activation must enter before a configured end. Without an end there is no end deadline. Late final setup keeps the original schedule. Convert bounds with checked arithmetic and intersect response windows so added bounds cannot erase earlier bounds. See [ending](0025-sale-closing.md).

Save immutable creator-allocation release `AfterSetup` or `AfterEnd`. Setup has four atomic transactions: claim exact mint and creator role; split inventory and allocation escrow; prepare pool, vaults and immediate recipient ATA; then fund inventory, pay `AfterSetup` allocation and mark `Complete`. The whole workflow spans transactions. Keep `AfterEnd` allocation in escrow for a separate creator-authorized claim when ending permits it. Unpaid allocations and proceeds belong to the current creator-role holder. This setup contract does not add role transfer or renunciation.

Retain fresh-custody and launch-token inventory checks. The owner claim blocks outsider ATA precreation through `Create`. Unsolicited collateral after `PoolPrepared` remains surplus and does not block activation; recorded collateral must remain zero.

## Reason and limits

Staged setup supports resumable preparation without trading unbacked inventory. Curve custody rules do not define this engine's behavior. Weighted seed funding is separate under [ADR 0031](0031-shared-launch-factory.md#shared-setup).

Public setup has local signed execution coverage; see [current evidence](../architecture/workspace-guide.md#execution-and-evidence-limits). Fees, ending and settlement follow [0015](0015-fee-accounting.md), [0025](0025-sale-closing.md) and [0020](0020-settlement-and-graduation.md). Private routes, cancellation, expired recovery, full SDK/CLI journeys, real proofs and deployment readiness remain outside this setup contract. Rebuild typed interfaces, clients, guests and image IDs together after interface changes.

## Sources

- [Shared launch factory](0031-shared-launch-factory.md)
- [Namespaces and authority](0006-namespaces-and-authority.md)
- [Typed sale inputs](0007-typed-sale-inputs.md)
- [Weighted-sale clock and poke](0026-lbp-clock-and-poke.md)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
