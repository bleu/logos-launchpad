# ADR 0007: Typed immutable sale inputs

Status: Accepted internally; To be reviewed by Logos team

Date: 2026-09-29

## Decision

Use separate typed creation inputs for curve and LBP sales because they have different economic rules. Validate inputs before creation and keep them immutable, including when creation resumes. Reserves, fees and lifecycle state can change through permitted instructions.

Validate inputs against each mechanism's qualified math limits. Do not invent thresholds. Immutable sale inputs do not freeze namespace settings; see [namespace authority](0006-namespaces-and-authority.md) and [LBP live fees](0016-lbp-live-fees.md).

[Allocations](0008-curve-allocation-and-close.md), [adapter settings](0020-settlement-and-graduation.md), [modes and allowlists](0021-modes-and-open-limits.md) and [closing](0025-sale-closing.md) define the mechanism-specific decisions and open work.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
- [Curve live configuration decision](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0003-admin-config-and-the-rfp-001-seam.md)
