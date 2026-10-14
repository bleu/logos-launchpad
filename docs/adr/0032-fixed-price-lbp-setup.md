# ADR 0032: Define fixed-price LBP setup

Status: Four-stage implementation; locked local verification passed using development receipts. Real proofs and deployment readiness pending.

Date: 2026-10-13

## Context

Fixed-price LBP creation needs a defined token, allocation, price, schedule, authority and funding path. The curve factory already has staged creation rules, but curve state and custody do not define LBP behavior.

This record partially supersedes [ADR 0006](0006-namespaces-and-authority.md) by defining the fixed-price LBP setup gap. ADR 0006 remains correct that curve stages, pending guards and factory custody do not define LBP creation. This record defines only fixed-price setup and leaves its mechanism and platform review limits visible.

## Decision

Reuse the curve-owned `NamespaceConfig` as the canonical namespace configuration for fixed-price LBP validation. Namespace role administration remains separate from a sale creator role. Each LBP sale embeds its own `AdminConfig`, initialized with the creator signer in the same creation claim. This leaves no public claim window. Readonly namespace account data exposes its format version, not a duplicate administrator or hidden role-authority bytes. This work does not migrate existing curve creator Authority NFTs.

A fixed-price LBP creates a new launch token with a strictly positive sale inventory and an optional separate creator allocation. It has no curve `R` allocation and starts with zero recorded collateral under [ADR 0033](0033-fixed-price-lbp-recorded-collateral.md). Its price is a strictly positive exact rational ratio of collateral smallest units to launch-token smallest units. Token decimals affect display only.

A `StartNow` schedule uses the trusted creation time. A scheduled start must be at or after trusted current time, and the end must be after the selected start. Late final setup does not change the configured end. Activation requires trusted current time before that end.

Creation saves an immutable creator-allocation release choice: `AfterSetup` or `AfterEnd`. The implementation uses four atomic transactions: claim the exact mint and per-sale creator role; split supply into sale inventory and allocation escrow; prepare the pool, vaults and immediate recipient ATA; then fund inventory into the LBP vault, pay an `AfterSetup` allocation only on successful funding and mark the sale `Complete`. The workflow can span those transactions. `AfterEnd` retains the allocation in escrow; its later settlement or payout handler is future work. Any unpaid allocation or future proceeds belong to the current creator-role holder. This scope does not add creator-role transfer or renunciation.

[ADR 0033](0033-fixed-price-lbp-recorded-collateral.md) supersedes the original physical collateral-empty activation rule. Unsolicited collateral after `PoolPrepared` remains surplus and does not block activation. The owner claim still blocks an outsider from precreating the ATA with `Create`.

## Consequences

Fixed-price setup does not add buys, sells, weighted virtual accounting, fees, close, end payout, end hooks, delayed settlement, cancellation or expired-recovery handlers, private LBP, allowlists, or full SDK and CLI flows. Weighted LBP work remains separate.

Public curve-factory variants with tags 0 through 4 remain unchanged. Public fixed-price variants use tags 5 and later. SDK-only private variants move after all public variants. This deliberately changes private encoding. Clients and guest IDs need rebuild for the implemented encoding.

## Sources

- [Shared launch factory](0031-shared-launch-factory.md)
- [Namespaces and authority](0006-namespaces-and-authority.md)
- [Typed sale inputs](0007-typed-sale-inputs.md)
- [LBP clock and poke](0026-lbp-clock-and-poke.md)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
