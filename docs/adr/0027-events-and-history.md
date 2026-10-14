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

The [historical event experiment](https://github.com/bleu/logos-launchpad/blob/f1198315569cfc8ada9f6c39964cb77314e2520c/packages/platform-probes/events/tests/event_roundtrip.rs) checked candidate LEZ `c31a6bca60537ec08f595cf786f1ae6275ab7f43`. Emission preserved the emitter, selector and payload. A later chained failure discarded earlier events; metered failure consumed the signer's nonce and replay was rejected. The sample decoder rejected unknown versions and truncated payloads. Its version byte was an application choice, not a platform compatibility guarantee.

Root adoption remains blocked. The current launchpad pin has no event output. Scaffold `f919fd03b064a91a4f261093cf2b0b4f8150e27c`, using SPEL CLI `0cb7e0980535af619482cf1c823f4d394b3ebd61`, encoded `collect-fees` as `08000000`; the candidate expects `08`. Submission also lacks candidate shard selectors. Program and client migration need separate verification.

The experiment is no longer maintained. Results used development receipts, not production proofs or deployed nodes. Private event delivery and deployed indexer ingestion remain unverified. The inspected candidate indexer captured no events by default and limited range queries to 1,000 blocks; neither establishes an emission limit. No event byte/count limit or stable cross-version payload guarantee was established. Do not claim full LP-0012 compliance.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
