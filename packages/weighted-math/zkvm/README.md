# RISC0 guest build and cycle measurement

This is a separate Cargo workspace. Both math host workspaces select Rust 1.94.0. Math host tests do not compile RISC0 crates. No CI workflows are enabled.

The host harness checks correctness against reference values from mpmath. This workspace checks that the unchanged weighted-math kernel gives the same output on the host and in the guest. It also measures guest cycles. See [cycle measurements](../docs/zkvm-cycles.md).

## Layout

- `methods/guest` reads `u32` words, calls the kernel and commits the result and cycle count to the journal. Its own release profile enables overflow checks, so the cycle counts include that cost.
- `methods` builds and embeds the guest with `risc0-build`.
- `host` loads the harness fixtures, executes the guest and compares every output with a direct host call. It exits with an error if any output differs.

## Prerequisites

Use the exact tools and setup commands in the repository [build guide](../../../verify/README.md). The tool pins are in `verify/build-tools.env`. Keep this workspace and the guest workspace's lockfiles separate. Do not use a moving `rzup install` default.

## Run

From the repository root:

```sh
RISC0_BUILD_LOCKED=1 cargo run --manifest-path packages/weighted-math/zkvm/Cargo.toml --release --locked
```

Both flags are needed: Cargo locks the host build, and `RISC0_BUILD_LOCKED=1` locks the nested guest build. A passing run prints `all outputs bit-identical host vs guest`. For release artifacts, use the fixed source checkout and full `--release` check in the [build guide](../../../verify/README.md).

This runs the executor only. It does not generate a proof. See the repository [build evidence](../../../verify/build-evidence.md) for the tested environment and results.
