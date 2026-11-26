# Workspace guide

The application uses one selected LEZ actor revision. There is no supported legacy lane or deployed state to migrate. [ADR 0034](../adr/0034-lez-v04-migration.md) owns the revision decision; [repository layout](repository-layout.md) defines package roles.

## Light development checks

Run `bash verify/check-dev.sh` for everyday work. It runs locked host tests for the core sale crates, compatibility support, weighted math and launchpad core logic. It uses the checked-in emergency-authority fixture when the variable is unset and omits debug symbols unless the caller selects another setting. It needs host Rust, but no RISC0 tools, node or wallet setup.

Plain `cargo test --locked` selects only the root core crates. Client and CLI tests need explicit package selection or `--workspace`. Guest builds, execution tests and package builds are manual. Light checks do not establish client, wire or runtime compatibility.

## Build contexts

Full checks use LEZ `55c0bf37184728556e158a9ffa987e3e9b41732b`, host Rust 1.98.1, guest Rust 1.94.1, RISC0 3.1.0 and zkOS wrapper 2.2.3. The RISC0 build library is 3.0.7; it has a separate version from the zkVM and executor. The [development base guide](../../packages/lez-compat/README.md) gives installation requirements. Check local tools with `bash verify/check-builds.sh --preflight`. Scaffold and SPEL CLI are not required.

Curve host and guest builds require `CURVE_EMERGENCY_AUTHORITY`, a nonzero 64-character hex account ID. For local tests, run this from the repository root:

```sh
export CURVE_EMERGENCY_AUTHORITY="$(< crates/curve-core/tests/fixtures/emergency-authority.hex)"
```

The built guest contains that selection; transaction callers cannot replace it. See [ADR 0028](../adr/0028-token-custody-and-freeze.md).

| Workspace | Contents | Command from the repository root |
| --- | --- | --- |
| Root `Cargo.toml` | Curve math, pool, handlers, client and CLI | `cargo +1.98.1 test --workspace --locked` |
| `packages/lez-compat/` | Temporary token, ATA and role-authority support | See the [development base guide](../../packages/lez-compat/README.md) |
| `packages/weighted-math/` | Independent kernel, harness and numerical fixtures | `cargo +1.98.1 test --manifest-path packages/weighted-math/Cargo.toml --workspace --locked` |
| Launchpad core workspaces | Separate logic and native adapter workspaces | See the [package README](../../packages/launchpad/README.md) |
| `methods/guest/` | Application actor guest entry points | `RISC0_BUILD_LOCKED=1 cargo +1.98.1 build --manifest-path methods/Cargo.toml --release --locked` |
| `packages/weighted-math/zkvm/` | Weighted-math host/guest parity | `RISC0_BUILD_LOCKED=1 cargo +1.98.1 run --manifest-path packages/weighted-math/zkvm/Cargo.toml --release --locked` |
| `packages/weighted-math/fuzz/` | Weighted trade invariant fuzz target | Run `cargo +nightly fuzz run trade_invariant` from that workspace |

Root Cargo commands do not include nested workspaces. Keep their lockfiles separate. `RISC0_BUILD_LOCKED=1` locks nested guest builds; outer Cargo `--locked` alone does not. Fuzzing needs nightly Rust and `cargo-fuzz`.

## Manual full checks

Run from the repository root after setting `CURVE_EMERGENCY_AUTHORITY` as above:

```sh
bash verify/check-pins.sh
bash verify/check-builds.sh --execution
bash verify/check-idl.sh
```

The build script selects locked host/client/CLI tests, application and compatibility guest builds, weighted-math checks, math guest parity and release-overflow checks. It checks that lockfiles stay unchanged. The old `--actor` flag is an alias for the same platform, not a separate foundation-only lane.

`check-pins.sh` checks application and compatibility dependencies against the selected actor revision. `generate-idl.sh` generates the actor schemas from Rust declarations. `check-idl.sh` checks drift and Borsh wire fixtures. The application schema is [`idl/launchpad.json`](../../idl/launchpad.json); the compatibility schema is separate. The application schema includes serialized private types. Schema `imports` lists parent modules to search when a type is not defined in the current module. The old SPEL instruction schemas are not supported. Regenerate after changing message types; do not edit generated output by hand.

Root handlers accept `ReceiveInput` and return `Result<Response, Error>`. Guests turn handler errors into transaction failure. Program account addresses identify deployments; image IDs identify binaries. Use program account addresses for PDA derivation and callbacks. Rebuild clients and guests together after wire changes. See the [actor interface guide](../../packages/lez-compat/README.md#interfaces-and-addresses).

Application, SDK and CLI timestamps use Unix seconds. Programs convert stored bounds to LEZ milliseconds with checked arithmetic. Response validity windows enforce start-inclusive, end-exclusive inclusion-block time. Saved clock data cannot enforce the current block's deadline. See [sale closing](../adr/0025-sale-closing.md).

Run the [standalone LEZ checks](local-node-checks.md) to configure a namespace through the CLI and check Rust wallet recovery. These tests need a disposable node and are ignored by ordinary test runs.

Run upstream private-route probes by hand only when evaluating a candidate dependency. Use a separate checkout and the tools required by that revision. Keep these probes out of Launchpad CI and leave the Launchpad pin unchanged. See [LOG-61](https://linear.app/bleu-builders/issue/LOG-61/check-logos-wallet-and-private-route-support) for commands and results.

## CLI payout terms

Curve `create-sale` uses `--graduation-launch-bps` and `--graduation-collateral-bps`. Both must be explicit with `--graduation-hook-program-id`, including zero values. `status` and `sale-info` show saved terms. Permissionless `graduate` requires `--creator-hint`. [ADR 0020](../adr/0020-settlement-and-graduation.md) defines validation, recipients, required routing and adapter initialization. Fixed-price and weighted CLI creation flows remain unsupported.

## CI

[Root CI](../../.github/workflows/ci.yml) runs formatting, static pins, mock shell regressions and the light host tests on pushes and pull requests. Full builds, client/CLI and IDL checks, guest parity and node-free execution run only through `workflow_dispatch` with `full` enabled. The full job is manual for now. Package installation, app smoke actions, live service integration, fuzz campaigns and real proofs remain separate checks.

## Execution and evidence limits

Add `--execution` to `check-builds.sh` to select the optional node-free token, seeded sale and role-authority fixtures. The script builds the application and compatibility guests and locates the pinned upstream clock guest; `LAUNCHPAD_CLOCK_ELF` can override that path. Execution needs local processes and IPC. A sandbox rejection is not a passing result. Token-only rollback checks do not prove sale rollback.

The [sale tests](../../methods/tests/actor_sale.rs) and [role-authority tests](../../crates/launchpad-client/tests/role_authority_guest.rs) cover signed setup for all engines, trades, curve administration and role transfer, controls, fees, manual settlement and graduation callback rollback. Fault tests check full state rollback and retries. See the [allowlist proof design](allowlist-proof.md). Use disposable state; replacing a test fault adapter does not establish live adapter replacement.

On 10 November 2026, 32 signed public sale tests passed, including allowlisted buys within the 10,000,000-cycle budget. Exact-input, exact-output and fresh-setup buys used 658,565/658,680/665,182 cycles for curve sales, 758,814/758,859/758,828 for fixed-price sales and 838,557/840,447/870,899-874,364 for weighted sales. That weighted run used a fake clock. These historical counts are not current performance guarantees or native private-proof evidence.

Local verification on 17 November 2026 included main commit `554f71d` with the selected upstream clock revision. The development gate passed 613 host tests. Release host/client/CLI tests, application and compatibility guest builds, weighted-math parity, release-overflow checks and token execution passed. Separate signed runs passed 82 sale tests and 38 curve, role-authority and settlement tests. IDL and wire checks, shell regressions, formatting and strict host lints passed with unchanged lockfiles. These checks used development receipts. They are evidence for that revision, not later changes.

The three `.lgx` archives passed an earlier Nix build. Installation, loading and `Test backend` were blocked by missing Basecamp and `lgpm`; there was no `Backend OK` result. See the [package evidence](../../packages/launchpad/README.md). These results do not establish current release readiness.

Host tests do not prove guest execution. Local guest execution does not establish real proofs, live sequencer timing, wallet/service integration, package loading or DEX liquidity. Curve failed-setup recovery and adapter-initiated reentry lack focused guest tests. General adapter size/cost and live DEX integration remain unverified. Private routes are unsupported because compatibility token holdings are public only. See the [private evidence requirements](private-participation.md#evidence-still-required) and [package requirements](../../packages/launchpad/README.md).

Ordinary post-close settlement has local execution coverage. The separate delayed-settlement scope item remains unresolved. [ADR 0032](../adr/0032-fixed-price-lbp-setup.md) lists the remaining cancellation, expired-setup recovery and complete SDK/CLI flow gaps.

For stable release image IDs, the build script requires `/tmp/logos-launchpad-release/source` with `--release`. Build paths can change guest binaries and IDs; cross-OS identity remains unverified. See [verification boundaries](../adr/0022-verification-boundaries.md).

## Licenses

Project-owned launchpad packages use [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE), at the recipient's option. Weighted math retains its own MIT license and the Balancer comparator notices in `packages/weighted-math/crates/harness/balancer-ref/`. Third-party dependencies keep their own licenses.

## Weighted pricing clock

Normal builds use the upstream clock under [ADR 0026](../adr/0026-lbp-clock-and-poke.md). There is no mock guest or build flag. Run `bash verify/check-weighted-clock.sh` for focused host and sale guest checks, or `bash verify/check-builds.sh --execution` for the full gate with the emergency-authority setup above. Commands use locked dependencies.

Actor execution fixtures use upstream `test-utils` in debug host builds. LEZ rejects that feature in release builds. Guest binaries and production host checks still use release builds. Release-overflow checks keep their release profile.

The [host tests](../../crates/lbp-core/tests/weighted_trading.rs) and [guest tests](../../methods/tests/actor_sale.rs) cover authenticated reads, malformed and block-ID replies, replay rejection, setup bounds, freeze, fractional schedules, checkpoint events and rollback. A real upstream `Tick` changes a later price. Stale-clock fixtures also check inclusion-time rejection without state or nonce changes. These checks do not prove sequencer ordering, timestamp trust or proof generation time.

An earlier five-test native-token probe on feature-branch pin `f94ca8a7550fa0012222b9fb5990dfe44c54a3dc` checked private funding, a public clock read and atomic private returns. A changed clock reply passed when the return stayed fixed. A return changed from 100 to 99 failed with `CrossMessageMismatch` despite a minimum of 98; a fresh proof passed. Failures preserved state. The probe used development receipts and a small payout guest, not launchpad token programs. It has not been repeated on the selected `dev` pin and does not complete the launchpad private route.

## Platform replacement checklist

The migration is complete for the selected public actor interfaces, not for all product or readiness requirements. The [dated upstream source check](../research/logos-v02-v04-migration-check.md) is background evidence, not a current compatible dependency set. Temporary support and its source/evidence record remain in the [development base guide](../../packages/lez-compat/README.md).

Each replacement must switch client/program interfaces together, remove superseded temporary code and rerun affected host, wire and guest checks with fresh disposable state. Preserve canonical addresses, token-definition checks, authority, custody, accounting and atomic rollback. No deployed-state conversion or backward compatibility is required.

- [ ] <a id="upstream-token-ata"></a> Replace temporary token/ATA support when actor-compatible upstream versions cover required definitions, exact supply, metadata, holdings, minting, transfers and burns with signer/PDA authority and checked balances. Do not build a general token product.
- [ ] <a id="upstream-authority"></a> Replace temporary role authority when compatible support exists. Preserve bootstrap, current-holder checks, consented transfer and each role's renunciation rules under [ADR 0006](../adr/0006-namespaces-and-authority.md).
- [ ] <a id="upstream-framework"></a> Replace temporary conversion/generation helpers where supported LEZ/SPEL helpers fit. Keep useful repository build, pin, IDL and verification commands; a full framework is not required.
- [ ] <a id="upstream-wallet"></a> Integrate compatible wallet/provider and module interfaces under [ADR 0029](../adr/0029-wallet-integration.md). Use platform network-fee handling; missing support is a platform dependency, not grounds for a replacement wallet or fee mechanism. Finish package loading and backend smoke checks without replacing the existing shells.
- [ ] <a id="release-check"></a> Compare the official v0.4 release with the selected revision. Update dependency pins and generated artifacts together, then repeat affected checks.
- [ ] <a id="deferred-evidence"></a> Complete deferred service integration, real proofs, live verification and focused guest gaps listed above. Use host tests for routine work; do not require a full network. Full private-route completion remains separate work under [ADR 0018](../adr/0018-private-participation.md).

## Distribution adapter checks

The handler is in `crates/distribution-adapter/`. The guest is `methods/guest/src/bin/distribution_adapter.rs`. It registers as `DISTRIBUTION_ADAPTER_ELF` in the methods crate. `distribution_fault_adapter` and `distribution_fault_token` are test fixtures; do not deploy them. The SDK owns configuration validation, previews and transaction account declarations.

After selecting the emergency-authority fixture above, run:

```sh
cargo +1.98.1 test -p distribution-adapter --locked
cargo +1.98.1 test -p launchpad-client --test distribution --locked
RISC0_BUILD_LOCKED=1 cargo +1.98.1 test --manifest-path methods/Cargo.toml --locked --features actor-execution-tests --test actor_sale distribution
bash verify/check-idl.sh
```

On 2026-11-18, the five focused distribution guest tests passed. They cover all three sale types, full balances, direct deposits, zero requests, repeated recipients, both eight-rule lists, later calls, preparation failure, intermediate credit failure, callback failure, full-state rollback and repaired retry. Adapter host tests, factory and shared settlement host tests, client/CLI tests, adapter lint checks, schema drift comparison, wire fixtures, source schema tests, formatting and static dependency pins passed. The broader ignored client guest suite and the full release script were not run.

The focused guest tests use real factory, pool, authority, token and distribution guests. The 18 November run used the explicit mock clock for weighted fixtures. Current fixtures use the upstream clock. Tests replace named programs only to inject and repair faults in disposable state. This is not a live adapter replacement path. Local execution does not prove real proofs, service integration, package loading, production tool compatibility or deployment readiness.

The CLI accepts `distribution encode --file rules.json` and `distribution preview --file rules.json --launch-balance 101 --collateral-balance 11` without a wallet or namespace. These commands check structure and percentages only. JSON uses `version: 1` and the two rule lists; each rule has `destination` as a base58 string and `bps` as an integer. Preview balances are explicit inputs and are estimates until execution.

For distribution sale creation, use `create-sale --distribution-configuration rules.json` with `--graduation-hook-program-id` and both graduation percentages. This option replaces `--graduation-hook-configuration`. Before it opens the wallet or submits creation, the CLI checks the recipients against custody for the selected namespace, factory deployment, adapter deployment and launch salt. SDK callers use `distribution::hook` and place its result in the typed sale inputs. Raw opaque configuration does not perform this sale-specific check.

Use `distribution initialize` with the adapter deployment, factory deployment, launch salt, sale kind and fee payer in the selected namespace. It reads configuration from the factory. Use `graduate --distribution` to admit saved recipient accounts for curve graduation. Use `distribution remaining` with the same identity after graduation. Run `--help` for exact flags. The SDK supports all three sale kinds; complete fixed-price and weighted CLI journeys remain outside this change.

## Graduation reserve checks

Creation accepts `graduation_reserve` for each sale type. Curve CLI creation uses `--graduation-reserve`, with zero as the default. Status output includes the saved reserve. The reserve remains in the existing mint custody after allocation. It does not enter the pool or change prices. ADR 0020 owns the [reserve rules](../adr/0020-settlement-and-graduation.md#graduation-reserve).

Run the distribution guest command above to include reserve setup, sellout, manual payouts, surplus, pricing isolation and callback rollback checks. Rebuild all guests and SDK clients after the changed creation, factory-state, event and request encoding. Existing local state must be recreated; no deployed-state migration is required.

On 2026-11-18, all ten focused distribution and reserve guest tests passed with locked guest builds. The reserve cases cover all three engines, manual payouts to a transferred creator role, no-buyer settlement, zero adapter percentages, fixed-price sellout, unchanged trading state, escrow surplus, insufficient reserve custody and repaired retries. Existing transfer and callback fault cases also run with positive `R` and check full-state rollback. The development gate, client/CLI tests, IDL and wire checks, formatting, static pins and strict host and execution-test lints passed. That run used the explicit local mock clock for weighted execution. Current fixtures use the upstream clock. The broader guest suite, full release gate, real proofs and service integration were not run for this change.

On 2026-11-19, merge checks against main `fec1885` passed with the upstream clock. The development gate passed 641 host tests. Workspace tests passed 717 tests, including client and CLI checks. IDL and wire checks passed 74 tests. Fresh application and compatibility guests passed all 98 signed sale tests and 37 client and role-authority execution tests. These include the ten distribution and reserve tests. Formatting, dependency pins, shell checks and strict host and execution-test lints passed. The methods lint skipped guest rebuilding; execution used the guests built before that lint. These runs used development receipts. The full release gate, real proofs and service integration were not run for this merge.

All sale types use the live namespace fee and can set `per_block_token_limit`. Curve CLI creation uses `--per-block-token-limit` in the smallest launch-token units. Capped buys require a canonical block-ID reply and an exact following-block inclusion window. The LBP launch-state format is version 8. Recreate local sale state and rebuild guests and clients after the wire changes. Private-route proof validity and live inclusion success remain unverified.
