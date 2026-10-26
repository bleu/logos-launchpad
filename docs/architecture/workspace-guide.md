# Workspace guide

The application is [migrated to the selected LEZ actor revision](../../plans/lez-v04-migration.md). Local checks below passed with the stated evidence gaps. There is no second supported legacy platform. See [repository layout](repository-layout.md) for package roles and [ADR 0034](../adr/0034-lez-v04-migration.md) for the decision.

## Build contexts

Use LEZ `578ce58a71b79c49d48e97e51c520bf71f0252d9`, host Rust 1.98.1, guest Rust 1.94.1, RISC0 3.0.5 and zkOS wrapper 2.2.2. The [development base guide](../../packages/lez-compat/README.md) gives installation requirements. Check local tools with `bash verify/check-builds.sh --preflight`. Scaffold and SPEL CLI are not required.

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

## Repository checks

Run from the repository root:

```sh
bash verify/check-pins.sh
bash verify/check-builds.sh
bash verify/check-idl.sh
```

The build script selects locked host/client/CLI tests, application and compatibility guest builds, weighted-math checks, math guest parity and release-overflow checks. It checks that lockfiles stay unchanged. The local results and their limits are recorded below. The old `--actor` flag is an alias for the same platform, not a separate foundation-only lane.

`check-pins.sh` checks application and compatibility dependencies against the selected actor revision. `generate-idl.sh` generates the actor schemas from Rust declarations. `check-idl.sh` checks drift and Borsh wire fixtures. The application schema is [`idl/launchpad.json`](../../idl/launchpad.json); the compatibility schema is separate. The application schema includes serialized private types. Schema `imports` lists parent modules to search when a type is not defined in the current module. The old SPEL instruction schemas are not supported. Regenerate after changing message types; do not edit generated output by hand.

Root handlers accept `ReceiveInput` and return `Result<Response, Error>`. Guests turn handler errors into transaction failure. Program account addresses identify deployments; image IDs identify binaries. Use program account addresses for PDA derivation and callbacks. Rebuild clients and guests together after wire changes. See the [actor interface guide](../../packages/lez-compat/README.md#interfaces-and-addresses).

Application, SDK and CLI timestamps use Unix seconds. The native LEZ clock uses Unix milliseconds; programs convert checked bounds at that interface. Deadline checks use trusted time intervals rather than exact client-time equality. See [sale closing](../adr/0025-sale-closing.md).

## CI

[Root CI](../../.github/workflows/ci.yml) is configured for actor formatting, pin and shell checks, locked repository builds and tests, and generated-interface checks. Manual runs can select node-free actor fixtures. This configuration does not establish a passing run. Package installation, app smoke actions, live service integration, fuzz campaigns and real proofs remain separate checks.

## Execution and evidence limits

Add `--execution` to `check-builds.sh` to select the optional node-free token and seeded curve fixtures. They need compatible guest binaries and an executor that can start local processes and use local IPC. A sandbox rejection is not a passing result. The token fixture's successful transfer and rollback checks cover disposable token actors only; they do not prove sale rollback.

The two [seeded curve tests](../../methods/tests/actor_sale.rs) passed through real curve and compatibility-token guests on `V03State`. Public buys passed, and stale-cap rejection preserved full state and buyer nonce. A delayed pre-expiry trade passed with the upstream `AssertTimestamp` guest and seeded native clock data; expiry rejection also preserved state and nonce. See the tests for exact amounts and timestamps.

These checks do not exercise sequencer `Tick`, setup, factory clocks, settlement, graduation, fees, wallet submission or real proofs. To run them directly:

```sh
RISC0_BUILD_LOCKED=1 cargo +1.98.1 test --manifest-path methods/Cargo.toml --release --locked --features actor-execution-tests --test actor_sale
```

The local `bash verify/check-builds.sh --execution` gate passed with unchanged lockfiles. It covered release host tests, application and compatibility guests, weighted-math parity, release-overflow checks, token execution and the two seeded curve tests. Strict root and compatibility Clippy, formatting across maintained workspaces, IDL and wire checks, and pin and shell regressions also passed. Linux CI has not run.

Setup, factory-clock, settlement, graduation, fee and PDA-administration runtime checks remain deferred, as do sequencer `Tick`, wallet/service integration and real proofs. Old namespace, private-helper and settlement guest evidence predates this actor migration and cannot establish compatibility on the selected revision. Private purchase and authority helpers return explicit unsupported errors because the compatibility token implementation has no private holdings.

The three launchpad package archives passed the Nix build. Installation, loading and `Test backend` are blocked by missing Basecamp and `lgpm`; no `Backend OK` result is recorded. See the [package README](../../packages/launchpad/README.md) for the command and local requirements.

Host tests do not prove guest execution or cross-program rollback. Guest development receipts and standalone math parity do not establish production readiness. Real proofs, service integration, wallet workflows and live E2E verification remain separate evidence gaps. Use disposable state for execution checks. Nothing is deployed to testnet or production, and no state migration is provided.

For stable release image IDs, the build script requires `/tmp/logos-launchpad-release/source` with `--release`. Build paths can change guest binaries and IDs; cross-OS identity remains unverified. See [verification boundaries](../adr/0022-verification-boundaries.md).

## Licenses

The curve packages use the root [MIT license](../../LICENSE). Weighted math retains its own MIT license and the Balancer comparator notices in `packages/weighted-math/crates/harness/balancer-ref/`. Third-party dependencies keep their own licenses.
