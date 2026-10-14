# ADR 0005: Keep mechanisms, pools and factory policies separate

Status: Accepted

Date: 2026-09-29

## Context

Bonding-curve, fixed-price, and weighted sales have different pricing, state, controls and fee behavior. The curve PoC already separates a neutral curve pool from its launch factory. Each mechanism needs its own factory and pool boundary without forcing a shared state or lifecycle model.

## Partial supersession

[ADR 0031](0031-shared-launch-factory.md) partially supersedes this record's separate-factory requirement for matching new-token creation, allocation, namespace binding and setup checks. The independent pool, pricing, custody, lifecycle and fee boundaries in this record remain in force.

## Amendment

This amendment supersedes this record's earlier generic fixed-price and weighted-sale boundary wording. It makes the bonding-curve factory and pool, and separate fixed-price and weighted-sale factories and pools, explicit. It retains the existing bonding-curve policy scope and does not assign new fixed-price or weighted-sale permissions.

## Decision

Retain the curve's ordered token-pair pool, pure math, handlers and factory boundaries. The curve pool owns pricing, real-reserve solvency, trading, custody and neutral lifecycle operations. The curve factory owns launch supply, allocations, creator rights, closure and settlement policy. Product curve sales use the curve factory; direct curve pools remain internal primitives and test surfaces.

Keep fixed-price and weighted-sale factories and pools separate from the bonding-curve factory and pool. Each sale factory owns its sale-specific creation validation, sale-to-namespace binding and policy. Each sale pool owns its runtime trading, custody and state rules. This decision does not assign fixed-price or weighted-sale upgrade, withdrawal or pause permissions.

Reuse libraries, platform adapters, serialization and errors only where their behavior matches. Do not force all three sale types into a generic pool model or copy launch vocabulary into neutral curve APIs. Namespace state is logical per-namespace configuration used by the factories and applicable runtime checks. This decision does not select a namespace registry, shared physical schema, cross-factory account owner, deployment layout or account layout.

## Consequences

Shared tooling can cover all three sale types without making their state interchangeable. Factory changes need not change pure curve math. Client interfaces can expose mechanism-specific operations rather than a misleading uniform contract. Curve staged creation, pending guards and factory custody remain curve-specific until separate fixed-price or weighted-sale design evidence adopts comparable rules.

## Sources

- [Curve pool boundary](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0006-neutral-bounded-pool-interface.md)
- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
