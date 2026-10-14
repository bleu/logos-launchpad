# ADR 0017: Fresh quotes and explicit slippage limits

Status: Accepted

Date: 2026-09-29

## Decision

Integrations must obtain fresh quotes and set limits from the participant's selected slippage tolerance. Never silently use an unrestricted trade. The program checks limits against execution-time pricing because reserves, fees and weighted-sale weights can change after a quote.

For curves, exact-input `min_amount_out` limits the participant's receipt after any sell fee. Exact-output buy `max_amount_in` caps the full collateral debit, including fees. SDK quotes and public execution must enforce the same [trade mode](0021-modes-and-open-limits.md#sale-modes).

The UI must show the tolerance and handle rejected limits without promising the quoted output. Test stale quotes and rejected limits in the SDK, CLI and app. See [curve evidence](../product/curve-contract.md#acceptance-evidence).

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [Reviewed curve context](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/CONTEXT.md)
