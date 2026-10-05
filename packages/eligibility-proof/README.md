# Eligibility-proof research

Synthetic-only LBP allowlist research. This package has no production handler, creator service, wallet integration or on-chain allowance store. Do not use live wallet keys. See the [evidence and open gates](../../docs/research/lbp-eligibility-proof.md) and [source record](SOURCES.md).

`eligibility-core` prepares membership data and checks account control, membership, the common cap and purchase binding. Its `AllowanceBook` simulates successful and rejected trades. It does not prove chain rollback or concurrent purchase safety. `zkvm` runs the checker in real guests and verifies local receipts. Direct Succinct verification in a normal public guest exceeded LEZ's 33,554,432-cycle cap. Groth16 has only rejection evidence; its required Docker image is absent locally.

The separate native experiment checks eligibility inside a private program and chains a mock public purchase. Genuine outer proofs for both public and private approved accounts passed the pinned local LEZ transaction validator. The marker, mock purchase and zero-balance private scratch effects committed together. This is a synthetic no-funds fixture, not a deployed LBP or complete private route. Final method selection, cost acceptance and privacy review remain open.

## Reproduce

Run from the repository root. Use host Rust 1.94.0, guest Rust 1.94.1, RISC0 tools 3.0.5 and C++ 2024.1.5. Set PATH for these commands; do not change the user's tool configuration. The three lockfiles are separate from the root and weighted-math locks. This package pins SDK and build library 3.0.6, matching the root SDK. Installed build/executor tools remain 3.0.5. Native compatibility evidence is limited to the synthetic experiment below.

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo +1.94.0 test --manifest-path packages/eligibility-proof/Cargo.toml --locked
RISC0_BUILD_LOCKED=1 RISC0_SKIP_BUILD= cargo +1.94.0 test --release --manifest-path packages/eligibility-proof/zkvm/Cargo.toml --locked
RISC0_BUILD_LOCKED=1 RISC0_SKIP_BUILD= cargo +1.94.0 build --release --manifest-path packages/eligibility-proof/zkvm/Cargo.toml --locked
/usr/bin/time -l packages/eligibility-proof/zkvm/target/release/eligibility-host execute sha256
/usr/bin/time -l packages/eligibility-proof/zkvm/target/release/eligibility-host execute poseidon
```

The runner creates one public account and 99,999 regular private account IDs from published test keys. It uses predictable synthetic blinding. Each tree has depth 17, 100,000 approved entries and 31,072 padding leaves. It checks one purchase for each account type. It prints costs and image IDs, not keys, account IDs, paths or blinding data. The creator's ID-collection cost and buyer account creation are not measured.

Proofs are opt-in, local and real. The command rejects development proving and refuses any set `RISC0_WITGEN_DEBUG`, `RISC0_DUMP_PATH`, `RISC0_PPROF_OUT` or `RUST_LOG`, including empty values. These guards run before fixture preparation. `RISC0_SERVER_PATH`, if set, must point to a trusted local `r0vm` executable. Run one proof at a time with an explicit wall-time and memory bound. The continuation uses 1,800 seconds as a research bound, not a product latency limit. Use an existing local timeout tool; `gtimeout` is available on the measured macOS host. These commands enforce time only; add memory supervision before a public proof. The measured runs used a prebuilt test executable, while the Cargo commands below also include any rebuild time.

```sh
RISC0_DEV_MODE=0 /usr/bin/time -l gtimeout --signal=TERM --kill-after=10s 1800s packages/eligibility-proof/zkvm/target/release/eligibility-host prove sha256 private composite
RISC0_DEV_MODE=0 /usr/bin/time -l gtimeout --signal=TERM --kill-after=10s 1800s packages/eligibility-proof/zkvm/target/release/eligibility-host prove sha256 private succinct
RISC0_BUILD_LOCKED=1 RISC0_SKIP_BUILD= RISC0_DEV_MODE=0 gtimeout --signal=TERM --kill-after=10s 1800s cargo +1.94.0 test --release --manifest-path packages/eligibility-proof/zkvm/Cargo.toml --locked --test execution real_private_receipt_binds_image_journal_and_purchase -- --ignored
RISC0_BUILD_LOCKED=1 RISC0_SKIP_BUILD= cargo +1.94.0 test --release --manifest-path packages/eligibility-proof/zkvm/Cargo.toml --locked --test execution guest_checks_large_creator_lists_with_both_account_types_and_hashes -- --ignored
```

`prove` also accepts `public` and `poseidon`; check the report for completed cases. Never generate 100,000 proofs. The runner does not export receipts or witnesses, but SDK proving writes witness-bearing segment files. The host creates their directory with owner-only `0700` permissions, reports aggregate file counts and size, and removes it after normal success. Error paths use normal temporary-directory cleanup. Signals, crashes and forced termination can leave protected files. Cleanup is not secure erasure. Receipt verification and literal leak checks do not establish mathematical zero knowledge or side-channel safety.

The positive Succinct public-guest experiment is a known failing research gate. It first verifies the genuine receipt on the host and checks selected literal leaks, then fails at the exact LEZ cycle limit. Do not treat it as a passing target-stack test or raise the limit to make it pass. Reproduce it separately from successful proof checks:

```sh
RISC0_BUILD_LOCKED=1 RISC0_SKIP_BUILD= RISC0_DEV_MODE=0 gtimeout --signal=TERM --kill-after=10s 1800s cargo +1.94.0 test --release --manifest-path packages/eligibility-proof/zkvm/Cargo.toml --locked --test execution normal_public_guest_verifies_a_real_private_succinct_receipt -- --ignored --exact --nocapture
```

## Native experiment

Native reproduction uses integration tests, not a `native-prove` CLI command. These commands use the measured debug host profile and real release guests. Run the large-list execution check first. Then run one local proof at a time. The native runner enforces a 1,800-second worker wall limit and a 12 GiB sampled child-process-group RSS limit.

```sh
RISC0_BUILD_LOCKED=1 RISC0_SKIP_BUILD= cargo +1.94.0 test --manifest-path packages/eligibility-proof/zkvm/Cargo.toml --locked --offline -p eligibility-host --test native_execution native_guests_execute_depth17_lists_for_both_account_types -- --ignored --exact --nocapture
RISC0_BUILD_LOCKED=1 RISC0_SKIP_BUILD= RISC0_DEV_MODE=0 RISC0_PROVER=ipc ELIGIBILITY_NATIVE_ACCOUNT=private ELIGIBILITY_NATIVE_OUTER_ARTIFACT="$PWD/packages/eligibility-proof/zkvm/target/native-private-outer.receipt" cargo +1.94.0 test --manifest-path packages/eligibility-proof/zkvm/Cargo.toml --locked --offline -p eligibility-host --test native native_proof_commits_marker_and_mock_purchase_together -- --ignored --exact --nocapture
```

The optional artifact export uses a new owner-only `0600` file and refuses to overwrite one. It contains only the final outer receipt and aggregate measurements. Do not publish native inner receipts or journals: they contain the witness. The final outer artifact can be reused for the checks below without proving again. Optionally set `ELIGIBILITY_NATIVE_OUTPUT_LOG` to the captured proof-command output to scan it too. Set `ELIGIBILITY_NATIVE_ACCOUNT=public` and use a separate `native-public-outer.receipt` path for a public-approved-account trial. Check the report for completed proof cases; do not retry a failed gate automatically.

```sh
RISC0_BUILD_LOCKED=1 RISC0_SKIP_BUILD= RISC0_DEV_MODE=0 RISC0_PROVER=ipc ELIGIBILITY_NATIVE_ACCOUNT=private ELIGIBILITY_NATIVE_OUTER_ARTIFACT="$PWD/packages/eligibility-proof/zkvm/target/native-private-outer.receipt" cargo +1.94.0 test --manifest-path packages/eligibility-proof/zkvm/Cargo.toml --locked --offline -p eligibility-host --test native native_outer_transaction_omits_known_private_input_literals -- --ignored --exact --nocapture
RISC0_BUILD_LOCKED=1 RISC0_SKIP_BUILD= RISC0_DEV_MODE=0 RISC0_PROVER=ipc ELIGIBILITY_NATIVE_ACCOUNT=private ELIGIBILITY_NATIVE_OUTER_ARTIFACT="$PWD/packages/eligibility-proof/zkvm/target/native-private-outer.receipt" cargo +1.94.0 test --manifest-path packages/eligibility-proof/zkvm/Cargo.toml --locked --offline -p eligibility-host --test native native_validator_rejects_tampering_stale_state_and_replay -- --ignored --exact --nocapture
```

The runner pins the installed local prover digest, refuses remote and development proving, uses child-only protected temporary storage, disables child core dumps and samples permissions every two seconds. Success verifies subtree removal. Error cleanup is scoped to the parent; it does not cover a parent crash. Sampling does not establish that all transitive writes are protected. See [SOURCES.md](SOURCES.md#local-prover-provenance). The serialized native transaction includes its message, witness set and outer proof, but excludes block overhead. No sequencer block was submitted. Literal scans and replay checks do not establish complete privacy, production allowance enforcement or concurrent buying.

## Build and lint checks

Leave `RISC0_SKIP_BUILD` empty for real guest builds. Any nonempty value, including `0`, skips the build. The public verifier pins the eligibility image in `zkvm/methods/guest/src/trusted_image.rs`. When the eligibility guest changes, rebuild, inspect `ELIGIBILITY_GUEST_ID` in `zkvm/target/release/build/eligibility-methods-*/out/methods.rs`, review the pin change and rebuild again. Host execution rejects a stale pin. Never select the trusted image from a supplied receipt.

```sh
cargo +1.94.0 fmt --manifest-path packages/eligibility-proof/Cargo.toml --all --check
cargo +1.94.0 fmt --manifest-path packages/eligibility-proof/zkvm/Cargo.toml --all --check
cargo +1.94.0 fmt --manifest-path packages/eligibility-proof/zkvm/methods/guest/Cargo.toml --all --check
cargo +1.94.0 clippy --manifest-path packages/eligibility-proof/Cargo.toml --locked --all-targets -- -D warnings
RISC0_BUILD_LOCKED=1 RISC0_SKIP_BUILD=1 cargo +1.94.0 clippy --manifest-path packages/eligibility-proof/zkvm/Cargo.toml --locked --all-targets -- -D warnings
cargo +1.94.0 clippy --manifest-path packages/eligibility-proof/zkvm/methods/guest/Cargo.toml --locked --all-targets -- -D warnings
```

The host Clippy command skips guest building because its stable host wrapper cannot compile the custom RISC0 target. The last command lints guest source on the host target. Neither is guest execution evidence. Run the real guest tests with an empty skip variable afterward. This research package is not added to the root verification gate.
