# ADR 0032: Define fixed-price sale creation and funding

Status: Four-stage implementation migrated; local actor host and guest-build checks passed. Setup execution, real proofs and deployment readiness remain pending.

Date: 2026-10-13

## Context

Fixed-price sale creation needs a defined token, allocation, price, schedule, authority and funding path. The curve factory already has staged creation rules, but curve state and custody do not define fixed-price sale behavior.

This record partially supersedes [ADR 0006](0006-namespaces-and-authority.md) by defining the fixed-price sale creation and funding gap. ADR 0006 remains correct that curve stages, pending guards and factory custody do not define fixed-price sale creation. This record defines only fixed-price sale creation and funding and leaves its mechanism and platform review limits visible.

## Decision

Reuse the curve-owned `NamespaceConfig` as the canonical namespace configuration for fixed-price sale validation. Namespace role administration remains separate from a sale creator role. Each fixed-price sale has a separate creator role, initialized with the creator signer in the same creation claim. This leaves no public claim window. Readonly namespace account data exposes its format version, not a duplicate administrator or hidden role-authority bytes. This work does not migrate existing curve creator Authority NFTs.

A fixed-price sale creates a new launch token with a strictly positive sale inventory and an optional separate creator allocation. It has no curve `R` allocation and starts with zero recorded collateral under [ADR 0033](0033-fixed-price-lbp-recorded-collateral.md). Its price is a strictly positive exact rational ratio of collateral smallest units to launch-token smallest units. Token decimals affect display only.

Public timestamps remain Unix seconds. `SaleStart::Now { timestamp }` saves that value verbatim as the start, including zero, and requires creation inclusion at or after it. There is no freshness check. A scheduled start must not precede the creation block timestamp; its inclusive millisecond limit becomes an exclusive validity upper bound of `start_ms + 1`. The selected ending policy determines whether an end is required: `Scheduled` and `Either` require one after the saved start; `Depletion` has none. Every incomplete setup stage and activation must enter strictly before that end. Without an end they have no end deadline. Late final setup keeps the configured schedule. Convert bounds with checked arithmetic and intersect response windows so an added bound cannot erase an earlier one. See [sale closing](0025-sale-closing.md) for inclusion-time rules.

Creation saves an immutable creator-allocation release choice: `AfterSetup` or `AfterEnd`. The implementation uses four atomic transactions: claim the exact mint and per-sale creator role; split supply into sale inventory and allocation escrow; prepare the pool, vaults and immediate recipient ATA; then fund inventory into the fixed-price sale vault, pay an `AfterSetup` allocation only on successful funding and mark the sale `Complete`. The workflow can span those transactions. `AfterEnd` retains the allocation in escrow until the selected ending policy permits a separate creator-authorized claim under [ADR 0025](0025-sale-closing.md). Any unpaid allocation or future proceeds belong to the current creator-role holder. This scope does not add creator-role transfer or renunciation.

[ADR 0033](0033-fixed-price-lbp-recorded-collateral.md) supersedes the original physical collateral-empty activation rule. Unsolicited collateral after `PoolPrepared` remains surplus and does not block activation. The owner claim still blocks an outsider from precreating the ATA with `Create`.

## Consequences

Fixed-price buys and permissionless fee collection are specified in [ADR 0033](0033-fixed-price-lbp-recorded-collateral.md) and [ADR 0016](0016-lbp-live-fees.md). Creator proceeds settlement and the separate `AfterEnd` allocation claim follow [ADR 0025](0025-sale-closing.md); signed guest tests cover settlement token operations and rollback. Weighted-sale work remains separate. This design does not add end hooks, delayed settlement, cancellation or expired-recovery handlers, private fixed-price sale routes, allowlists, or full SDK and CLI flows.

The actor migration replaces the old instruction encoding with typed messages. Rebuild clients, guest binaries and image IDs together. See the [workspace guide](../architecture/workspace-guide.md) for interface generation and checks.

## Sources

- [Shared launch factory](0031-shared-launch-factory.md)
- [Namespaces and authority](0006-namespaces-and-authority.md)
- [Typed sale inputs](0007-typed-sale-inputs.md)
- [Weighted-sale clock and poke](0026-lbp-clock-and-poke.md)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
