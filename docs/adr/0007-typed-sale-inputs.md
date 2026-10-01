# ADR 0007: Typed immutable sale inputs

Status: Accepted internally; To be reviewed by Logos team

Date: 2026-09-29

## Context

Curve and LBP creation inputs express different economic rules. One untyped configuration would hide those differences and make post-creation changes difficult to reason about.

## Decision

Use separate typed creation inputs for each mechanism. Validate them before creating a sale and keep the accepted sale configuration immutable. Runtime reserves, accrued fees and lifecycle state still change through permitted instructions.

Curve inputs include the purchasable `D` allocation, required immutable `TradeMode::{BuyOnly, BuyAndSell}`, and optional immutable adapter program and opaque configuration. There is no program default for the mode, no configuration without an adapter program, and no post-creation setter. Resumed creation cannot change saved inputs. Mode and adapter inputs are implemented; the [curve contract](../product/curve-contract.md) maps verified host, full guest-suite and generated-IDL checks, with their limits under amended [ADR 0020](0020-settlement-and-graduation.md). Curve inputs may include an expiry end timestamp. The contract does not validate a minimum sale duration when expiry is configured. This is an accepted internal departure from RFP-015 with external status `To be reviewed by Logos team`. Supply exhaustion can close a curve sale immediately. Closing and unsold-`D` rules are defined in [ADR 0025](0025-sale-closing.md).

Validate mechanism-specific inputs against their qualified math envelope. This includes applicable weight and amount limits plus dust and oversized-trade guards. Do not invent thresholds before their mechanism specifications establish them. LBP allowlist support remains required, but its proof design is open.

Immutable sale inputs do not freeze namespace administration. The treasury and applicable live fee settings belong to namespace configuration. Keep that distinction visible in the program, SDK, on-chain program IDL, module `.lidl` contract and product forms. LBP live-rate behavior is accepted internally, with external status `To be reviewed by Logos team` in [ADR 0016](0016-lbp-live-fees.md).

## Consequences

Invalid combinations should fail at the creation boundary. A client cannot treat mutable trading state as creation configuration or silently modify a sale's initial terms. Detailed input bounds belong in the mechanism specifications; this record does not invent fields or ranges absent from those specifications.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
- [Curve live configuration decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0003-admin-config-and-the-rfp-001-seam.md)
