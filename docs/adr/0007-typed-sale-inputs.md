# ADR 0007: Typed immutable sale inputs

Status: Accepted internally; To be reviewed by Logos team

## Decision

Use separate typed creation inputs for bonding-curve, fixed-price and weighted sales. Validate them before creation and keep them immutable, including during resumed setup. Reserves, fees and lifecycle state can change only through permitted operations.

## Reason and limits

The engines have different economic rules. Validate against qualified math limits; do not invent thresholds. The fee rate, treasury and freeze remain live namespace settings for all sale types. See [authority](0006-namespaces-and-authority.md), [fees](0015-fee-accounting.md), [allocations](0008-curve-allocation-and-close.md), [settlement](0020-settlement-and-graduation.md), [modes](0021-modes-and-open-limits.md) and [ending](0025-sale-closing.md). External Logos review remains pending.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
- [Curve live configuration decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0003-admin-config-and-the-rfp-001-seam.md)
