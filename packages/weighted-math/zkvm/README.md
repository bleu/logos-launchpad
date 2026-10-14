# RISC0 guest checks

This is a separate Cargo workspace. The kernel and independent host harness use Rust 1.94.0 without RISC0 dependencies. This workspace builds the same kernel as a guest. It checks host/guest bit parity and guest cycles. These checks do not establish full weighted-sale program or LEZ compatibility. The kernel remains unaudited.

The independent mpmath oracle and numerical checks are in `../crates/harness`. Matching host and guest results do not replace those checks. See the [qualification and historical proof evidence](../docs/qualification.md) and the [historical cycle measurements](../docs/zkvm-cycles.md).

## Layout

- `methods/guest` reads the word protocol, calls the kernel and commits each result and its cycle count. Its release profile enables overflow checks.
- `methods` builds and embeds the guest ELF and image ID.
- `host` runs fixture parity and cycle checks. It exits with an error on a mismatch.

The guest protocol is unchanged: input starts with the operation and case count, then four little-endian `u32` words per `u128` operand. Each journal record has a 16-byte little-endian result and an 8-byte little-endian cycle count.

## Tools

The nested lockfiles select RISC0 3.0.5. The checked local tools are `cargo-risczero` 3.0.5, `r0vm` 3.0.5 and the `risc0` Rust toolchain reporting `1.94.1-dev (06e01cb0d 2026-04-09)`. Use host Rust 1.94.0. The root lockfiles resolve RISC0 3.0.6; this workspace does not change them or prove compatibility between these selections.

Install missing tools with the [RISC0 installation guide](https://dev.risczero.com/api/zkvm/install). Select the recorded versions rather than an unpinned latest release. Verify them before recording evidence:

Use the tool versions in [`verify/build-tools.env`](../../../verify/build-tools.env). With the pinned rzup installed, run from the repository root:

```sh
source verify/build-tools.env
rzup install rust "$RISC0_RUST_VERSION"
rzup install cpp "$RISC0_CPP_VERSION"
rzup install r0vm "$RISC0_TOOLS_VERSION"
rzup install cargo-risczero "$RISC0_TOOLS_VERSION"
```

Check the selected tools before recording evidence:

```sh
cargo risczero --version
r0vm --version
rustup run risc0 rustc --version
rustc +1.94.0 --version
```

## Build and test

Run from the repository root. Set the path to the installed local `r0vm` if it is not on `PATH`. These example commands place build files outside the checkout.

```sh
export CARGO_TARGET_DIR="${TMPDIR:-/tmp}/weighted-math-zkvm"
export RISC0_SERVER_PATH="$HOME/.cargo/bin/r0vm"
RISC0_BUILD_LOCKED=1 cargo +1.94.0 test --manifest-path packages/weighted-math/zkvm/Cargo.toml --locked -p zkvm-host
RISC0_BUILD_LOCKED=1 cargo +1.94.0 build --release --manifest-path packages/weighted-math/zkvm/Cargo.toml --locked -p zkvm-host
```

The command test rejects unknown arguments before guest execution. Regular tests do not generate a proof.

## Guest parity

```sh
RISC0_EXECUTOR=ipc "$CARGO_TARGET_DIR/release/zkvm-host"
```

The default command executes each fixture in the guest and compares it with a host kernel call. It prints per-call and whole-session cycles. Padded executor segment cycles describe proof workload, not measured proof time. This command creates no proof.

The [qualification record](../docs/qualification.md) retains the historical fixed-input proof result. It does not verify the current guest image and is not a supported proof workflow.
