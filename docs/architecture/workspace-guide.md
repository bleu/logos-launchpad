# Workspace guide

This repository contains the imported curve PoC and weighted-math library. They are a starting point for production work, not a claim that the [accepted design](../adr/README.md) is implemented. See [repository layout](repository-layout.md) for each package's role.

## Build contexts

| Workspace | Contents | Command from the repository root |
| --- | --- | --- |
| Root `Cargo.toml` | Curve math, pool, handlers, client and CLI | `cargo test --workspace --locked` |
| `packages/weighted-math/Cargo.toml` | Weighted kernel, harness and numerical fixtures | `cargo test --manifest-path packages/weighted-math/Cargo.toml --workspace --locked` |
| Launchpad core workspaces | Separate logic and native adapter workspaces under `packages/launchpad/core/` | See the [package README](../../packages/launchpad/README.md) for setup and package commands |
| `methods/guest/` | Curve, factory and private-flow guest entry points | `RISC0_BUILD_LOCKED=1 cargo build --manifest-path methods/Cargo.toml --release --locked` |
| `packages/weighted-math/zkvm/` | Weighted-math host/guest parity | `RISC0_BUILD_LOCKED=1 cargo run --manifest-path packages/weighted-math/zkvm/Cargo.toml --release --locked` |
| `packages/weighted-math/fuzz/` | Weighted trade invariant fuzz target | `cd packages/weighted-math/fuzz && cargo +nightly fuzz run trade_invariant` |

Root workspace commands do not include the nested math, launchpad, guest or fuzz workspaces. Keep their lockfiles separate. The future LBP can depend on `packages/weighted-math/crates/weighted-math-core/` by Cargo path. No LBP package exists yet.

The root and weighted-math host workspaces declare edition 2024 and a minimum Rust version of 1.94. The root and math zkVM `rust-toolchain.toml` files select Rust 1.94.0. Guest builds and parity also require the tool versions in [`verify/build-tools.env`](../../verify/build-tools.env). Check setup with `bash verify/check-builds.sh --preflight`. Fuzzing needs nightly Rust and `cargo-fuzz`.

## Host checks

Run these curve and weighted-math checks from the repository root:

```sh
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --manifest-path packages/weighted-math/Cargo.toml --workspace --locked
cargo fmt --manifest-path packages/weighted-math/Cargo.toml --all -- --check
cargo clippy --manifest-path packages/weighted-math/Cargo.toml --workspace --all-targets --all-features --locked -- -D warnings
bash verify/check-pins.sh
bash verify/check-idl.sh
```

`check-pins.sh` checks the LEZ revision in the root and guest manifests against `scaffold.toml`. `check-idl.sh` requires `lgs`, checks generated on-chain IDLs against `idl-src/`, and runs the non-default `idl-tests` lane. Normal host tests do not require `lgs` or wallet state.

## Guest integration tests

Client tests that execute guest programs use the non-default `guest-tests` feature. Build the guest artifacts with `RISC0_BUILD_LOCKED=1 cargo build --manifest-path methods/Cargo.toml --release --locked`. The nested guest lock needs `RISC0_BUILD_LOCKED=1`; outer Cargo `--locked` alone does not lock it. The `r0vm` executable must be on `PATH`, or `RISC0_SERVER_PATH` must point to it. Then run:

```sh
RISC0_DEV_MODE=1 cargo test -p launchpad-client --release --features guest-tests --test private_authority --test settlement --locked
```

This lane checks private-authority, private-buy and settlement execution. Missing artifacts are failures when the lane is selected. Host-only tests do not execute it. All-feature clippy compiles these test targets without running their guests.

## Known limits

A passing host suite does not prove guest execution. The math margin test runs with the host suite; the [math README](../../packages/weighted-math/README.md) describes fixtures, bounds and separate verification tools.

Public enum variants must remain in source-IDL order. SDK-only private wrappers follow the public variants so they do not change public tags. The IDL uses the pinned serializer's supported primitive wire values: fees use `u32`, depletion uses `Option<u8>`, and owner-program ID and seed use paired optional fields. Generated CLI calls require explicit PDA addresses; the SDK computes them with its existing helpers. The trailing treasury account is a remaining-account vector and may be omitted when it aliases an earlier account. `check-idl.sh` verifies generated artifacts and compares CLI dry-run bytes and account metadata with `Program::serialize_instruction`. An omitted trailing Treasury account appears as null rest metadata in a dry run and is absent from the CLI transaction account vector. This repository does not support the old pre-launch encoding.

The imported private-buy route exceeds the pinned LEZ chained-call limit when executed as a complete graph. Its guest test checks the router's instruction envelope and records this full-route rejection; it does not prove a successful private purchase. The router also deshields the full collateral cap and only requests re-shielding of purchased tokens. Its call graph leaves unspent collateral in the transient public ATA, and the SDK has no automatic remainder recovery. These route limits require separate work; the IDL correction does not remove them.

Live E2E verification remains future work. Use disposable wallet and localnet state for those checks.

No source CI workflows are enabled. Run `bash verify/check-builds.sh` for locked host and guest builds, tests and release overflow checks. For stable guest IDs, use a committed checkout at `/tmp/logos-launchpad-release/source` and run `bash verify/check-builds.sh --release`. Keep its guest binaries and image IDs. Builds from other paths can change the factory ID; cross-OS identity is unverified. CI, live platform tests and real-proof generation remain separate work.

## Licenses

The curve packages use the root [MIT license](../../LICENSE). Weighted math retains its own MIT license and the Balancer comparator notices in `packages/weighted-math/crates/harness/balancer-ref/`. Third-party dependencies keep their own licenses.
