# Agent instructions

Read [CONTEXT.md](CONTEXT.md) and the relevant [ADRs](docs/adr/README.md) before work. Use their domain terms. Keep pending decisions distinct from accepted ones, and accepted designs distinct from implemented behavior.

This repository contains an imported PoC baseline and design documentation. The three sale types are fixed-price sale, bonding-curve sale and weighted sale. Public setup, buys, fee collection and settlement use the actor interface. Fixed-price and weighted setup have four stages. Public buy eligibility, collateral namespace fees, optional pause/resume and ending decisions are shared under ADR 0031. `Manual` is direct-only; factory curve and fixed-price sales support `Scheduled`, `Depletion` and `Either`; weighted sales support `Scheduled` and `Either` with a required schedule end. Selected depletion is final; `Scheduled` ignores it for payouts, and settlement burns are not depletion evidence. Weighted buys price net input and gross up exact output; pricing collateral equals seed-inclusive recorded collateral during trading, with vault coverage of recorded collateral plus accrued fees. Retained fees and new sells remain future work. Shared settlement route rules, duplicate-payout protection and cross-engine graduation are implemented; common events remain incomplete. See the [workspace guide](docs/architecture/workspace-guide.md#execution-and-evidence-limits) for current checks and limits. The shared core and QML apps remain status placeholders. Do not scaffold replacements for working PoC code. Preserve required third-party licenses and copyright notices. Do not copy private tracker metadata from a PoC. Public documentation must not cite private tickets.

Keep mechanism-specific pricing, state and lifecycle code separate. Share helpers only when behavior matches. Weighted math is an independent, unaudited package. GUI and CLI code must not duplicate protocol logic.

Use ASD-STE100 Simplified Technical English for all documentation. Use short sentences and simple words; use `CONTEXT.md` terms where technical words are needed. Keep overview pages brief. Put decisions and their reasons in ADRs, definitions in the glossary, and flows in sequence diagrams. Link to those pages instead of repeating them. Answer one-off questions in the conversation; do not turn every explanation or requested change into permanent prose.

## Agent skills

### Issue tracker

Use the [issue-tracker guide](docs/agents/issue-tracker.md) for Linear setup. Do not repeat tracker details in public documentation.

### Triage labels

Use the five default triage labels. See [docs/agents/triage-labels.md](docs/agents/triage-labels.md).

### Domain docs

Read root `CONTEXT.md` and relevant ADRs. See [docs/agents/domain.md](docs/agents/domain.md).
