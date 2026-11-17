# Workspace guide

The application uses the [selected LEZ actor revision](../../plans/lez-v04-migration.md). There is no second supported legacy platform. See [repository layout](repository-layout.md) for package roles and [ADR 0034](../adr/0034-lez-v04-migration.md) for the decision.

## Light development checks

Run `bash verify/check-dev.sh` for everyday work. It runs locked host tests for the core sale crates, compatibility support, weighted math and launchpad core logic. It uses the checked-in emergency-authority fixture when the variable is unset and omits debug symbols unless the caller selects another setting. It needs host Rust, but no RISC0 tools, node or wallet setup.

Plain `cargo test --locked` selects only the root core crates. Client and CLI tests need explicit package selection or `--workspace`. Guest builds, execution tests and package builds are manual. Light checks do not establish client, wire or runtime compatibility.

## Build contexts

Full checks use LEZ `cb075d5777cb812ba44216429d4f2837ea8518bd`, host Rust 1.98.1, guest Rust 1.94.1, RISC0 3.1.0 and zkOS wrapper 2.2.3. The [development base guide](../../packages/lez-compat/README.md) gives installation requirements. Check local tools with `bash verify/check-builds.sh --preflight`. Scaffold and SPEL CLI are not required.

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

## CI

[Root CI](../../.github/workflows/ci.yml) runs formatting, static pins, mock shell regressions and the light host tests on pushes and pull requests. Full builds, client/CLI and IDL checks, guest parity and node-free execution run only through `workflow_dispatch` with `full` enabled. The full job is manual for now. Package installation, app smoke actions, live service integration, fuzz campaigns and real proofs remain separate checks.

## Execution and evidence limits

Add `--execution` to `check-builds.sh` to select the optional node-free token, seeded sale and role-authority fixtures. The script builds the application and compatibility guests and locates the pinned upstream clock guest; `LAUNCHPAD_CLOCK_ELF` can override that path. Execution needs local processes and IPC. A sandbox rejection is not a passing result. Token-only rollback checks do not prove sale rollback.

The [sale tests](../../methods/tests/actor_sale.rs) and [role-authority tests](../../crates/launchpad-client/tests/role_authority_guest.rs) exercise signed setup, trades, controls and settlement through the guests. Fault tests check rollback and retries. Use disposable state; replacing a test fault adapter does not establish live adapter replacement.

Host tests do not prove guest execution. Local guest execution does not establish real proofs, live sequencer timing, wallet/service integration, package loading or DEX liquidity. Private routes are unsupported because compatibility token holdings are public only. See the [package README](../../packages/launchpad/README.md) for package requirements.

For stable release image IDs, the build script requires `/tmp/logos-launchpad-release/source` with `--release`. Build paths can change guest binaries and IDs; cross-OS identity remains unverified. See [verification boundaries](../adr/0022-verification-boundaries.md).

## Licenses

The curve packages use the root [MIT license](../../LICENSE). Weighted math retains its own MIT license and the Balancer comparator notices in `packages/weighted-math/crates/harness/balancer-ref/`. Third-party dependencies keep their own licenses.

## Weighted pricing clock

Normal builds use the pinned upstream clock. Weighted buys send `ReadTimestamp { granularity: 1 }` to the canonical actor. The reply contains Unix milliseconds. Pricing divides by 1000. There is no mock guest or build flag. See [ADR 0026](../adr/0026-lbp-clock-and-poke.md#upstream-clock-amendment-2026-11-09).

Actor execution fixtures use the upstream `test-utils` feature in debug host builds. LEZ rejects that feature in release builds. Guest binaries and production host checks still use release builds. Release-overflow checks keep their release profile.

Run `bash verify/check-weighted-clock.sh` for focused host and sale guest checks. Run `bash verify/check-builds.sh --execution` with the emergency-authority fixture set for the full local gate. Commands use locked dependencies. The focused tests execute the pinned clock artifact. They retain explicit stale-state fixtures as separate cases.

The clock is updated at the end of a block. A node-free `Tick` test checks the real guest's state update and a later weighted price. It also holds saved time before expiry while inclusion reaches expiry. The transaction is rejected without state or nonce changes. This does not prove sequencer ordering, timestamp trust, network operation or proof generation time.

Earlier fake-clock evidence is historical. The 9 November results below used feature-branch pin `f94ca8a7550fa0012222b9fb5990dfe44c54a3dc`. They do not establish behavior on the current `dev` pin.

A separate local native-token probe at the earlier feature-branch pin checked private funding, a public clock read and atomic private returns. This probe has not been repeated on the current `dev` pin. A changed clock reply passed when the return stayed fixed. A changed return failed with `CrossMessageMismatch` despite an acceptable minimum output. A fresh proof passed. Failed attempts preserved all state. The five-test probe used development receipts and a small payout guest. It did not run launchpad token programs or complete a launchpad private route.

Before the checkpoint merge on 9 November 2026, `bash verify/check-builds.sh --execution` passed on the pinned clock revision with unchanged lockfiles. It includes 22 sale guest tests and 31 curve, role-authority and settlement guest tests. The sale tests cover real clock reads and a real `Tick`, saved-clock and inclusion bounds, both weighted buy forms, full weighted setup and rollback. Setup rejects saved time past its bound even when inclusion would pass. The host development gate, IDL and wire checks, all shell regressions, strict host lints and formatting also passed locally. Linux CI has not run for this change. Service integration, full private routes and real proofs remain separate checks.

After the checkpoint merge on 9 November 2026, the full locked execution gate passed with unchanged lockfiles. All 28 sale guest tests and 31 curve, role-authority and settlement guest tests passed. The sale suite retains settlement and allocation rollback checks. It also checks real clock ticks, same-second checkpoint event suppression, later checkpoints, and poke at the sale end and after settlement. Host tests reject forged, malformed, block-ID and replayed poke replies. The host development gate, generated IDL and wire checks, shell regressions, formatting and strict host lints passed. These local checks use development execution. Live sequencer timing, full private routes, real proofs and deployment readiness remain unverified.

## Dev pin verification: 2026-11-16

The first check used `dev` revision `cb075d5777cb812ba44216429d4f2837ea8518bd` before the allowlist merge below. The full locked execution gate passed with unchanged lockfiles. All 28 sale guest tests and 31 curve, role-authority and settlement guest tests passed. The gate also passed release host tests, guest builds, weighted-math parity, release-overflow checks and token execution. The development gate passed 327 host tests. IDL and wire checks, all shell regressions, workspace formatting and strict workspace lints passed. Generated interfaces did not change. The merged clock still returns saved previous-block time; current inclusion-block timestamp reads are unavailable through this API. Live sequencer checks, the earlier private-return probe on this pin, real proofs and deployment readiness remain unverified.

## Main merge verification: 2026-11-16

Main commit `5e64eba` was merged with the upstream clock integration on LEZ revision `cb075d5777cb812ba44216429d4f2837ea8518bd`. The full locked gate passed all 34 sale guest tests and 31 curve, role-authority and settlement guest tests. The sale suite includes allowlisted public buys and fresh setup for all three engines. Weighted cases use the upstream clock and retain their cycle limits. The development gate passed 375 host tests. IDL and wire checks, all shell regressions, formatting and strict workspace lints passed. Verification did not change lockfiles. The generated interface matches the merged source. Live sequencer timing, private execution, real proofs and deployment readiness remain unverified.
