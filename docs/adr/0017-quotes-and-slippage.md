# ADR 0017: Fresh quotes and explicit slippage limits

Status: Accepted

Date: 2026-09-29

## Context

Reserves can change between a displayed quote and transaction execution. LBP weights also change with chain time. A quote is not a promise that execution will use the same state.

## Decision

Integrations must obtain a fresh quote and supply `minAmountOut` derived from the participant's selected slippage tolerance for purchase flows. Do not silently replace that bound with an unrestricted trade. The program checks the supplied limit against execution-time pricing.

For curves, retain exact-input buys and sells, and exact-output buys only. Exact-input `min_amount_out` is the participant's receipt, after any sell fee. Exact-output buy `max_amount_in` caps the full collateral debit including fees. Both use saved creation-time `k` and execution-time namespace fees. The immutable trade mode must restrict SDK quotes and public execution in the same way; both buy forms are valid in BuyOnly. See the [curve contract](../product/curve-contract.md) for the forward and inverse formulas, literal examples and evidence limits. This amendment makes the existing quote types explicit; it does not add exact-output sells.

## Consequences

SDK, CLI and app tests must cover stale quotes and rejected limits. The UI must show the selected tolerance and handle a failed trade without claiming the quoted output was guaranteed. Fresh quotes reduce surprises but cannot eliminate concurrent state changes.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Reviewed curve context](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/CONTEXT.md)
