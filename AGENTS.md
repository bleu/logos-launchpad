# Agent instructions

Read [CONTEXT.md](CONTEXT.md) and the relevant [ADRs](docs/adr/README.md) before work. Use their domain terms. An accepted design is not proof of implementation or external approval. Check code and tests for current behavior. Use the [workspace guide](docs/architecture/workspace-guide.md) for checks and evidence limits.

## Code boundaries

Keep pricing-engine state, custody and lifecycle code separate. Share helpers only where behavior matches. Weighted math is an independent, unaudited package. GUI and CLI code must not copy protocol logic. Do not replace working code with scaffolding.

Preserve third-party licenses and copyright notices. Do not copy private tracker metadata into public docs or source comments.

## Documentation and comments

Use ASD-STE100 writing rules: short sentences, simple verbs and consistent terms. Use project terms from `CONTEXT.md` when needed. Do not claim formal compliance without a check against the standard.

Give each topic one main home. Link to it instead of copying its detail:

| Topic | Home |
| --- | --- |
| Short product view and entry points | `README.md` and `docs/product/` |
| Domain language and technical definitions | `CONTEXT.md` and `docs/glossary.md` |
| Current decisions and their reasons | `docs/adr/` |
| Structure and flows | `docs/architecture/`, with Mermaid where useful |
| Commands, coverage and readiness limits | `docs/architecture/workspace-guide.md` |

Keep current behavior, planned work and pending approvals distinct. Update ADRs to state the current decision; Git keeps old decisions. Remove completed plans only after preserving useful reasons, evidence and unresolved requirements. Do not add progress logs or permanent docs for one-off questions.

Keep each Markdown paragraph on one line. In code, use few, short comments. Explain a reason or constraint that the code cannot show. Preserve safety assumptions, math bounds, tool directives and required notices. Do not edit generated or third-party text for style.

## Agent guides

- [Issue tracker](docs/agents/issue-tracker.md)
- [Triage labels](docs/agents/triage-labels.md)
- [Domain docs](docs/agents/domain.md)
