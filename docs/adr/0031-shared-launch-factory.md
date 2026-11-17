# ADR 0031: Shared launch setup and reusable pool policies

Status: Accepted; shared policies and public paths implemented; private routes and readiness pending.

## Decision

### Factory and mechanism boundaries

Use the launch factory for common new-token creation, allocation, namespace binding and setup validation when rules match. Keep typed bonding-curve, fixed-price and weighted inputs separate. Namespace state is logical per-namespace configuration; this does not select a registry, shared physical schema, cross-factory account owner or deployment layout.

Keep each engine's pricing, pool state, custody and lifecycle effects separate. The neutral ordered-pair curve pool owns pricing, real-reserve solvency, trading, custody and neutral lifecycle operations. The curve factory owns launch supply, allocations, creator rights and settlement policy. Product curve sales use the factory; direct curve pools are internal primitives and test surfaces. Keep launch vocabulary outside pure curve math and neutral APIs.

Reuse libraries, platform adapters, serialization and errors only where behavior matches. Do not force engines into a generic pool-state model. Curve staged creation, pending guards and factory custody do not define fixed-price or weighted setup. Each engine must enforce its own backing and account checks.

### Shared setup

Fixed-price and weighted setup use four stages: claim the exact mint and creator role; split sale inventory and creator allocation; prepare the pool and vaults; then fund and activate. Use a setup record with an explicit sale mode and separate typed messages. [ADR 0032](0032-fixed-price-lbp-setup.md) defines fixed-price setup.

The final weighted stage transfers the full saved collateral seed from the current creator and inventory from factory custody. A vault donation cannot replace the seed. Check creator balance and vault capacity before transfer. Activate only with backed inventory and seed. Pay an `AfterSetup` allocation in the same transaction; keep `AfterEnd` allocation in escrow. Any failed operation rolls back all effects. [ADR 0026](0026-lbp-clock-and-poke.md) defines weighted start and weights; the factory does not share weighted pricing with fixed-price pools.

## Policy contract

Treat the calculations as pricing engines. Share ending, fee and optional eligibility policies through small typed configurations, not a runtime plugin system. `crates/sale-policy` owns common decisions; each caller owns custody accounting. Creation saves supported choices. Pool entry, setup and resumed clients reject substitutions. Policies must hold at the pool boundary even when callers bypass the factory or SDK. Direct pools can use explicit open defaults.

Separate permission to trade from permission to settle. Preserve authority, freeze, inclusion-time bounds, allocations, seed backing, vault coverage and retries. Shared [ending rules](0025-sale-closing.md) keep inclusive starts and exclusive ends. Selected depletion is final and needs no scheduled-end payout bound; `Scheduled` ignores depletion for payouts. Empty inventory blocks buys. Settlement payouts cannot create depletion evidence. Direct curve `Manual` pools retain owner close rights and their supported depletion-side choices.

All engines use collateral namespace fees and immutable factory trade modes. See [fees](0015-fee-accounting.md), [modes](0021-modes-and-open-limits.md), [weighted accounting](0030-lbp-pricing-collateral.md) and [settlement routes](0020-settlement-and-graduation.md). New policy choices require explicit accounting rules. All engines save the shared immutable graduation policy at creation.

The shared buy allowlist uses `crates/eligibility-policy`, pre-approved public account batches and repeated authorized purchases. Setup preserves the initial policy. [Public execution evidence](../architecture/allowlist-proof.md#public-execution-evidence) does not supply private holdings or complete private routes; weighted tests use a mock clock.

Creation fixes `PausePolicy` as disabled or authority-controlled. The current creator controls factory sales; the current owner controls direct pools. Authenticate every call, including retries. Pause blocks swaps but permits fees and safe settlement. Resume cannot move the schedule, clear emergency freeze or reopen an ended sale. Authorized unchanged controls emit no change event.

## Program events

`SaleEventV1` identifies namespace, pricing engine and pool or launch. Emit public changes for setup, trades, controls, fees, allocation, closure, settlement and graduation. Source declarations define generated IDL payloads and selectors. Weighted legacy selectors and payload bytes remain unchanged. Changed shared payloads require updated decoders; do not claim old wire compatibility.

Record closure once through a successful pool operation, with reason and inclusion bounds. Time passing alone emits nothing. Allocation reads do not record closure. Fee events distinguish discharged liability from tokens transferred. Authenticated no-ops emit no change event. Failed transactions commit neither changes nor events. Indexer delivery and history are outside this contract; see [ADR 0027](0027-events-and-history.md).

## Reason and limits

Shared decisions reduce policy drift without making pricing state or custody interchangeable. Test through each engine's interface. Signed guest tests must check token effects, rollback and retries; callback success does not prove DEX liquidity. See [execution limits](../architecture/workspace-guide.md#execution-and-evidence-limits). Private routes, real proofs and deployment readiness remain pending.

## Sources

- [Neutral curve pool boundary](https://github.com/bleu/lez-bonding-curve-launchpad/blob/778f92169ac08edf3cff02bbf2e45d770f176283/docs/adr/0006-neutral-bounded-pool-interface.md)
- [Typed sale inputs](0007-typed-sale-inputs.md)
- [Fixed-price and weighted sale proposal](https://github.com/logos-co/rfp/issues/119)
