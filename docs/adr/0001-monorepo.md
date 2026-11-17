# ADR 0001: One maintained monorepo

Status: Accepted

## Decision

Maintain all three sale types, SDK, shared core, CLI, QML packages and integration checks in this repository. Keep useful curve package boundaries and weighted math as a separate package.

## Reason and limits

One repository keeps related changes and review evidence together. It does not require one Cargo workspace, one deployable artifact or shared pool state. Preserve guest and math build boundaries. Each component owns its tests and interfaces. Repository placement does not prove implementation or production readiness.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
