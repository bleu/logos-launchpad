# ADR 0034: LEZ v0.4 migration and local compatibility support

Status: Accepted; actor development base implemented; application migration pending

Date: 2026-10-20

## Context

The launchpad uses LEZ v0.2 interfaces. The v0.4 development actor interface changes program inputs, outputs, program identity and cross-program interaction. Reviewed token, ATA, framework, authority and tooling versions do not form a complete actor-compatible set. The [migration plan](../../plans/lez-v04-migration.md) records the source checks and work sequence.

Nothing from this project is deployed to testnet or production. There are no deployed programs, sales, balances or accounts to transfer. The migration covers implemented features and repository guidance for future work; it does not complete unfinished product features or deploy the system.

## Decision

Target one fixed LEZ actor-development revision and check compatibility again when the official v0.4 release is available. Keep pricing and mechanism-specific lifecycle rules separate from platform adaptation. Update program interfaces, generated artifacts, SDK callers and CLI behavior together.

Use minimal local compatibility implementations for missing dependencies. Preserve existing token, custody and authority behavior, including legacy NFT roles until their separate migration. Prefer reviewed source reuse with provenance and required notices. Keep temporary support outside sale rules, mark its replacement condition, and link to an [upstream-integration follow-up task](../../plans/lez-v04-migration.md#follow-up-tasks). Use the direct LEZ actor interface with narrow helpers instead of recreating the full SPEL framework.

Use supported upstream wallet, network-fee, clock and event primitives where available. Do not create a separate network-fee mechanism or replacement wallet/provider product. Repository-owned build, interface-generation and verification commands can remain maintained after temporary runtime support is removed.

Use ordinary host tests by default. Evaluate one small node-free actor-execution test before building a broader suite. Do not require a running node or local network. Heavy or fragile execution infrastructure can be deferred with an explicit evidence gap. This changes the migration test gate, not the [broader readiness requirements](0022-verification-boundaries.md). Host tests and generated messages alone do not prove cross-program execution or rollback.

The foundation uses actor revision `578ce58a71b79c49d48e97e51c520bf71f0252d9`, host Rust 1.98.1, guest Rust 1.94.1, RISC0 3.0.5 and the loader's zkOS wrapper 2.2.2. Keep the temporary handlers in a separate workspace while the application migration is pending. The existing root application is an unported baseline, not a second supported platform implementation. The [development base guide](../../packages/lez-compat/README.md) records the exact requirements, commands and evidence.

Use per-definition and per-holding token actors with synchronous credits and burns. Keep metadata in the definition actor for this temporary interface. Role actors approve key successors through co-signature. A PDA successor's owner program approves through the supplied helper and a runtime seed grant. This replaces a whole-account owner-field check that the actor model no longer supplies. Owner programs must integrate this approval message; source compatibility alone does not prove that integration. Authenticate callback program and account addresses before accepting token or role proofs. These local interfaces preserve required behavior without claiming upstream wire equivalence.

Keep full private-route completion, fixed-price trading and settlement, weighted-sale execution and new app workflows as separate work. Update existing private helpers only within their supported scope and retain accurate limits. Do not claim successful private purchases from interface compatibility.

## Consequences

Local compatibility code adds temporary maintenance and requires a later comparison with upstream behavior. Following an interface does not establish that the implementations are equivalent. The final migration evidence must distinguish source compatibility, host checks, guest execution, package loading and deferred checks. No migration result establishes production readiness or deployment.

The small node-free token test was practical and is retained as an optional lane. It checks a successful transfer, insufficient balance and rollback after a source debit when the destination is undeclared. Host tests and locked guest builds cover the new handlers; broader execution, authority guest flows, service integration, wallet/module builds and real proofs remain separate evidence gaps. The `--actor` verification commands check the foundation without scaffold or SPEL CLI. Existing application commands remain until their migration.
