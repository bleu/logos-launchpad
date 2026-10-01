# ADR 0003: Direct source reuse and licenses

Status: Accepted

Date: 2026-09-29

## Context

The imported PoC baseline needs a clean source import without repository history or private tracker metadata. Imported code must retain its required third-party notices.

## Decision

Copy code directly from reviewed source snapshots in a clean import without source history. Do not merge, squash, subtree-import or preserve source ancestry. Bleu owns its code.

Keep the required third-party licenses, copyright notices and modification notices already present in copied code. This ADR does not assert ownership of third-party rights.

## Consequences

The maintained repository records its own history. Source repositories remain public historical references. The imported baseline does not prove production readiness.

## Sources

- [Curve source](https://github.com/bleu/lez-bonding-curve-launchpad)
- [Weighted-math source](https://github.com/bleu/lez-weighted-math)
