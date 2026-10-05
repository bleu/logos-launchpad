# Eligibility-proof research

Synthetic-only LBP allowlist research. This package has no production handler, creator service, wallet integration or on-chain allowance store. Do not use live wallet keys. See the [evidence and open gates](../../docs/research/lbp-eligibility-proof.md) and [source record](SOURCES.md).

`eligibility-core` prepares membership data and checks account control, membership, the common cap and purchase binding. Its `AllowanceBook` simulates successful and rejected trades. It does not prove chain rollback or concurrent purchase safety. `zkvm` runs the checker in a real guest and verifies local receipts. A second normal guest checks Groth16 receipts directly, without program assumptions; only rejection tests have passed for that guest.

## Reproduce

Run from the repository root. Use host Rust 1.94.0, guest Rust 1.94.1, RISC0 tools 3.0.5 and C++ 2024.1.5. Set PATH for these commands; do not change the user's tool configuration. The three lockfiles are separate from the root and weighted-math locks. This package pins SDK 3.0.5; the root resolves 3.0.6. That difference remains a platform integration gate.

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo +1.94.0 test --manifest-path packages/eligibility-proof/Cargo.toml --locked
RISC0_BUILD_LOCKED=1 RISC0_SKIP_BUILD= cargo +1.94.0 test --release --manifest-path packages/eligibility-proof/zkvm/Cargo.toml --locked
RISC0_BUILD_LOCKED=1 RISC0_SKIP_BUILD= cargo +1.94.0 build --release --manifest-path packages/eligibility-proof/zkvm/Cargo.toml --locked
/usr/bin/time -l packages/eligibility-proof/zkvm/target/release/eligibility-host execute sha256
/usr/bin/time -l packages/eligibility-proof/zkvm/target/release/eligibility-host execute poseidon
```

The runner creates one public account and 99,999 regular private account IDs from published test keys. It uses predictable synthetic blinding. Each tree has depth 17, 100,000 approved entries and 31,072 padding leaves. It checks one purchase for each account type. It prints costs and image IDs, not keys, account IDs, paths or blinding data. The creator's ID-collection cost and buyer account creation are not measured.

Proofs are opt-in, local and real. The command rejects development mode and never selects a remote prover. `RISC0_SERVER_PATH`, if set, must point to a trusted local `r0vm` executable. The 600-second limit is an operational bound, not a product latency limit. Install/use a local timeout tool before an expensive run. On this macOS host, `gtimeout` is already available.

```sh
RISC0_DEV_MODE=0 /usr/bin/time -l gtimeout --signal=KILL 600s packages/eligibility-proof/zkvm/target/release/eligibility-host prove sha256 private composite
RISC0_DEV_MODE=0 /usr/bin/time -l gtimeout --signal=KILL 600s packages/eligibility-proof/zkvm/target/release/eligibility-host prove sha256 private succinct
RISC0_BUILD_LOCKED=1 RISC0_SKIP_BUILD= RISC0_DEV_MODE=0 gtimeout --signal=KILL 600s cargo +1.94.0 test --release --manifest-path packages/eligibility-proof/zkvm/Cargo.toml --locked --test execution real_private_receipt_binds_image_journal_and_purchase -- --ignored
RISC0_BUILD_LOCKED=1 RISC0_SKIP_BUILD= cargo +1.94.0 test --release --manifest-path packages/eligibility-proof/zkvm/Cargo.toml --locked --test execution guest_checks_large_creator_lists_with_both_account_types_and_hashes -- --ignored
```

`prove` also accepts `public` and `poseidon`. Those are not claims of completed proofs. Never generate 100,000 proofs. The runner does not save a receipt or witness artifact. Receipt verification alone does not validate witness confidentiality or safe debug artifacts.

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
