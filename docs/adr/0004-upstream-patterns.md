# ADR 0004: Adopt AMM patterns without an AMM fork

Status: Accepted

Date: 2026-09-29

## Context

The platform AMM is a useful reference for token custody, program calls and module packaging. Its constant-product implementation does not supply the full launchpad product or SDK private workflow.

## Decision

Continue the launchpad PoCs as this codebase. Adopt applicable patterns from `lez-programs/programs/amm`: LP-0015 tail-call settlement, LP-0014 associated token accounts, shared core with thin interfaces, and separate `.lgx` modules. Do not import the full AMM repository as a fork.

For each copied source fragment, record its upstream revision and retain required notices. For patterns alone, cite the reference and verify the actual target-platform API before implementation.

## Consequences

This repository owns launchpad behavior and maintenance. Shared platform conventions reduce unnecessary differences, but do not prove custody, administration, event or package compatibility. Those checks need executable evidence on the selected LEZ version. AMM behavior must not be presented as an implemented SDK privacy guarantee.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
