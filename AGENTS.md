# Agent instructions

Read [CONTEXT.md](CONTEXT.md) and the relevant [ADRs](docs/adr/README.md) before work. Use their domain terms. Keep pending decisions distinct from accepted ones, and accepted designs distinct from implemented behavior.

This repository contains an imported PoC baseline and design documentation. The three sale types are fixed-price sale, bonding-curve sale and weighted sale. Public setup, buys, fee collection and settlement use the actor interface. Fixed-price and weighted setup have four stages. Public buy eligibility, collateral namespace fees, pause/resume and ending decisions are shared under ADR 0031. All factory engines select immutable `BuyOnly` or `BuyAndSell`; allowlists gate buys only. Keep direct curve `allowed_input_token` separate. Selected depletion is final; `Scheduled` can replenish empty inventory through sells. Weighted trades protect seed, fees and surplus under ADR 0030. Actor and SDK quotes share pricing, but SDK time is advisory. See ADRs 0021 and 0025 for direction and ending rules. Retained creator fees remain future work. Shared settlement route rules, duplicate-payout protection, cross-engine graduation and common program events are implemented. See the [workspace guide](docs/architecture/workspace-guide.md#execution-and-evidence-limits) for current checks and limits. The shared core and QML apps remain status placeholders. Do not scaffold replacements for working PoC code. Preserve required third-party licenses and copyright notices. Do not copy private tracker metadata from a PoC. Public documentation must not cite private tickets.

Keep mechanism-specific pricing, state and lifecycle code separate. Share helpers only when behavior matches. Weighted math is an independent, unaudited package. GUI and CLI code must not duplicate protocol logic.

Use ASD-STE100 Simplified Technical English for all documentation. Use short sentences and simple words; use `CONTEXT.md` terms where technical words are needed. Keep overview pages brief. Put decisions and their reasons in ADRs, definitions in the glossary, and flows in sequence diagrams. Link to those pages instead of repeating them. Answer one-off questions in the conversation; do not turn every explanation or requested change into permanent prose.

## Agent skills

### Issue tracker

Use the [issue-tracker guide](docs/agents/issue-tracker.md) for Linear setup. Do not repeat tracker details in public documentation.

### Triage labels

Use the five default triage labels. See [docs/agents/triage-labels.md](docs/agents/triage-labels.md).

### Domain docs

Read root `CONTEXT.md` and relevant ADRs. See [docs/agents/domain.md](docs/agents/domain.md).
