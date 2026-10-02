# Build evidence

## Tested environment

Checks ran on 2026-10-01 on source commit `82f627f9386175e24182b64d428e6a93ecbdd361` with the build changes in this checkout. The machine used macOS 14.4, build 23E214, arm64 and 18 GiB RAM. Apple Command Line Tools supplied clang 15.0.0 (`clang-1500.0.40.1`). Docker was not available because its daemon was stopped.

| Component | Selected version or source |
| --- | --- |
| Host Rust and Cargo | 1.94.0 |
| RISC0 guest Rust | 1.94.1 |
| rzup | 0.5.0 |
| cargo-risczero and r0vm | 3.0.5 |
| RISC0 C++ component | 2024.1.5 |
| Curve risc0-build and risc0-zkvm locks | 3.0.6 |
| Weighted-math risc0-build and risc0-zkvm locks | 3.0.5 |
| LEZ | `a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a` |
| SPEL setup reference | `0cb7e0980535af619482cf1c823f4d394b3ebd61` |
| Scaffold v0.2.0 source | `f919fd03b064a91a4f261093cf2b0b4f8150e27c` |

The scaffold source prints package version 0.1.1. It and rzup were installed from the exact references under separate local roots. Global tools were not replaced. The rzup source install succeeded with its lockfile; Cargo warned that the locked `spin` 0.9.8 version is yanked and that `num-bigint-dig` 0.8.4 has a future-compatibility warning. Scaffold installation emitted upstream `sha2` configuration warnings.

## Build and execution checks

With the tools selected as shown in the [build guide](README.md), these commands passed in the working source tree and in fresh source copies:

```sh
CARGO_BUILD_JOBS=4 bash verify/check-builds.sh
CARGO_BUILD_JOBS=4 bash verify/tests/check-builds.sh complete-build
```

Each source copy had no prior build outputs, `.scaffold` state or wallet state. Cargo downloads, Git dependency caches and installed RISC0 tools were shared. These runs check fresh source builds on one machine. They do not check a fresh operating system or container.

| Lane | Result |
| --- | --- |
| Root host, client and CLI release build | Passed |
| Root release tests | 154 passed |
| Weighted-math host release build | Passed |
| Weighted-math host tests | 35 passed; 2 doc tests passed |
| Four curve guest release builds | Passed |
| Weighted-math guest execution | All outputs matched host output |
| Curve client guest tests with development receipts | 5 passed |
| Actual host and guest release overflow tests | 2 passed |
| Release CLI `--help` in fresh source | Passed |
| Lockfile hashes before and after each full run | Unchanged |

The overflow guest first returned `u64::MAX` for `u64::MAX - 1 + 1`. Its unchecked `u64::MAX + 1` then failed with the overflow panic. Before the guest release policy was enabled, the same overflow execution returned zero. The probe is an example and is not part of normal program embedding.

The seven preflight regressions passed. Release-path rejection, refusal of an occupied lock and owned-lock cleanup after a failed stage also passed. Failed host, math and development guest test stages were detected with executable fixtures. A changed lockfile was also detected with a fixture. Separate real curve and math guest builds rejected stale nested guest locks without rewriting them.

## Guest artifact identity

```sh
CARGO_BUILD_JOBS=4 bash verify/tests/check-builds.sh repeated-build
```

Two independent fresh source copies passed the full `--release` checker after each was staged at `/tmp/logos-launchpad-release/source`. On this macOS machine, its physical path is `/private/tmp/logos-launchpad-release/source`. Outputs were removed between builds. All five packaged guest SHA-256 hashes and image IDs matched, including the factory. Each run passed 198 tests, weighted-math execution parity, CLI help and unchanged-lock checks. Downloaded dependencies and installed tools were shared.

This establishes the fixed-path workaround on the tested environment. Arbitrary-path reproduction remains unresolved. Earlier sibling-folder builds matched, but a stronger comparison with different parent folders and depths failed for the factory while the other four guests matched.

The initial arbitrary-path comparison failed because all five guest hashes differed across source paths. Inspection found absolute repository paths in the curve binaries. Adding `-Zremap-cwd-prefix=/source` made the four curve artifacts match, but the math artifact still differed. Setting the math guest release profile to `debug = 0` and `strip = "symbols"` made that artifact match too. Both guest workspaces retain `overflow-checks = true`.

The stronger comparison recorded factory hashes `7001c701e50be624f5d9a18800d70deef639a0c09bdae8ec2af0393f1038a5bd` and `f2043d1afe97a3e2c8129243f827ab42842822eb87208d456d7e98d12fb9da35`. It first stopped for lack of disk space. After approved build outputs were removed, both builds passed and the artifact comparison found this mismatch.

Two fresh factory builds at the same source and output paths matched. A shallow-path and deep-path comparison of stripped ELF files differed only in the `.text` section: 449 bytes changed, while section size and all other sections stayed the same. Disassembly showed code blocks in a different order and changed call offsets.

Two later factory-only builds retained symbols. Moving the source changed Cargo compiler metadata for local dependencies outside the guest workspace and changed Rust v0 internal function names. Six cleanup and serialization helpers moved in internal-name order. This explains the observed factory mismatch. Compiler path remapping does not change those Cargo dependency identities.

A bounded experiment added the pinned compiler's `-Zcodegen-source-order=yes` option in two temporary source copies. Both release guest builds passed. The curve, private-authority and private-buy hashes matched across paths, but the factory hashes still differed. The option was not applied to the repository. The fixed-path workaround was used instead.

Both full fixed-path release runs captured the same values below:

| Guest | SHA-256 |
| --- | --- |
| curve | `83b44028665ab13066abc4b380b8bc716aac405cf7bf5fdfeab50623d4b63ece` |
| factory | `f2043d1afe97a3e2c8129243f827ab42842822eb87208d456d7e98d12fb9da35` |
| private_authority | `4f16f5ee77b9be3489efe4cd1b04a5be022c46bd8b69246571bf86ee8e8e53ed` |
| private_buy | `f0883fe074e7224ea2c5923287c0a86e1f98b1469c6beca47021b39bd361158b` |
| weighted-math-guest | `d67b5a72f825d50ec3ad4df12a3f9ed90216a79ec54078d4aa25a60c05677589` |

Their generated image IDs were:

```text
CURVE_ID = [2058396366, 1557456673, 3450414136, 3905733366, 3191897698, 3953110956, 875526332, 881433843];
FACTORY_ID = [2817923444, 3580901875, 1987882518, 3776444177, 857841889, 226764153, 2082819035, 3392687597];
PRIVATE_AUTHORITY_ID = [2880879093, 1948288899, 2092328508, 52593632, 2863033171, 3765936065, 2167951280, 2710304];
PRIVATE_BUY_ID = [2553503892, 1696524450, 3534928473, 2658846646, 2075663863, 2061749754, 996845381, 1116902929];
WEIGHTED_MATH_GUEST_ID = [3063768234, 2975350377, 1992792472, 3794246296, 3926135782, 2124030971, 243660934, 3096448929];
```

Curve artifacts are under `methods/target/riscv-guest/launchpad_methods/launchpad_programs/riscv32im-risc0-zkvm-elf/release/`. The math artifact is under `packages/weighted-math/zkvm/target/riscv-guest/zkvm-methods/weighted-math-guest/riscv32im-risc0-zkvm-elf/release/`. These paths are relative to the release source checkout. The acceptance test removed its source copies and outputs after recording the results; it did not retain a deployment bundle. Native host binary byte equality and cross-operating-system guest identity were not tested.

## Other checks and limits

After rebasing onto `main` commit `46cdf25` on 2026-10-02, the root release suite passed 154 tests and weighted math passed 35 tests plus 2 doc tests. The small command regressions, all six workspace format checks, shell syntax, whitespace and documentation links passed. The incoming changes were documentation only. The full guest build pair was not repeated after the rebase.

Format checks passed for the root, methods, curve guest, math host and math zkVM workspaces. Shell syntax and `git diff --check` passed. Strict root and math host Clippy checks passed. Methods and math zkVM host-only Clippy checks passed with `RISC0_SKIP_BUILD=1` after real guest builds. Building guests during those Clippy checks failed because the pinned builder inherited the host Clippy wrapper and could not find the guest standard library. Guest lint is not established. The normal full checker passed again without the skip setting.

Weighted-math parity and the overflow probe use guest execution only. Curve client guest tests use development receipts. No real proofs were generated, and no programs were deployed. The private-buy test's expected full-route rejection at the pinned LEZ chained-call limit is not a successful private purchase. IDL checks, fuzzing, live platform tests and CI were not run for these changes. The SPEL reference was preserved; no separate SPEL build was run.
