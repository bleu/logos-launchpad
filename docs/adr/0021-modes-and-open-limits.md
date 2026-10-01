# ADR 0021: Keep adopted modes and mark unresolved limits

Status: Accepted internally; To be reviewed by Logos team for the expiry-validation and allocation-ceiling departures; allowlist design open

Date: 2026-09-29

## Context

The proposals include optional sale modes. This design retains selected modes but does not complete all LBP limits or private allowlist design.

## Decision

This amendment replaces the earlier unspecified curve one-way mode with required immutable `TradeMode::{BuyOnly, BuyAndSell}`. BuyOnly permits both exact-input and exact-output collateral-input buys, and rejects selling. BuyAndSell also permits exact-input sells. There is no program default or exact-output sell; UI defaults remain deferred. Enforce the direction in the neutral account layer without adding factory policy to pure pricing state. These mode inputs are implemented and have host evidence mapped in the [curve contract](../product/curve-contract.md); final release checks remain separate.

Retain optional curve expiry. Curve closing follows [ADR 0025](0025-sale-closing.md): `D` exhaustion closes immediately, while an optional configured expiry closes at `D` exhaustion or its end timestamp. The contract does not validate a minimum sale duration when expiry is configured. This is an accepted internal departure from RFP-015 with external status `To be reviewed by Logos team`. No collateral-raised target closes a curve sale.

Retain weighted and fixed-price LBP modes, start/end times, clock-derived weights, permissionless poke and pause controls.

Defer the optional LBP per-block token-allocation ceiling. This is an accepted internal departure from the LBP RFP with external status `To be reviewed by Logos team`. Do not silently drop it from scope or claim it is implemented. LBP allowlist support remains required, but private LBP and ZK allowlist design stay open. DistributionX is a reference to validate, not an approved design; no final proof inputs, nullifier policy or cost claim is established here.

## Consequences

Mode-specific behavior needs specifications and executable tests before delivery. Deferral is not external approval to remove a requirement. The proposal's allowlist sketch remains an input for review, not an accepted cryptographic design.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
