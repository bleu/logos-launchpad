# ADR 0003: Direct source reuse and licenses

Status: Accepted

## Decision

Offer project-owned launchpad code under `MIT OR Apache-2.0`. The recipient may choose either license. Keep both root license texts and matching package metadata. This restores the licensing promised in the proposals. The independent weighted-math package retains its MIT license; this decision does not claim rights to relicense its contributors or third-party code.

Reuse code directly from reviewed public source snapshots without source ancestry or private tracker metadata. Do not merge, squash or subtree-import source repositories. Bleu owns its code, not third-party rights.

Keep required third-party licenses, copyright notices and modification notices in copied code. Record source revisions for copied fragments.

## Reason and limits

This repository owns maintenance and its own history. Public source references support review; an import does not prove production readiness.

## Sources

- [Curve source](https://github.com/bleu/lez-bonding-curve-launchpad)
- [Weighted-math source](https://github.com/bleu/lez-weighted-math)
