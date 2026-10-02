# Weighted-math qualification

Recorded on 2026-10-02, based on main `a80eb64`. This is evidence for the standalone, unaudited kernel at public scale 52 and internal scale 62, not a working LBP program.

## Accuracy

The selected target is at most one basis point (`1/10000`) of extra arithmetic loss against the independently rounded ideal trade. Fees, slippage and normal integer rounding are separate. Trades that cannot meet the target must be rejected. The [error analysis](error-analysis.md) defines the bounds; their hand derivation still needs mathematical review.

The 692 trade fixtures cover mixed decimals, large reserves, small buys and weight/amount boundaries. The policy classifies 584 as accepted, 28 as dust, 79 as accuracy-rejected and one as representation-rejected. Every executable case gets direction and error-bound checks, including rejected cases.

## Checks

- All 42 weighted tests passed in debug and release with overflow checks on and off. Formatting and Clippy passed.
- All 154 root host tests, pin checks and seven build-check regression groups passed.
- All 988 guest math evaluations matched the host, before and after the final build update. Three command tests passed in debug and release.
- Two oracle regenerations matched the fixtures byte for byte. Raising mpmath precision from 220 to 440 digits changed no trade reference or diagnostic field.

The tests include public regressions for the rounding-carry fix. Proptest emitted existing source-persistence warnings. Guest Clippy failed because its wrapper could not find guest `std`; host-only Clippy passed. No new fuzz campaign, scale sweep, root guest execution, IDL check or live end-to-end test ran.

## Real proof

One local composite proof passed within the 30-minute cap: 5.908625 seconds to prove, then 0.013822 seconds to verify. It used 11,243 kernel cycles and 65,536 total cycles. Development proving was disabled; no remote prover was used.

The machine was an Apple M3 Pro with 11 logical CPUs and 18 GiB RAM. Host Rust was 1.94.0, guest Rust was 1.94.1-dev (`06e01cb0d`), and nested RISC0 tools/dependencies were 3.0.5. Root lockfiles remain at 3.0.6.

Proved image: `8ba0ec41bd7dd3af32d861b84679967807c644d2ec95f223dcd586c58916513a`.

Final image: `3ea031f8c4b2c1c2a57290441b99a5c033348616988c1cbae1650f0db6962eae`.

Main's newer build settings changed the image ID. Final-build execution checks passed, but the original receipt does not verify the final image. No second proof was attempted.

See the [oracle guide](../crates/harness/oracle/README.md) and [zkVM guide](../zkvm/README.md) for commands. Checked LBP handlers, custody, clock integration and full-operation proof costs remain future work.
