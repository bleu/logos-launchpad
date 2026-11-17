# ADR 0003: Direct source reuse and licenses

Status: Accepted

## Decision

Reuse code directly from reviewed public source snapshots without source ancestry or private tracker metadata. Do not merge, squash or subtree-import source repositories. Bleu owns its code, not third-party rights.

Keep required third-party licenses, copyright notices and modification notices in copied code. Record source revisions for copied fragments.

## Reason and limits

This repository owns maintenance and its own history. Public source references support review; an import does not prove production readiness.

## Sources

- [Curve source](https://github.com/bleu/lez-bonding-curve-launchpad)
- [Weighted-math source](https://github.com/bleu/lez-weighted-math)
