# Workspace guide

This repository contains the imported curve PoC and weighted-math library. They are a starting point for production work, not a claim that the [accepted design](../adr/README.md) is implemented. The LBP state machine, shared core module and QML apps remain planned. See [repository layout](repository-layout.md) for each package's role.

## Build contexts

| Workspace | Contents | Test command from the repository root |
| --- | --- | --- |
| Root `Cargo.toml` | Curve math, pool, handlers, client and CLI | `cargo test --workspace --locked` |
| `packages/weighted-math/Cargo.toml` | Weighted kernel, harness and numerical fixtures | `cargo test --manifest-path packages/weighted-math/Cargo.toml --workspace --locked` |
| `methods/guest/` | Curve, factory and private-flow guest entry points | Separate guest build with `lgs build` |
| `packages/weighted-math/zkvm/` | Weighted-math host/guest parity | `cd packages/weighted-math/zkvm && cargo run --release --locked` |
| `packages/weighted-math/fuzz/` | Weighted trade invariant fuzz target | `cd packages/weighted-math/fuzz && cargo +nightly fuzz run trade_invariant` |

Root workspace commands do not include the nested math, guest or fuzz workspaces. Keep their lockfiles separate. The future LBP can depend on `packages/weighted-math/crates/weighted-math-core/` by Cargo path. No LBP package exists yet.

The root `rust-toolchain.toml` selects Rust 1.94.0. Host tests require Cargo and access to the dependencies in the lockfiles. Guest builds and parity also require the RISC0 tools; the curve build uses the `lgs` CLI. Fuzzing needs nightly Rust and `cargo-fuzz`.

## Host checks

Run from the repository root:

```sh
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --manifest-path packages/weighted-math/Cargo.toml --workspace --locked
cargo fmt --manifest-path packages/weighted-math/Cargo.toml --all -- --check
cargo clippy --manifest-path packages/weighted-math/Cargo.toml --workspace --all-targets --locked -- -D warnings
bash verify/check-pins.sh
bash verify/check-idl.sh
```

`check-pins.sh` checks the LEZ revision in the root and guest manifests against `scaffold.toml`. `check-idl.sh` requires `lgs` and checks generated on-chain IDLs against `idl-src/`. These checks do not cover the planned module `.lidl` interface.

## Known limits

Ignored tests are not passes. Client integration tests need guest artifacts and an executor; the weighted-math error-margin test is also ignored by default. The [math README](../../packages/weighted-math/README.md) describes its fixtures, bounds and separate verification tools.

Live E2E verification remains future work. Use disposable wallet and localnet state for those checks.

The imported formatter settings include nightly-only options. Stable Rust can report warnings for those options even when the formatting check passes. No source CI workflows were enabled by this import. Reproducible guest builds, CI coverage and live platform tests remain separate work.

## Licenses

The curve packages retain the root MIT and Apache-2.0 license files. Weighted math retains its own MIT license and the Balancer comparator notices in `packages/weighted-math/crates/harness/balancer-ref/`. Third-party dependencies keep their own licenses.
