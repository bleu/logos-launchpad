# ADR 0027: Events and history

Status: Accepted internally; program emission implemented; failed-event retention: To be reviewed by Logos team; indexer delivery unverified.

## Decision

Publish versioned public program events for successful sale changes. `SaleEventV1` identifies namespace, pricing engine and pool or launch. Source declarations define payloads and selectors in generated IDLs. See [program event rules](0031-shared-launch-factory.md#program-events).

Use existing `lez-indexer-module` and block-explorer tooling; do not build a separate indexer. QML requests indexed listings, history and monitoring through a backend module read API, not direct indexer HTTP calls. The derived analytics view is excluded; history and monitoring remain in scope.

Do not add persistent private participant IDs to events. Pool state and trade effects remain public. Failed or reverted transactions commit no sale events. Discarding pre-failure events departs from LP-0012 retention and has status `To be reviewed by Logos team`.

## Reason and limits

Common schemas support shared consumers without merging sale state. Review payload compatibility and privacy; decoders must handle schema changes. Indexed state can lag and never authorizes a trade or replaces current-state checks.

Program emission is implemented. Indexer delivery, deployed ingestion, private delivery and complete history remain unverified. Local development execution is not a production proof. No stable cross-version payload guarantee or platform event byte/count limit is established. Do not claim full LP-0012 compliance. See [execution evidence](../architecture/workspace-guide.md#execution-and-evidence-limits).

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
