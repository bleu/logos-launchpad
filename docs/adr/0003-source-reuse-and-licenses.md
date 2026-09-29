# ADR 0003: Direct source reuse and licenses

Status: Accepted

Date: 2026-09-29

## Context

Later implementation needs source provenance without importing repository history or private tracker metadata. Copied code must retain its existing required notices.

## Decision

Copy code directly from reviewed source snapshots. The import pull request must identify each source repository, revision and local change. Do not merge, squash, subtree-import or preserve source ancestry.

Keep the required licenses, copyright notices and modification notices already present in copied code. The import pull request provides source provenance; no separate license audit or review is required. This record does not assert ownership of third-party rights.

## Consequences

The maintained repository records its own history. Source repositories remain public references. This ADR does not copy code or establish a production dependency.

## Sources

- [Curve source](https://github.com/bleu/lez-bonding-curve-launchpad)
- [Weighted-math source](https://github.com/bleu/lez-weighted-math)
