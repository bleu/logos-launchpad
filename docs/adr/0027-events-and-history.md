# ADR 0027: Events and history

Status: Accepted

Date: 2026-09-29

## Context

Participants, creators and administrators need sale history and monitoring. The platform already provides `lez-indexer-module` and block-explorer tooling. The derived analytics view is outside this product scope.

## Decision

Publish the LP-0012 events needed by the existing `lez-indexer-module` and block-explorer tooling. Do not build a separate indexer. QML apps request indexed listings, history and monitoring through a backend module read API, not direct HTTP calls to the indexer.

Use those events to preserve successful transaction history and support monitoring. Events from failed or reverted transactions may be discarded. This is an accepted internal departure from LP-0012 pre-failure retention, with external status `To be reviewed by Logos team`. Do not add persistent private participant identifiers to events. Public pool state and public trade effects remain visible. Excluding the derived analytics view does not exclude event history or monitoring.

## Consequences

Event schemas require compatibility and privacy review. Product interfaces can consume existing indexed history through the backend module read API, but they must not claim that an excluded analytics view exists. Indexed state can lag and does not authorize a trade or replace a current-state check by a program.

The [platform probes](../../packages/platform-probes/README.md) verify emission and decoding on selected candidate `c31a6bca60537ec08f595cf786f1ae6275ab7f43`. The current launchpad pin has no event output and remains unchanged. The candidate drops events after a later chained failure, as permitted by this decision; do not claim full LP-0012 compliance. Root adoption still needs program and client migration. The pinned generic CLI emits the old instruction and transaction format. Private event delivery and deployed indexer ingestion remain unverified.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
