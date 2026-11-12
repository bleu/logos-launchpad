# Workspace guide

The application is [migrated to the selected LEZ actor revision](../../plans/lez-v04-migration.md). Local checks below passed with the stated evidence gaps. There is no second supported legacy platform. See [repository layout](repository-layout.md) for package roles and [ADR 0034](../adr/0034-lez-v04-migration.md) for the decision.

## Light development checks

Run `bash verify/check-dev.sh` for everyday work. It runs locked host tests for the core sale crates, compatibility support, weighted math and launchpad core logic. It uses the checked-in emergency-authority fixture when the variable is unset and omits debug symbols unless the caller selects another setting. It needs host Rust, but no RISC0 tools, node or wallet setup.

Plain `cargo test --locked` selects only the root core crates. Client and CLI tests need explicit package selection or `--workspace`. Guest builds, execution tests and package builds are manual. Light checks do not establish client, wire or runtime compatibility.

## Build contexts

Full checks use LEZ `578ce58a71b79c49d48e97e51c520bf71f0252d9`, host Rust 1.98.1, guest Rust 1.94.1, RISC0 3.0.5 and zkOS wrapper 2.2.2. The [development base guide](../../packages/lez-compat/README.md) gives installation requirements. Check local tools with `bash verify/check-builds.sh --preflight`. Scaffold and SPEL CLI are not required.

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

The build script selects locked host/client/CLI tests, application and compatibility guest builds, weighted-math checks, math guest parity and release-overflow checks. It checks that lockfiles stay unchanged. The local results and their limits are recorded below. The old `--actor` flag is an alias for the same platform, not a separate foundation-only lane.

`check-pins.sh` checks application and compatibility dependencies against the selected actor revision. `generate-idl.sh` generates the actor schemas from Rust declarations. `check-idl.sh` checks drift and Borsh wire fixtures. The application schema is [`idl/launchpad.json`](../../idl/launchpad.json); the compatibility schema is separate. The application schema includes serialized private types. Schema `imports` lists parent modules to search when a type is not defined in the current module. The old SPEL instruction schemas are not supported. Regenerate after changing message types; do not edit generated output by hand.

Root handlers accept `ReceiveInput` and return `Result<Response, Error>`. Guests turn handler errors into transaction failure. Program account addresses identify deployments; image IDs identify binaries. Use program account addresses for PDA derivation and callbacks. Rebuild clients and guests together after wire changes. See the [actor interface guide](../../packages/lez-compat/README.md#interfaces-and-addresses).

Application, SDK and CLI timestamps use Unix seconds. Programs convert stored bounds to LEZ milliseconds with checked arithmetic. Response validity windows enforce start-inclusive, end-exclusive inclusion-block time. Saved clock data cannot enforce the current block's deadline. See [sale closing](../adr/0025-sale-closing.md).

## CI

[Root CI](../../.github/workflows/ci.yml) runs formatting, static pins, mock shell regressions and the light host tests on pushes and pull requests. Full builds, client/CLI and IDL checks, guest parity and node-free execution run only through `workflow_dispatch` with `full` enabled. The full job is manual for now. Package installation, app smoke actions, live service integration, fuzz campaigns and real proofs remain separate checks.

## Execution and evidence limits

Add `--execution` to `check-builds.sh` to select the optional node-free token, seeded sale and role-authority fixtures. The script builds the application and compatibility guests and locates the pinned upstream clock guest; `LAUNCHPAD_CLOCK_ELF` can override that path. Execution needs local processes and IPC. A sandbox rejection is not a passing result. Token-only rollback checks do not prove sale rollback.

The two [seeded curve tests](../../methods/tests/actor_sale.rs) passed through real curve and compatibility-token guests on `V03State`. Public buys passed, and stale-cap rejection preserved full state and buyer nonce. With saved clock time held before expiry, the same signed trade passed at the last millisecond before expiry and failed at expiry and later. Validity rejection preserved full state and nonce.

The five seeded fixed-price tests passed through real fixed-price, namespace and compatibility-token guests. They cover fee-first exact-input and exact-output buys, live fees, current-treasury collection, surplus, no-end sellout, inclusion bounds, slippage, fee-consumed dust and freeze. The same signed buy passed at start and just before end, but failed before start, at end and later, with saved clock time unchanged. Rejection preserved full state and buyer nonce. A failed output transfer also rolled back the earlier input transfer. Fee collection passed after expiry while frozen and without a signer; a zero-fee retry left state unchanged.

The five seeded weighted-sale tests passed through real sale, namespace, clock and compatibility-token guests on 4 November 2026. They cover both buy forms, delayed execution within the validity window, inclusion-window rejection with stale saved clock data, successive recorded reserves, fee-neutral pricing after collection, accuracy rejection and vault overflow. A failed final token credit preserved the full state and buyer nonce, including the earlier collateral transfer. A separate weighted factory test passed all four creation stages through real factory, authority, token, namespace, sale and clock guests. It covers both allocation choices, normalized fractional weights, full creator seed funding, surplus, and a buy after setup. Creation at the scheduled start passed. Creation after that start and every incomplete setup stage at or after the end were rejected with stale saved clock data. Setup immediately before the end passed. An insufficient seed and a failed final allocation credit preserved full state and creator nonce. The latter check includes rollback of earlier inventory transfer, seed transfer and pool activation. Public-account admission uses canonical PDA evidence. These local checks do not produce real proofs or check live clock updates.

On 6 November 2026, the sale guest suite passed 26 tests with the temporary pricing clock. Weighted controls and settlement checks cover creator authorization, pause and resume, scheduled-end inclusion bounds, both allocation and settlement orders, frozen buys and permitted frozen payouts. Forced fee-credit, burn, final proceeds-credit and allocation-credit failures preserve full runtime state and creator nonce. Repaired retries pay and burn once. Host checks cover the checkpoint event selector and payload. The IDL gate and 32 client wire tests passed, including the new weighted commands and weighted state layout. These checks do not establish real clock support, private execution or production readiness.

The [role-authority guest tests](../../crates/launchpad-client/tests/role_authority_guest.rs) use signed `V03State` transactions through real guests. They cover fresh namespace, direct-pool and factory setup, role transfer and rejection, and a program-controlled namespace admin that accepts its role and changes a fee. The [controls tests](../../crates/launchpad-client/tests/role_authority_guest/controls.rs) cover both trade modes, live fees, current-treasury collection, namespace isolation, expiry and `D` depletion. Freeze blocks swaps while administration, role transfer, collection and close remain available.

The [settlement tests](../../crates/launchpad-client/tests/role_authority_guest/settlement.rs) cover manual and graduation routes, allocation, unsold-token burn, payout failures, callback rejection and invalid final acknowledgment. Failed transactions preserve full state and signer nonces; repaired retries do not repeat completed payments or burns. Tests also cover competing-route rejection, manual recovery, frozen settlement and the permitted zero-payout hook. A real adapter receives payouts and records a successful callback. This proves local graduation execution, not DEX liquidity.

The seeded fixed-price trade checks do not exercise fresh setup or settlement. Weighted setup has the separate factory test above. To run those trade fixtures directly:

```sh
RISC0_BUILD_LOCKED=1 cargo +1.98.1 test --manifest-path methods/Cargo.toml --release --locked --features actor-execution-tests --test actor_sale
```

The local `bash verify/check-builds.sh --execution` gate passed with unchanged lockfiles. It checks release host tests, application and compatibility guests, weighted-math parity, release-overflow checks, token execution, seeded sale tests and the signed curve contract guest tests. IDL and wire checks, formatting, pins and shell regressions are separate checks. Linux CI has not run for the expanded curve evidence.

A separate direct run passed all eleven sale tests, including the weighted factory test.

Fixed-price setup and payout runtime checks remain deferred, as do sequencer `Tick`, wallet/service integration and real proofs. Old namespace and private-helper guest evidence predates this actor migration and cannot establish compatibility on the selected revision. Private purchase and authority helpers return explicit unsupported errors because the compatibility token implementation has no private holdings.

The three launchpad package archives passed the Nix build. Installation, loading and `Test backend` are blocked by missing Basecamp and `lgpm`; no `Backend OK` result is recorded. See the [package README](../../packages/launchpad/README.md) for the command and local requirements.

Host tests do not prove guest execution or cross-program rollback. Guest development receipts and standalone math parity do not establish production readiness. Real proofs, service integration, wallet workflows and live E2E verification remain separate evidence gaps. Use disposable state for execution checks. Nothing is deployed to testnet or production, and no state migration is provided.

For stable release image IDs, the build script requires `/tmp/logos-launchpad-release/source` with `--release`. Build paths can change guest binaries and IDs; cross-OS identity remains unverified. See [verification boundaries](../adr/0022-verification-boundaries.md).

## Licenses

The curve packages use the root [MIT license](../../LICENSE). Weighted math retains its own MIT license and the Balancer comparator notices in `packages/weighted-math/crates/harness/balancer-ref/`. Third-party dependencies keep their own licenses.

## Temporary weighted pricing clock

Weighted buy messages no longer contain pricing windows. The sale requests one timestamp through a temporary read-and-reply interface. The existing fake clock is the default provider; no build-time switch is needed. It uses fixture state at a separate actor address and does not replace the canonical clock used for setup.

Run `bash verify/check-weighted-clock.sh` for focused host and guest checks. The light development gate already covers weighted host tests in its root run. Use `bash verify/check-builds.sh --execution` with the normal emergency-authority setup for the full execution gate. The earlier window-based evidence above describes the previous implementation; current pricing follows [ADR 0026](../adr/0026-lbp-clock-and-poke.md#clock-read-amendment-2026-11-05).

The local fixture models previous-block milliseconds. Fixture time does not advance automatically when inclusion time advances. These checks do not prove real clock reads, live clock updates, private execution or production readiness. Replace the fake clock with a verified LEZ timestamp provider and repeat these checks before real sales.

## Historical clock evidence

The following result records the earlier opt-in provider. Its disabled-provider check does not describe current default behavior.

On 5 November 2026, the host development gate, 24 weighted host tests, 30 client wire tests and IDL checks passed. All 13 existing sale guest tests passed with the fake clock. Two focused clock guest tests then passed, including the new malformed-clock and freeze test. The normal-build provider check also passed with the mock disabled. Lockfiles did not change.
