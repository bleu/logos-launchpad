# Workspace guide

The application uses the [selected LEZ actor revision](../../plans/lez-v04-migration.md). There is no second supported legacy platform. See [repository layout](repository-layout.md) for package roles and [ADR 0034](../adr/0034-lez-v04-migration.md) for the decision.

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
LAUNCHPAD_MOCK_CLOCK=1 bash verify/check-builds.sh --execution
bash verify/check-idl.sh
```

The build script selects locked host/client/CLI tests, application and compatibility guest builds, weighted-math checks, math guest parity and release-overflow checks. It checks that lockfiles stay unchanged. The old `--actor` flag is an alias for the same platform, not a separate foundation-only lane.

`check-pins.sh` checks application and compatibility dependencies against the selected actor revision. `generate-idl.sh` generates the actor schemas from Rust declarations. `check-idl.sh` checks drift and Borsh wire fixtures. The application schema is [`idl/launchpad.json`](../../idl/launchpad.json); the compatibility schema is separate. The application schema includes serialized private types. Schema `imports` lists parent modules to search when a type is not defined in the current module. The old SPEL instruction schemas are not supported. Regenerate after changing message types; do not edit generated output by hand.

Root handlers accept `ReceiveInput` and return `Result<Response, Error>`. Guests turn handler errors into transaction failure. Program account addresses identify deployments; image IDs identify binaries. Use program account addresses for PDA derivation and callbacks. Rebuild clients and guests together after wire changes. See the [actor interface guide](../../packages/lez-compat/README.md#interfaces-and-addresses).

Application, SDK and CLI timestamps use Unix seconds. Programs convert stored bounds to LEZ milliseconds with checked arithmetic. Response validity windows enforce start-inclusive, end-exclusive inclusion-block time. Saved clock data cannot enforce the current block's deadline. See [sale closing](../adr/0025-sale-closing.md).

## CLI payout terms

Curve `create-sale` uses `--graduation-launch-bps` and `--graduation-collateral-bps` instead of the removed DEX-seed reserve input. Both must be explicit with `--graduation-hook-program-id`, including zero values. Each is in `0..=10_000`; without a hook, only zero shares are valid. `status` and `sale-info` show the saved terms. See [ADR 0020](../adr/0020-settlement-and-graduation.md) for eligible balances and required routing. This does not add fixed-price or weighted CLI creation flows.

`graduate` is permissionless and requires `--creator-hint`. The factory verifies that hint through role-authority `Resolve` and pays the current creator. A failed callback permits retry, with no manual bypass of the configured adapter. Adapter initialization requires an existing factory and a matching sale kind and adapter program; it has no later activation step.

## CI

[Root CI](../../.github/workflows/ci.yml) runs formatting, static pins, mock shell regressions and the light host tests on pushes and pull requests. Full builds, client/CLI and IDL checks, guest parity and node-free execution run only through `workflow_dispatch` with `full` enabled. The full job is manual for now. Package installation, app smoke actions, live service integration, fuzz campaigns and real proofs remain separate checks.

## Execution and evidence limits

Add `--execution` to `check-builds.sh` to select the optional node-free token, seeded sale and role-authority fixtures. The script builds the application and compatibility guests and locates the pinned upstream clock guest; `LAUNCHPAD_CLOCK_ELF` can override that path. Execution needs local processes and IPC. A sandbox rejection is not a passing result. Token-only rollback checks do not prove sale rollback.

The [sale tests](../../methods/tests/actor_sale.rs) and [role-authority tests](../../crates/launchpad-client/tests/role_authority_guest.rs) exercise signed setup, trades, controls and settlement through the guests. Fault tests check rollback and retries. Use disposable state; replacing a test fault adapter does not establish live adapter replacement.

Host tests do not prove guest execution. Local guest execution does not establish real proofs, live clock support, wallet/service integration, package loading or DEX liquidity. Private routes are unsupported because compatibility token holdings are public only. See the [package README](../../packages/launchpad/README.md) for package requirements.

For stable release image IDs, the build script requires `/tmp/logos-launchpad-release/source` with `--release`. Build paths can change guest binaries and IDs; cross-OS identity remains unverified. See [verification boundaries](../adr/0022-verification-boundaries.md).

## Licenses

The curve packages use the root [MIT license](../../LICENSE). Weighted math retains its own MIT license and the Balancer comparator notices in `packages/weighted-math/crates/harness/balancer-ref/`. Third-party dependencies keep their own licenses.

## Temporary weighted pricing clock

Weighted buy messages no longer contain pricing windows. The sale requests one timestamp through a temporary read-and-reply interface. Normal builds reject weighted buys with `ClockUnavailable`. Build with `LAUNCHPAD_MOCK_CLOCK=1` only for local tests. The fake clock uses fixture state at a separate actor address. It does not replace the canonical clock used for setup. Changes to this variable rebuild the sale guests.

Run `bash verify/check-weighted-clock.sh` for host and guest checks with the fake clock. To include these buys in the full execution gate, run `LAUNCHPAD_MOCK_CLOCK=1 bash verify/check-builds.sh --execution` with the normal emergency-authority setup. Keep that variable unset for normal builds. See [ADR 0026](../adr/0026-lbp-clock-and-poke.md#clock-read-amendment-2026-11-05).

The local fixture models previous-block milliseconds. It can hold that value while inclusion time advances. These checks do not prove real clock reads, live clock updates, private execution or production readiness. A future LEZ clock API must replace the temporary provider and pass these checks again.
