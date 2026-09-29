# Weighted math

An unaudited `no_std` Rust library for weighted pricing. `weighted-math-core` provides fixed-point arithmetic, `pow`, exact-input pricing and exact-output pricing. It has no custody or sale policy. The `harness` crate holds its fixtures and tests.

Decisions use the repository's root ADR series: [integer arithmetic](../../docs/adr/0010-weighted-integer-arithmetic.md), [precision](../../docs/adr/0011-weighted-precision.md), [series](../../docs/adr/0012-weighted-series.md), [exact-output pricing](../../docs/adr/0013-weighted-exact-output.md) and [evidence](../../docs/adr/0014-weighted-math-evidence.md).

## Build and test

Run from this directory:

```sh
cargo build --workspace --locked
cargo test --workspace --locked
```

The [workspace guide](../../docs/architecture/workspace-guide.md) lists lint, fuzz and guest-parity commands. Fuzzing needs nightly Rust and `cargo-fuzz`; parity needs the RISC0 tools. These separate workspaces are not covered by the command above.

## Supporting material

- [Oracle fixtures and regeneration](crates/harness/oracle/README.md)
- [Balancer comparison and source notices](crates/harness/balancer-ref/README.md)
- [Overflow proof](docs/overflow-proof.md)
- [Error analysis](docs/error-analysis.md)
- [Historical guest cycle measurements](docs/zkvm-cycles.md)
- [Guest-parity setup](zkvm/README.md)

Licensed under the [MIT license](LICENSE).
