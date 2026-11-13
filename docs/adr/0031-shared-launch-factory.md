# ADR 0031: Shared launch setup and reusable pool policies

Status: Shared setup, public buy eligibility, fee arithmetic, optional pause/resume, supported fee/ending decisions, settlement route rules with duplicate-payout protection, cross-engine graduation and common program events implemented. Private routes, real proofs and deployment readiness pending.

Date: 2026-10-13; policy amendment: 2026-11-09; ending and namespace-fee amendment: 2026-11-13

## Context

Bonding-curve, fixed-price, and weighted sales all need new-token creation, allocation, namespace binding and setup checks. Those operations can use one launch factory when their rules match. Their pricing and custody rules do not match; fee and ending decisions can be shared.

This record partially supersedes [ADR 0005](0005-mechanism-boundaries.md). It first replaced the separate-factory requirement for common launch setup. The 2026-11-09 amendment also replaces the requirement to keep fee and ending policies tied to each mechanism. It does not make their pricing state, custody or settlement effects interchangeable.

## Decision

Use the existing launch factory for common new-token creation, allocation, namespace binding and setup validation when the rules are the same. Keep typed bonding-curve, fixed-price sale, and weighted-sale inputs separate.

The current programs keep bonding-curve, fixed-price and weighted pricing, custody and lifecycle implementations separate. Shared public buy eligibility is implemented under [ADR 0021](0021-modes-and-open-limits.md). `crates/sale-policy` shares namespace fee and ending decisions; callers retain custody accounting. Creation persists supported choices; pool entry, setup and resumed clients reject substitutions. Factory curve and fixed-price sales support `Scheduled`, `Depletion` and `Either`. Weighted sales support `Scheduled` and `Either`, with a required schedule end for interpolation and settlement fallback. Explicit `Manual` is direct-only. See [ADR 0025](0025-sale-closing.md). Namespace fees always use collateral buy input or sell output. Weighted buys now price net input and gross up exact output; [ADR 0030](0030-lbp-pricing-collateral.md) defines seed-inclusive accounting. All factory engines share immutable `BuyOnly` or `BuyAndSell` under [ADR 0021](0021-modes-and-open-limits.md). Retained creator fees remain future work.

Weighted creation uses four stages: mint the exact supply and create the creator role; split sale inventory and creator allocation; prepare the weighted pool and vaults; fund and activate the pool. Keep typed creation messages separate. Use one setup record with an explicit sale mode. The weighted-setup change kept the existing fixed-price message tags and state bytes unchanged. The later shared eligibility change adds policy fields and changes the state encoding.

The final weighted stage transfers the full collateral seed from the current creator authority. It also transfers the sale inventory from factory custody. A prior vault donation does not replace the seed. Check the seed balance and vault capacity before transfer. Activate only when inventory and seed are backed. Release an `AfterSetup` allocation in the same transaction. Keep an `AfterEnd` allocation in escrow. All effects must roll back if any operation fails.

Weighted weights and start rules are in [ADR 0026](0026-lbp-clock-and-poke.md). The factory does not share weighted pool pricing or buy rules with fixed-price pools.

## Pool-policy amendment

Treat bonding-curve, fixed-price and weighted calculations as pricing engines. Ending, fee and optional eligibility policies must be reusable across engines and available to any pool. Keep small typed configurations and validate supported combinations at creation; do not add a runtime plugin system. Neutral direct pools can use explicit open defaults. Configured policies must hold at the pool boundary, including calls that bypass the factory or SDK.

Separate permission to trade from permission to settle. Reuse policy checks without merging engine-specific reserve state, token custody or settlement effects. Preserve authority, freeze, inclusion-time bounds, allocation, seed backing, vault coverage and retry behavior. Shared ending decisions preserve inclusive starts and exclusive ends. Selected depletion is final and needs no scheduled-end payout bound. `Scheduled` ignores depletion; empty buys remain unavailable without early payouts. Settlement burns cannot supply new depletion evidence. Direct curve pools retain owner close rights under explicit `Manual` and their supported depletion-side choices.

The allowlist is a shared buy-admission policy, with pre-approved public trading accounts and unlimited repeated purchases under [ADR 0021](0021-modes-and-open-limits.md). All three engines use the maintained `crates/eligibility-policy` verifier and preserve the initial policy through staged setup. The [integration evidence](../../plans/allowlist-program-integration.md#public-execution-evidence) covers signed public buys; weighted tests use a mock clock and normal weighted buys remain unsupported. Private accounts remain planned funding and return endpoints. Policy reuse does not supply private token holdings or a complete private route.

## Policy contract

Use the shared ending and namespace-fee rules above; preserve payouts, live namespace settings and buy-only allowlist admission. See [settlement](0020-settlement-and-graduation.md) and [sale closing](0025-sale-closing.md). New policy choices require explicit accounting rules; neither fixed-price nor weighted sales has a curve `R` reserve.

Optional pause/resume is implemented through `PausePolicy` and `TradeControl`. Creation fixes pause as disabled or authority-controlled. The current creator controls factory sales; the current owner controls direct pools. Authenticate every call, including retries. Pause blocks swaps, but permits fee collection and safe settlement. Resume must not move the schedule or clear emergency freeze. Controls reject ended sales under the selected ending policy.

Test shared policies through each engine's interface. Signed guest tests must check token effects, rollback and retries. Callback success does not prove DEX liquidity.

## Program events

`SaleEventV1` identifies the namespace, pricing engine and pool or launch. Programs emit public changes for setup, trades, controls, fees, allocation, closure, settlement and graduation. Source declarations define payloads and selectors in the generated IDL. Weighted legacy selectors and payload bytes are unchanged. Changed shared event payloads require updated decoders; they are not wire-compatible with earlier payloads.

Closure is recorded once by a successful pool operation, with its reason and inclusion bounds. Time passing alone emits nothing. Allocation reads do not record closure. Fee events distinguish discharged liability from tokens transferred. Authenticated no-ops emit no change event. Failed transactions commit neither changes nor events. Indexer delivery and history remain outside this contract.

## Consequences

A common factory can create and prepare sales without making sale state interchangeable. Ending choices now control trading and payout eligibility; engine-specific settlement effects remain separate. Fixed-price sale creation and funding rules are in [ADR 0032](0032-fixed-price-lbp-setup.md). Shared trade modes permit exact-input sells without changing allocation or seed obligations. Retained creator fees remain future work.

## Sources

- [Mechanism boundaries](0005-mechanism-boundaries.md)
- [Typed sale inputs](0007-typed-sale-inputs.md)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
