# ADR 0001: One maintained monorepo

Status: Accepted

Date: 2026-09-29

## Context

The curve program and weighted-math PoCs are separate public sources. The three sale types need common developer tools, a shared core bridge and product packages. Maintaining separate bonding-curve implementations would split fixes and review evidence.

## Decision

Use this repository as the maintained repository for all three sale types, SDK/core bridge, CLI, QML packages and integration checks. Retain useful curve package boundaries. Keep weighted math as a separate package inside this repository.

Repository ownership does not require one Cargo workspace, one deployable artifact or a shared pool state model. Preserve guest and math build boundaries. A later import copies code directly from identified reviewed source snapshots and retains required licenses and copyright notices.

## Consequences

Cross-component changes can be reviewed together. Each component still owns its tests and interfaces. This decision creates no working programs. Old repositories remain independent public sources.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
