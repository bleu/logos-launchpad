# Plan: M1-M4 launchpad re-plan

> Source PRD: [product goals and use cases](../docs/product/prd.md)

Status: M1 contract reviewed; implementation and evidence remain subject to each phase's acceptance criteria.

## Architectural decisions

- **Mechanisms**: Keep curve and LBP state, pricing, custody, and lifecycle code separate. The curve baseline remains the starting point. The LBP factory and pool are new mechanism-specific components.
- **Public interfaces**: On-chain program IDLs define transaction interfaces. M1 owns program handlers, source IDLs, generated artifacts and local guest evidence. It updates existing callers when interfaces change. M2 owns complete SDK and CLI journeys. Interfaces can change during development; see [ADR 0022](../docs/adr/0022-verification-boundaries.md#milestone-and-interface-boundary).
- **Product packages**: Keep separate admin and configured-sales apps. The non-UI shared core is the QML boundary. QML reads listings, history, and monitoring through a backend module read API.
- **Emergency controls**: Each namespace has its own emergency authority and freeze state. Freeze stops curve swaps and LBP buys only. See [ADR 0028](../docs/adr/0028-token-custody-and-freeze.md#namespace-emergency-control).
- **Public first**: Deliver public flows before private flows. Private trading remains gated on platform and wallet evidence. The allowlist proof remains separate follow-up work.
- **Events and history**: History and creator monitoring remain required. Do not claim they work until event emission, privacy review, indexer capture, and namespace-scoped reads are verified.
- **External review**: Keep the accepted internal RFP departures visible as pending Logos-team review. Do not silently change the design back to an RFP baseline or claim external approval.

## M1 scope

M1 covers the public curve and LBP programs in Phases 1–3. It includes program handlers, on-chain program IDLs, generated artifacts, handler checks and local guest evidence. Keep existing callers working when an interface changes. Complete SDK and CLI journeys belong to M2, including fresh quote workflows.

Private routes, allowlist proofs, event history, product apps, live deployment and production readiness are outside M1. Existing public recovery operations remain available during a namespace freeze. This does not add private-route recovery to M1. Local evidence does not prove a working product journey or production readiness.

---

## Phase 1: Preserve and complete the public curve baseline

**Milestone**: M1 — programs and contracts.

**User stories**: A namespace administrator configures a namespace. A creator creates and settles a curve sale. A participant performs allowed public curve trades. An operator collects curve fees.

### What to build

Review and integrate existing curve and namespace-control work without rebuilding the imported PoC. Stabilize the public curve contract on the pinned platform stack. Keep namespace administration, creator authority, custody, fee collection, closing, and settlement consistent across handlers and on-chain interfaces. Update generated artifacts and existing callers when these interfaces change.

### Acceptance criteria

- [ ] Namespace fee, treasury, administration transfer, and renunciation actions use the accepted separate-role rules.
- [ ] Public curve creation, allowed trade directions, explicit limits, fee collection, closing, and settlement have matching handler, IDL and local guest checks. Generated artifacts and existing callers stay consistent. Complete SDK and CLI journeys remain in M2.
- [ ] Namespace emergency controls follow [ADR 0028](../docs/adr/0028-token-custody-and-freeze.md#namespace-emergency-control). Checks cover isolation, authority, immediate transfer, repeated calls and permitted operations during freeze.
- [ ] Existing namespace-controls work is reviewed and integrated without duplicate implementation or scope.
- [ ] Curve settlement retains idempotent stages and reports that graduation is not proof of DEX liquidity.
- [ ] Evidence records state platform, wallet, event and live-DEX limits. M1 does not claim product journeys or production readiness.

---

## Phase 2: Deliver a fixed-price public LBP slice

**Milestone**: M1 — programs and contracts.

**User stories**: A creator creates and funds a fixed-price LBP. A participant makes a public buy with an explicit limit. A creator receives permitted proceeds after the scheduled end. Any caller collects accrued fees.

### What to build

Build the smallest complete LBP path. It includes LBP-specific creation validation, namespace binding, fixed-price exact-input and exact-output buys, custody, creator-borne fees, scheduled close, collection, and post-close proceeds. Use the weighted-math package only where the mechanism needs it; do not put LBP policy in that package.

### Acceptance criteria

- [ ] An LBP sale is bound to its creation namespace and validates its immutable fixed-price inputs and schedule.
- [ ] Exact-input and exact-output public buys enforce explicit limits and reject invalid or oversized buys in full.
- [ ] Full buyer collateral is used for pricing. Accrued fees and vault collateral remain separate from pricing collateral.
- [ ] Fee collection is full, idempotent, paid to the current treasury, and does not change buyer pricing.
- [ ] The sale closes at its original end timestamp. Creator proceeds stay unavailable before that end.
- [ ] Handler and local guest checks cover authorization, custody, rounding, failed calls, retries, zero fees, and repeated collection. Namespace freeze rejects buys and leaves collection, settlement and public recovery available under their normal checks.

---

## Phase 3: Add weighted LBP and public lifecycle controls

**Milestone**: M1 — programs and contracts.

**User stories**: A creator creates a weighted LBP and pauses or resumes it. A participant buys at execution-time weights. Any caller records a checkpoint for observers. A creator settles after the scheduled end.

### What to build

Extend the LBP through its own pool and factory. Add trusted-clock weight calculation, permissionless checkpoints, creator pause/resume, and weighted buy behavior. Keep the schedule independent from pause state. Keep the fixed-price path intact and do not introduce curve allocation or settlement rules into LBP code.

### Acceptance criteria

- [ ] Weighted buys calculate weights from trusted chain time and current reserves, with or without an earlier checkpoint.
- [ ] A successful buy updates its timestamp-keyed checkpoint only when required; failed buys update no checkpoint or trade state.
- [ ] Pause stops buys but does not stop weights or move the scheduled end. Resume cannot reopen an ended sale.
- [ ] The LBP public lifecycle enforces custody, creator-borne fees, collection, and post-close proceeds under the same rules as the fixed-price slice.
- [ ] Numerical, state-machine, authorization, and failure-atomicity tests cover both LBP modes.
- [ ] Namespace freeze rejects buys in both modes. Poke, fee collection, settlement and public recovery remain available under their normal checks. Freeze does not stop weights or move the scheduled end. Creator resume cannot bypass namespace freeze.

---

## Phase 4: Deliver public SDK and CLI journeys

**Milestone**: M2 — SDK and developer interface.

**User stories**: Administrators, creators, participants, and operators complete supported public curve and LBP actions through the SDK or CLI.

### What to build

Expose the completed public contracts through shared SDK operations and thin CLI commands. Each operation reads current state, builds one transaction, reports expected and actual amounts, and classifies failures. Add single and namespace-batched fee collection only when each sale's current eligibility can be checked without changing the per-sale result.

### Acceptance criteria

- [ ] SDK and CLI use generated program interfaces and preserve quote, slippage, authority, custody, and lifecycle errors.
- [ ] Quotes state the full collateral amount, expected output, fee treatment, and explicit participant limit.
- [ ] LBP creation shows creator-borne fee treatment before approval and actual fees and net proceeds after operations.
- [ ] Single and batched fee collection use the current treasury and correctly report skipped or rejected sales.
- [ ] SDK and CLI tests cover stale quotes, wrong namespace, wrong authority, invalid state, and retry-safe settlement or collection.

---

## Phase 5: Deliver public product applications

**Milestone**: M3 — product surface.

**User stories**: A namespace administrator uses the admin app. A creator configures and monitors supported public sales. A participant finds a sale, obtains a quote, and completes a public trade.

### What to build

Replace the package smoke screens with public workflows that call the shared core. Keep administration in the admin app and creator/participant workflows in the configured-sales app. Show clear confirmations and actionable failure states. Do not add private trading, allowlist purchase, direct indexer HTTP calls, or an analytics product.

### Acceptance criteria

- [ ] The admin app performs only namespace administration and clearly separates administrator authority from creator authority.
- [ ] The configured-sales app performs supported public curve and LBP workflows through the shared core.
- [ ] Creation, trade, collection, and settlement confirmations show sale state, amounts, fee treatment, and next allowed action.
- [ ] Failure states explain whether a caller, account, sale state, amount, quote limit, or custody rule caused the rejection.
- [ ] Core and both app packages build, install, load, and exercise their supported workflows in the target Basecamp environment.

---

## Phase 6: Prove history and monitoring

**Milestone**: M3, after a separate platform and event compatibility decision.

**User stories**: A creator monitors a sale. An administrator views history for a namespace. A participant views available sale history without exposing private participant material.

### What to build

Verify a compatible event path on the selected platform before building product claims on it. Then publish the required successful-action events, capture them in the existing indexer, and expose namespace-scoped listings, history, and monitoring through the backend module read API.

If the selected platform cannot provide this path, record the compatibility blocker and decide a different solution as separate work. Do not silently remove history or creator monitoring from the product.

### Acceptance criteria

- [ ] The selected platform/client stack emits and decodes the required event schemas with documented compatibility behavior.
- [ ] Events and indexed reads contain no private account identifiers, private witnesses, or other protected material.
- [ ] The existing indexer captures the required events and supports namespace-scoped listings, purchase history, and creator monitoring.
- [ ] Apps show indexed-data freshness limits and do not use indexed state to authorize a transaction.
- [ ] The derived analytics view remains excluded.
- [ ] An incompatible platform path leaves an explicit blocker and does not remove the required product capability.

---

## Phase 7: Harden and gate deployment

**Milestone**: M4 — hardening and gated deployment.

**User stories**: Operators can verify release evidence. Reviewers can assess custody, state transitions, numerical limits, and security before deployment.

### What to build

Add cross-component and live-environment evidence to the feature-level checks. Prepare audit inputs and deployment documentation. Treat platform readiness and audit completion as gates, not as date-based assumptions.

### Acceptance criteria

- [ ] System tests cover public curve and LBP lifecycles, namespace isolation, custody, collection, settlement, and all implemented modes.
- [ ] Numerical and state-machine tests cover the LBP's qualified input envelope and failures.
- [ ] Deployment evidence records the exact platform and tool versions, test conditions, and limits.
- [ ] Audit material describes the threat model, trusted boundaries, known limits, and evidence for each implemented feature.
- [ ] Deployment remains blocked until platform readiness, required compatibility evidence, and audit gates are satisfied.

---

## Deferred gated work

Private trading, allowlisted LBP purchases, and history/monitoring are not removed from the product. They need separate scope and evidence before implementation begins.

- **Private trading**: Start only after the platform and wallet can prove the required one-transaction route, participant-controlled returns, outcome recovery, and complete return accounting.
- **Allowlisted LBP purchases**: Start only after a proof construction is selected and reviewed for cost, confidentiality, verifier integration, sale-scoped replay protection, and failed-purchase behavior.
- **History and monitoring**: Start product delivery only after Phase 6 proves the event and indexer path.
