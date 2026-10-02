# Weighted-math qualification evidence

Recorded on 2026-10-02. This work started from remote main `46cdf253ae134ccf9052d3d35d55c113bc4e70c9`. It qualifies the standalone, unaudited kernel at public scale 52 and internal scale 62. It specifies future LBP behavior in the [LBP contract](../../../docs/product/lbp-contract.md); it does not implement that program.

## Scope and numerical policy

The raw kernel uses `u128` reserves and amounts, raw weights in `[1,2^64]` and ratios in `[1/99,99]`. Exact output has a 30% reserve cap. That cap limits its domain; it does not guarantee accuracy or payment fit. Raw calls intentionally panic on invalid or unrepresentable inputs. Checked quotes, stable program errors, custody, clock access and lifecycle handlers remain future work.

The internally selected allowance is one basis point (`1/10000`) of additional arithmetic loss against the independently rounded ideal payout or payment. This is a product tolerance, not a result derived from mathematics or a universal one-raw-unit promise. Fees, participant slippage and natural integer rounding are separate. The [error analysis](error-analysis.md) derives conservative rational guards, including equal-weight tightening. The hand-derived unpadded power bound is below 400 internal units; the combined kernel-plus-pad allowance is 912 internal units. These bounds still need mathematical review. They are not machine-checked proofs or an audit.

Every executable trade fixture gets both a direction and an error-bound check. The harness then classifies it against the policy without skipping large sensitivities or saturating tolerances. This classification does not execute a checked LBP handler.

| Buy form | Total | Accepted | Dust | Accuracy rejected | Representation rejected |
| --- | ---: | ---: | ---: | ---: | ---: |
| Exact input | 351 | 293 | 26 | 32 | 0 |
| Exact output | 341 | 291 | 2 | 47 | 1 |

Rejected cases stay in numerical and guest-parity checks when the raw call is executable. Expected-panic cases are separate host regressions. In particular, exponent 99 at 30% output is ill-conditioned: the nominal pad alone can add about 31.6% to payment. The accuracy guard rejects it. The representation count checks post-buy pricing collateral fit; future handlers must also check actual vault additions, fees, counters and all intermediates before calling the raw kernel.

## Independent references and coverage

The generator uses Python 3.10.1 and mpmath 1.3.0 at 220 decimal digits, with exact `Fraction` arithmetic for integer exponents. Two independent temporary regenerations matched all eight committed generator outputs byte for byte. A 440-digit run matched every reference and diagnostic field in all 351 exact-input and 341 exact-output cases. References do not call the Rust kernel. Equal-weight references are exact rational floors and ceilings. Near-`2^64` unequal weights use high-precision powers of their rational ratios; they are not sent to the small-integer BigInt invariant referee as huge exponents.

The expanded grid covers all 25 combinations of 0, 6, 8, 18 and 24 token decimals. Reserves are one billion whole units of each token, up to `10^33` raw units; buys are 1, 100 and 10000 whole units. Other cases cover ratio endpoints, unequal raw weights near `2^64`, equal-weight boundaries, dust and `u128` edges. A finite grid does not establish universal accuracy, and not every grid case is accepted.

The unchanged primitive fixtures contain 87 power cases, 23 logarithm/exponential cases, 12 arithmetic cases and six scale-grid records. Historical Balancer captures remain an independent grader check, not the kernel's price reference.

## TDD regressions and host checks

Tests use the existing public Fixed/arithmetic and pricing functions, and the host command interface. No private helper or new test-only public interface was introduced.

- Public `Fixed::mul_up` with operands `2^90-1` and `2^90+1` failed to panic with the old unchecked ceiling carry in release without overflow checks. The product is `2^180-1`; its scale-52 ceiling is `2^128`. Checked addition now rejects it instead of returning zero.
- A public exact-output regression also reproduced that ceiling carry. Its old unchecked result was zero. The checked helper now panics. This is raw-kernel reachability, not a valid settled sale: resulting collateral also cannot fit. See the [overflow analysis](overflow-proof.md).
- Each large-weight, equal-weight-boundary and mixed-decimal coverage test first failed because its fixture zone had no cases. Independent fixtures then made it pass. These red runs identified missing coverage, not new pricing defects.
- The host command tests first failed for unknown arguments, development mode and extra arguments. They now reject those inputs before starting a prover. They do not mock or generate a real proof.

The weighted workspace passed 40 tests and two doctests in all four configurations: debug and release, each with overflow checks enabled and disabled through explicit `RUSTFLAGS` and `RUSTDOCFLAGS`. Formatting and all-target/all-feature Clippy with warnings denied passed. Its power diagnostic reported zero wrong-side cases, upward/downward maximum error two public units against a four-unit fixture bound. The reserve-normalized payout diagnostic was 1,084,410 reserve-ulps; that diagnostic is not the trade-relative policy. Property tests retain their bounded random domains and exact small-integer invariant referee. No new long fuzz campaign or compile-time scale sweep was run.

The root workspace host tests reported 154 passed, zero failed and zero ignored. Root formatting, all-target/all-feature Clippy with warnings denied, and `verify/check-pins.sh` passed. Root source and lockfiles were unchanged. These host checks do not establish platform compatibility.

The zkVM command tests passed in debug and release. Recursive guest Clippy failed because its wrapper could not locate guest `std`. Host-scoped Clippy passed with `RISC0_SKIP_BUILD=1`; a real guest rebuild followed with that skip unset. Guest lint is therefore not claimed as passing. The executor and proof results below used the rebuilt guest, not a skipped or development guest.

## Selected tools and machine

| Item | Recorded value |
| --- | --- |
| Host Rust | 1.94.0 |
| Guest Rust | 1.94.1-dev, commit `06e01cb0d` |
| `cargo-risczero`, `r0vm`, nested RISC0 lockfiles | 3.0.5 |
| Root RISC0 lockfiles | 3.0.6, unchanged |
| Machine | Apple M3 Pro, 11 logical CPUs, 18 GiB RAM |
| OS | Darwin 23.4.0, arm64 |

Different lockfile selections are recorded explicitly. The standalone nested workspace does not prove root 3.0.6 or LEZ compatibility.

## Guest execution

Executor-only parity passed for 988 math evaluations: 351 exact-input, 341 exact-output, 87 each of `pow`, `pow_up` and `pow_down`, 11 logarithm, 12 exponential and 12 complement calls. Six division and 16 baseline cases also passed. All outputs were bit-identical to the host. Dust and accuracy-rejected executable cases were included.

| Operation | Net median kernel cycles | Single-call user cycles | Padded single-call total cycles |
| --- | ---: | ---: | ---: |
| Power | 8010 | 12191 | 65536 |
| Exact input | 10939 | 15119 | 65536 |
| Exact output | 13346 | 17526 | 65536 |

Net medians subtract the 98-cycle baseline. Session totals include guest setup and paging. These are pure math measurements. They contain no LBP authority, account validation, clock, fees, custody or settlement. They are not full-operation cost or proof-time guarantees.

## One real local proof

Exactly one real attempt ran, after setup and builds, with a 30-minute limit. It used local IPC, disabled development proving and verification, rejected Fake receipts, verified the guest image ID and compared the verified journal with the host output. No remote or paid prover was used.

From the repository root, the recorded command was:

```sh
RISC0_DEV_MODE=0 RISC0_SERVER_PATH=/Users/yvesfracari/.cargo/bin/r0vm gtimeout --signal=KILL 1800s /tmp/log-32-zkvm-target/release/zkvm-host --prove-buy
```

| Result | Recorded value |
| --- | --- |
| Exit | 0; proof and journal verification passed |
| Operation | `calc_out_given_in` |
| Inputs in public argument order | `[1000000000000, 4611686018427387904, 1000000000000000000000000, 1152921504606846976, 100000000]` |
| Host and verified guest output | `399900019995567609920` |
| Receipt | Composite |
| Image ID | `8ba0ec41bd7dd3af32d861b84679967807c644d2ec95f223dcd586c58916513a` |
| Prove time | 5.908625 seconds, including IPC and prover receipt-integrity check |
| Separate verify time | 0.013822 seconds |
| Kernel cycles | 11243 |
| User cycles | 15325 |
| Paging cycles | 28919 |
| Reserved cycles | 21292 |
| Total cycles / segments | 65536 / 1 |

The final guest rebuilt after comment-only source edits and retained exactly this image ID. Executor parity passed again. No second proof attempt was made. Build and setup time were outside the proof budget. This one-run result proves the selected guest math computation, not a working LBP, numerical truth for all inputs, full LEZ compatibility or production performance.

## Reproduce and remaining checks

Use the [oracle guide](../crates/harness/oracle/README.md) for fixture generation and the [zkVM guide](../zkvm/README.md) for tool versions, parity and opt-in proof commands. Regular host tests do not prove anything. Rerunning the real-proof command is optional and separate from the default suite.

Run from the repository root:

```sh
RUSTFLAGS='-C overflow-checks=yes' RUSTDOCFLAGS='-C overflow-checks=yes' cargo +1.94.0 test --manifest-path packages/weighted-math/Cargo.toml --workspace --locked
RUSTFLAGS='-C overflow-checks=no' RUSTDOCFLAGS='-C overflow-checks=no' cargo +1.94.0 test --manifest-path packages/weighted-math/Cargo.toml --workspace --locked
RUSTFLAGS='-C overflow-checks=yes' RUSTDOCFLAGS='-C overflow-checks=yes' cargo +1.94.0 test --release --manifest-path packages/weighted-math/Cargo.toml --workspace --locked
RUSTFLAGS='-C overflow-checks=no' RUSTDOCFLAGS='-C overflow-checks=no' cargo +1.94.0 test --release --manifest-path packages/weighted-math/Cargo.toml --workspace --locked
cargo +1.94.0 test --workspace --locked
```

Record overflow-check configuration when repeating release tests. Root guest execution, program IDL checks and live end-to-end tests were not run for this change. Full LBP-operation execution/proving measurements remain later work. Clock/token/authority verification, checked handlers, private-route integration, allowlist proof design, mathematical review and Logos review of the existing fee departures remain open.
