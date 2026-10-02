# RISC0 guest checks and local proof

This is a separate Cargo workspace. The kernel and independent host harness use Rust 1.94.0 without RISC0 dependencies. This workspace builds the same kernel as a guest. It checks host/guest bit parity and guest cycles. An explicit command also proves one weighted buy. These checks do not establish full LBP program or LEZ compatibility. The kernel remains unaudited.

The independent mpmath oracle and numerical checks are in `../crates/harness`. Matching host and guest results do not replace those checks. See [current qualification and proof evidence](../docs/qualification.md) and the [historical cycle measurements](../docs/zkvm-cycles.md).

## Layout

- `methods/guest` reads the word protocol, calls the kernel and commits each result and its cycle count. Its release profile enables overflow checks.
- `methods` builds and embeds the guest ELF and image ID.
- `host` runs fixture parity and cycle checks by default. It exits with an error on a mismatch. `--prove-buy` instead makes one local composite proof and verifies it.

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

The command tests reject unknown arguments, extra arguments and development proving before a prover starts. Regular tests do not generate a proof.

## Guest parity

```sh
RISC0_EXECUTOR=ipc "$CARGO_TARGET_DIR/release/zkvm-host"
```

The default command executes each fixture in the guest and compares it with a host kernel call. It prints per-call and whole-session cycles. Padded executor segment cycles describe proof workload, not measured proof time. This command creates no proof.

## One real proof

Build before starting the time budget. With GNU `timeout` installed as `gtimeout` on macOS:

```sh
unset RISC0_DEV_MODE
gtimeout --signal=KILL 1800s "$CARGO_TARGET_DIR/release/zkvm-host" --prove-buy
```

The command accepts no extra arguments. It requires `RISC0_DEV_MODE` to be unset or exactly `0`. It uses a local `r0vm` subprocess directly, so `RISC0_PROVER` and Bonsai settings cannot select a remote prover. It disables development proving and verification, rejects Fake receipts, checks the guest image ID and compares the verified journal result with the host result.

The example has a collateral reserve of `10^12`, launch-token reserve of `10^24`, weights `(4 * 2^60, 2^60)` and collateral input `10^8`. With collateral decimals 6 and launch-token decimals 18, this is a 100-token buy against one million tokens in each reserve. The proof covers this pure weighted-math call. It contains no fees, custody, clock, authority or LBP lifecycle handlers.

The output records the inputs, host result, image ID, receipt kind, journal result, kernel cycles and session cycles. `prove_seconds` includes local IPC and the prover's receipt-integrity check. `verify_seconds` measures the separate image-ID and journal verification. Record a failed or timed-out run as such. Development receipts and executor-only runs do not count as real proofs.

On 2026-10-02, the local attempt on an Apple M3 Pro with 11 logical CPUs and 18 GiB RAM succeeded within the 30-minute cap. The composite proof took 5.908625 seconds; the separate verification took 0.013822 seconds. The verified output was `399900019995567609920`. The image ID was `8ba0ec41bd7dd3af32d861b84679967807c644d2ec95f223dcd586c58916513a`. The call used 11,243 kernel cycles and 15,325 whole-guest user cycles. Its one segment had 65,536 total cycles, including 28,919 paging cycles and 21,292 reserved cycles. These are one-run kernel measurements, not full LBP-handler costs or performance guarantees.
