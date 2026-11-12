# ADR 0031: Shared launch setup and reusable pool policies

Status: Shared setup and shared public buy eligibility implemented with host and signed development-execution evidence. Broader ending and fee policy refactor not implemented. Private routes, real proofs and deployment readiness pending.

Date: 2026-10-13; policy amendment: 2026-11-09

## Context

Bonding-curve, fixed-price, and weighted sales all need new-token creation, allocation, namespace binding and setup checks. Those operations can use one launch factory when their rules match. Their pricing, custody, fees and lifecycle rules do not match.

This record partially supersedes [ADR 0005](0005-mechanism-boundaries.md). It first replaced the separate-factory requirement for common launch setup. The 2026-11-09 amendment also replaces the requirement to keep fee and ending policies tied to each mechanism. It does not make their pricing state, custody or settlement effects interchangeable.

## Decision

Use the existing launch factory for common new-token creation, allocation, namespace binding and setup validation when the rules are the same. Keep typed bonding-curve, fixed-price sale, and weighted-sale inputs separate.

The current programs keep bonding-curve, fixed-price and weighted pricing, custody, lifecycle and fee implementations separate. Shared public buy eligibility is implemented under [ADR 0021](0021-modes-and-open-limits.md); independently selectable ending and fee policies remain the target described below.

Weighted creation uses four stages: mint the exact supply and create the creator role; split sale inventory and creator allocation; prepare the weighted pool and vaults; fund and activate the pool. Keep typed creation messages separate. Use one setup record with an explicit sale mode. The weighted-setup change kept the existing fixed-price message tags and state bytes unchanged. The later shared eligibility change adds policy fields and changes the state encoding.

The final weighted stage transfers the full collateral seed from the current creator authority. It also transfers the sale inventory from factory custody. A prior vault donation does not replace the seed. Check the seed balance and vault capacity before transfer. Activate only when inventory and seed are backed. Release an `AfterSetup` allocation in the same transaction. Keep an `AfterEnd` allocation in escrow. All effects must roll back if any operation fails.

Weighted weights and start rules are in [ADR 0026](0026-lbp-clock-and-poke.md). The factory does not share weighted pool pricing or buy rules with fixed-price pools.

## Pool-policy amendment

Treat bonding-curve, fixed-price and weighted calculations as pricing engines. Ending, fee and optional eligibility policies must be reusable across engines and available to any pool. Keep small typed configurations and validate supported combinations at creation; do not add a runtime plugin system. Neutral direct pools can use explicit open defaults. Configured policies must hold at the pool boundary, including calls that bypass the factory or SDK.

Separate permission to trade from permission to settle. Reuse policy checks without merging engine-specific reserve state, token custody or settlement effects. Preserve existing fee incidence, rounding, authority, freeze, inclusion-time bounds, allocation and seed accounting, vault coverage and retry behavior during extraction. Selecting a different policy must not silently change existing sales. The broader ending/fee configuration matrix and program refactor remain work to do.

The allowlist is a shared buy-admission policy, with pre-approved public trading accounts and unlimited repeated purchases under [ADR 0021](0021-modes-and-open-limits.md). All three engines use the maintained `crates/eligibility-policy` verifier and preserve the initial policy through staged setup. The [integration evidence](../../plans/allowlist-program-integration.md#public-execution-evidence) covers signed public buys; weighted builds use the existing fake clock by default, with the limits in [ADR 0026](0026-lbp-clock-and-poke.md). Private accounts remain planned funding and return endpoints. Policy reuse does not supply private token holdings or a complete private route.

## Consequences

A common factory can create and prepare sales without making sale state interchangeable. Current bonding-curve close and settlement behavior remains unchanged until the policy refactor. Fixed-price sale creation and funding rules are in [ADR 0032](0032-fixed-price-lbp-setup.md). This amendment does not add sells or change allocation, seed or settlement payouts.

## Sources

- [Mechanism boundaries](0005-mechanism-boundaries.md)
- [Typed sale inputs](0007-typed-sale-inputs.md)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
