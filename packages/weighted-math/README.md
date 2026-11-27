# Weighted math

An unaudited `no_std` Rust library for weighted pricing. `weighted-math-core` provides fixed-point arithmetic, `pow`, exact-input pricing and exact-output pricing. It has no custody or sale policy. The `harness` crate holds its fixtures and tests.

Decisions use the repository's root ADR series: [integer arithmetic](../../docs/adr/0010-weighted-integer-arithmetic.md), [precision](../../docs/adr/0011-weighted-precision.md), [series](../../docs/adr/0012-weighted-series.md), [exact-output pricing](../../docs/adr/0013-weighted-exact-output.md) and [evidence](../../docs/adr/0014-weighted-math-evidence.md).

## Build and test

Use Rust 1.94 or later. Run from this directory:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
```

The test suite includes `measure_error_margins`. All-features checks do not sweep math scales; this package has no scale features.

The [workspace guide](../../docs/architecture/workspace-guide.md) lists fuzz and guest-parity commands. Fuzzing needs nightly Rust and `cargo-fuzz`; parity needs the RISC0 tools. These separate workspaces are not covered by the commands above.

## Supporting material

- [Oracle fixtures and regeneration](crates/harness/oracle/README.md)
- [Balancer comparison and source notices](crates/harness/balancer-ref/README.md)
- [Overflow analysis and checked-fit requirements](docs/overflow-proof.md)
- [Error bounds and trade-relative guards](docs/error-analysis.md)
- [Qualification checks and limits](docs/qualification.md)
- [Historical guest cycle measurements](docs/zkvm-cycles.md)
- [Guest-parity setup](zkvm/README.md)

Licensed under the [MIT license](LICENSE).
