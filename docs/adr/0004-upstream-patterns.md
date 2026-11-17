# ADR 0004: Adopt AMM patterns without an AMM fork

Status: Accepted

## Decision

Use applicable patterns from `lez-programs/programs/amm`: LP-0015 tail-call settlement, LP-0014 associated token accounts, shared core with thin interfaces, and separate `.lgx` modules. Do not fork the full AMM repository.

Record the upstream revision and required notices for copied fragments. For patterns alone, cite the reference and check the target-platform API before use.

## Reason and limits

The AMM provides useful platform patterns, but not the launchpad product or its SDK private route. Verify custody, authority, events and packaging on the selected LEZ revision. AMM behavior does not establish a launchpad privacy guarantee.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
