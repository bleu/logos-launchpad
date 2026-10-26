# LEZ v0.4 migration plan

Status: The actor development base and application are migrated to the selected revision. Listed local checks passed; full sale-flow compatibility and production readiness are not established. See [ADR 0034](../docs/adr/0034-lez-v04-migration.md) for the approach and the [workspace guide](../docs/architecture/workspace-guide.md#execution-and-evidence-limits) for results and remaining gaps.

## Agreed scope

Migrate implemented features to a fixed LEZ v0.4 development revision. Keep unfinished product features as separate work. Preserve mechanism-specific rules and existing evidence limits. Update repository guidance so future features use the selected platform interfaces, with links to upstream documentation rather than a full copy of platform details. Recheck compatibility when the official v0.4 release is available.

Nothing from this project is deployed to testnet or production. There are no deployed programs, sales, balances or accounts to migrate. This work does not include deployment, deployed-state transfer or production cutover.

Network fees use the supported Logos wallet and SDK interfaces and platform conventions. Missing support is a platform dependency; this project will not build a separate network-fee mechanism. Launchpad protocol fees remain application rules.

For missing platform dependencies, implement minimal local versions that follow the available interfaces as closely as possible. Keep their scope small and preserve the authorization, custody and accounting behavior required by existing features. The sequence below defines the required operations and replacement boundaries. Add explicit follow-up tasks to integrate the supported upstream implementations when available.

Repository-owned development tooling can remain maintained after the migration. Cover the build, interface-generation, local execution and verification tasks this repository needs instead of reproducing all scaffold features. Follow-up upstream integration does not require discarding useful repository commands.

Use the LEZ actor interface directly with narrow repository helpers instead of rebuilding the full SPEL framework and code-generation system. Clearly mark temporary compatibility components, explain what upstream support replaces each one, and link its removal task. Keep maintained development tooling distinct from temporary runtime support.

Completion of the full private route remains separate work. The temporary token implementation has no private holdings. Existing private purchase and authority helpers return explicit unsupported errors; they are not successful private routes.

Completion checks cover locked builds, existing tests adapted to the selected interfaces, local execution of supported public flows, client/program encoding checks, and build/load checks for the existing app packages. Use disposable local state. Real-proof readiness remains a separate visible requirement. Testnet deployment is not required.

Do not require a running node or full local network for routine app tests. Use host tests by default. Evaluate one small in-process actor test with a successful transfer and a failed transfer that preserves balances before building a broader execution suite. Record setup requirements, build time and test time. Defer heavy or fragile platform test infrastructure until a simpler or more stable path is available. Such deferral remains an explicit evidence gap, not a passing integration result. Public guest execution does not require cryptographic proof generation; it still requires compatible guest binaries and an executor.

Preserve the existing curve creator and pool-owner authority behavior where the selected platform supports it. Replacing their legacy authority NFTs with role authority remains separate work. If platform compatibility requires a permission change, resolve that conflict before implementation. This does not defer compatibility work for role authority already used by namespace administration and fixed-price setup.

## Current baseline and sources

The application and development base now target the same fixed actor revision. Root handlers accept `ReceiveInput` and return `Result<Response, Error>`. There is no supported old-platform lane. See the [workspace guide](../docs/architecture/workspace-guide.md) for current commands and known gaps, [repository layout](../docs/architecture/repository-layout.md) for component boundaries, and [ADR index](../docs/adr/README.md) for accepted designs and unfinished work. This plan does not replace the broader product delivery plan.

The three `.lgx` archives passed the Nix build recorded in the [package README](../packages/launchpad/README.md). Installation, loading and the `Test backend` action are blocked by missing Basecamp and `lgpm`; no `Backend OK` result is recorded. Local locked builds and tests, strict Clippy, formatting, pin/shell and IDL/wire checks passed. The execution gate passed with unchanged lockfiles; Linux CI has not run. Broader sale execution remains deferred. Seeded curve tests executed public buys, stale-cap rejection and clock-gated deadline checks, with unchanged state and nonce on rejection. The clock test uses the real upstream assertion guest with seeded clock data. It does not cover sequencer `Tick`, setup, factory clocks, settlement, graduation or fees. Costly execution checks can be deferred with a named evidence gap. Neither host checks nor token-fixture rollback establish sale rollback.

The dated [upstream source check](../docs/research/logos-v02-v04-migration-check.md) links the actor interface, program identities and wallet/module changes. It is source evidence, not a compatible dependency set. The exact revisions and supported platform interfaces must be checked before implementation.

## Implementation sequence

Deliver this work in two issues. The development-base issue owns phases 1–2, including their checks and documentation. The application-migration issue depends on that base and owns phases 3–6, including final verification and guidance. Use checklists and focused commits inside each issue; do not create a ticket for every phase or helper. Keep upstream replacements as follow-up checklists rather than additional migration issues.

The six stages below describe the work sequence within those two issues. Record blocked checks by name. Do not report full compatibility if a required component remains blocked. Keep existing public behavior and known incomplete private behavior distinct throughout the migration.

### 1. Fix the target and check the toolchain

- Use actor development revision `578ce58a71b79c49d48e97e51c520bf71f0252d9` as the initial candidate. Verify that all required public interfaces are present before changing application code. Any later revision change must update the dependency record and affected checks together.
- Record the LEZ, Rust, RISC0 and circuit requirements for each workspace. Check a minimal host and actor guest build without starting a node.
- Check the in-memory execution path using the upstream [flash-swap example](https://github.com/logos-blockchain/logos-execution-zone/blob/578ce58a71b79c49d48e97e51c520bf71f0252d9/lee/state_machine/src/state/tests/flash_swap.rs) as a reference. Measure the small transfer test before committing to broader runtime tests. Do not build a replacement LEZ execution engine if this path is unavailable.
- Record existing feature tests and their evidence limits as the migration baseline. Retain the original pins in repository history, without maintaining two active platform implementations.

### 2. Add narrow compatibility components and maintained tooling

- Keep temporary token, ATA and existing role-authority support outside mechanism-specific pricing and lifecycle modules. Reuse reviewed upstream logic where suitable, recording revisions and required notices.
- Token support covers existing fungible definitions, metadata, exact supply, holdings, minting, transfers and burns. Retain NFT master creation and transfer required by current curve roles. Do not add a general token product.
- ATA support covers deterministic addresses, holding creation, transfers and burns with signer/PDA authorization and checked balances. Preserve token-definition and canonical-vault checks.
- Existing role-authority support covers bootstrap, current-authority checks, transfer and renunciation where used by namespace and fixed-price setup. It does not replace remaining curve creator/pool-owner NFT roles.
- Use small host/guest-compatible helpers around LEZ actor messages. Do not recreate SPEL macros or its complete framework. Reuse upstream wallet, native-fee, clock and event primitives instead of replacing them.
- Mark each temporary component with its purpose, upstream source, replacement condition and a link to the matching follow-up task below. Keep interface conversion at the compatibility boundary so upstream token types do not spread through sale logic.
- Keep repository-owned commands for guest builds, pin checks, interface generation and verification. Replace scripted scaffold/SPEL CLI requirements only where needed. Keep source IDLs and generated output reproducible and check their wire bytes against actor messages. Update CI setup and shell tests with the commands they verify.

### 3. Port a complete public bonding-curve flow

- Move namespace and curve entry points to actor messages, then update shared factory curve setup. Keep pricing, fee treatment, trade modes, allocation, emergency controls and settlement rules unchanged.
- Design explicit interactions for current namespace fee/treasury settings and trusted time. Authenticate replies and enforce checks within the transaction; do not substitute stale client values for authoritative state.
- Use synchronous Calls for required token movement and related accounting. Keep standalone fee collection atomic and terminal collection part of atomic settlement. Do not introduce automatic Cast-based collection in this migration.
- Distinguish deployed program account addresses from image IDs. Update PDA derivation, canonical vault checks, client address helpers and disposable fixtures together.
- Update source interfaces, generated artifacts, SDK builders and affected CLI calls with each supported operation. Use platform wallet submission and network-fee handling.
- Use typed graduation callbacks with no arbitrary remaining-account forwarding or factory PDA seed grant to the adapter. Runtime transaction signer permissions still apply; do not recreate the old generic unauthorized extra-account guard. See [ADR 0020](../docs/adr/0020-settlement-and-graduation.md).
- Start with namespace setup, sale creation, funding and one exact-output public buy. Then cover remaining implemented trades, administration, freeze, collection, close and settlement.
- Check the Alice/Bob example: both approve 1,000 launch tokens with a 200 collateral limit; Alice executes first; Bob uses current reserves and either pays within the limit or fails without partial application changes. Host checks alone do not establish transaction rollback.

### 4. Port fixed-price setup and existing private helpers

- Port the four implemented fixed-price setup stages using the shared factory and fixed-price actor boundaries. Preserve exact mint/supply, creator authority, allocation escrow, funding and recorded-collateral checks. Apply the amended immediate-start and trusted-clock rules in [ADR 0032](../docs/adr/0032-fixed-price-lbp-setup.md).
- Keep fixed-price trading, close and settlement and all weighted-sale execution outside this migration. Preserve independent weighted math and its existing numerical checks.
- Adapt existing private helpers and callers only where the target platform supports their current scope. Replace obsolete limit-specific failure assertions with accurate target-revision checks; do not preserve a known failure as a product requirement.
- Keep complete private returns, safe gas funding, wallet recovery and successful whole-route evidence as separate feature work. Never treat a compilation pass or helper envelope test as a successful private purchase.

### 5. Finish client, package and verification updates

- Check every existing SDK/CLI operation against the selected actor interfaces, including encoding, addresses, current-state quotes, submission and error handling. Preserve useful user-facing arguments; old transaction bytes need no compatibility layer for undeployed programs.
- Build and load the existing core module and both QML package shells and exercise their current backend smoke action. Do not build replacement wallet/provider modules or new product screens.
- Run adapted locked builds, host/client/CLI tests, wire and pin checks, and the independent math checks. Include each nested workspace explicitly.
- Expand node-free execution checks for supported public flows only if the initial setup is practical. Keep deferred actor execution, RPC/service integration, real proofs and full-network tests visible in the evidence record. Existing broad integration requirements in ADR 0022 remain readiness requirements even when this migration defers their execution.
- Review app-facing and research documentation for stale platform instructions. Keep implementation claims tied to actual checks.

### 6. Update future-feature guidance and record results

- Update `CONTEXT.md`, the ADR index, workspace guide, repository layout and package README where their commands or interface guidance changed. Keep overview pages brief and link to the selected upstream actor, wallet, clock and event interfaces.
- Document temporary component ownership and removal conditions, maintained tooling commands, new message/IDL generation, program addresses versus image IDs, and the exact supported revision set.
- Record which implemented features passed which checks, which checks were deferred, and why. Keep the statement that nothing is deployed to testnet or production unless a later deployment actually changes that fact.

## Follow-up tasks

Track these items as follow-up checklists within the two delivery issues, not separate migration issues. Each upstream replacement must remove superseded temporary code and retain the applicable behavior and wire checks. There is no deployed state to preserve; use fresh disposable development state for replacement checks.

- [ ] <a id="upstream-token-ata"></a> **Integrate upstream token and ATA support.** When actor-compatible versions are available, compare required operations, authority rules, addresses and formats, switch client/program adapters together, remove local token/ATA compatibility implementations, and run the relevant checks.
- [ ] <a id="upstream-authority"></a> **Integrate upstream role-authority support.** Replace the temporary implementation used by current namespace and fixed-price roles when a compatible version exists. Keep the separate curve creator/pool-owner role migration outside this task.
- [ ] <a id="upstream-framework"></a> **Remove superseded framework compatibility helpers.** Review supported SPEL or LEZ helpers when available and replace temporary conversion/generation support where useful. Keep maintained repository commands; do not require adoption of a full framework merely to remove a temporary helper.
- [ ] <a id="upstream-wallet"></a> **Integrate compatible wallet/provider and module interfaces.** Use the accepted provider design when it is ready for actual app workflows. Do not create a replacement provider during this migration.
- [ ] <a id="release-check"></a> **Check the official v0.4 release.** Compare it with the selected development revision, update compatible dependency pins and generated artifacts, then repeat affected checks.
- [ ] <a id="deferred-evidence"></a> **Complete deferred execution and readiness checks.** Revisit skipped in-process checks, service integration, real proofs and live verification when practical. Keep full private-route completion as separate product work under ADR 0018.

## Dependency source check

Checked on 20 October 2026 against LEZ actor development revision `578ce58a71b79c49d48e97e51c520bf71f0252d9`. The table records source checks. The separate [foundation evidence](../packages/lez-compat/README.md#evidence-and-limits) records completed host, guest-build and token-execution checks. Default branches and visible development branches do not establish a compatible upstream dependency set.

| Integration | Evidence | Consequence |
| --- | --- | --- |
| Token and ATA | [Default manifest](https://github.com/logos-blockchain/lez-programs/blob/1288474c7b4c3b0bc96fdfe02848d59a0a800d6d/Cargo.toml) pins LEZ v0.2.5-rc2. The [development token guest](https://github.com/logos-blockchain/lez-programs/blob/4b4e8b8f3f769b5ff8198b313f2c31ec79dbf665/programs/token/methods/guest/src/bin/token.rs) and [ATA guest](https://github.com/logos-blockchain/lez-programs/blob/4b4e8b8f3f769b5ff8198b313f2c31ec79dbf665/programs/ata/methods/guest/src/bin/ata.rs) use plan/apply. | No actor-compatible port was found in the checked branches and open PRs. Current LEZ dev also no longer contains the old token/ATA crates. |
| SPEL | [Manifest](https://github.com/logos-co/spel/blob/c6f73ce21e4805f8430f03afdbdedf0b7e479a5e/spel-framework/Cargo.toml) pins LEZ v0.2.5-rc2. | Existing framework and authority integrations need actor-compatible support. |
| Scaffold | [Open PR #273](https://github.com/logos-co/scaffold/pull/273) targets LEZ v0.3.1 and removes SPEL integration. | This PR does not establish actor-development compatibility. |
| Role authority | [Library manifest](https://github.com/mmlado/spel-admin-authority/blob/027197ffc28e9a48e87c3822bcd44d52482adca4/admin-authority/Cargo.toml) uses tagged SPEL v0.7.0, which pins LEZ v0.2.4; current source uses the old account interface. | Compatibility is required for existing namespace and fixed-price setup roles, separately from the future curve creator/pool-owner role change. |
| Public Rust wallet and network fees | [Wallet implementation](https://github.com/logos-blockchain/logos-execution-zone/blob/578ce58a71b79c49d48e97e51c520bf71f0252d9/lez/wallet/src/lib.rs) provides actor transaction submission, payer selection and fee declarations. | Use this platform API and verify it locally; no separate network-fee mechanism is needed. |
| Basecamp LEZ module | [Module inputs](https://github.com/logos-blockchain/logos-execution-zone-module/blob/main/flake.nix) still target LEZ v0.3.1 and the old wallet library. | Do not assume actor-development compatibility for future wallet-connected app workflows. Existing package build/load checks remain required. |
| Clock | [Clock interface](https://github.com/logos-blockchain/logos-execution-zone/blob/578ce58a71b79c49d48e97e51c520bf71f0252d9/lez/programs/clock/core/src/lib.rs) includes actor messages and timestamp assertions. | The primitive exists. Adapt sale time checks and verify behavior on the chosen revision. |
| Events and indexer | [Indexer RPC](https://github.com/logos-blockchain/logos-execution-zone/blob/578ce58a71b79c49d48e97e51c520bf71f0252d9/lez/indexer/service/rpc/src/lib.rs) exposes event queries. The [default event filter](https://github.com/logos-blockchain/logos-execution-zone/blob/578ce58a71b79c49d48e97e51c520bf71f0252d9/lez/indexer/core/src/event_filter.rs) stores no events. | Platform interfaces exist; capture configuration and app-module compatibility need checks before history is claimed. |
