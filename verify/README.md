# Repeatable builds

Run these commands from the repository root. They use the imported PoC and weighted-math workspaces. They do not deploy programs or need wallet state. See [build evidence](build-evidence.md) for the tested environment and results.

## Tools and setup

The host toolchain is Rust 1.94.0. Install its format and lint tools:

```sh
rustup toolchain install 1.94.0 --component rustfmt --component clippy
```

A native C/C++ compiler and linker are needed. On macOS, install Apple Command Line Tools with `xcode-select --install`. Network access is needed to get Cargo dependencies, Git sources and RISC0 tool archives. Keep the downloaded caches for later runs. Allow at least 10 GiB for fresh build outputs and run workspaces in sequence. More space is needed if you retain both debug and release outputs.

Install the build tools in a separate directory so they do not replace global executables:

```sh
source verify/build-tools.env
tools="$HOME/.local/share/logos-launchpad-build-tools"
cargo +1.94.0 install rzup --version "$RZUP_VERSION" --locked --root "$tools/rzup"
cargo +1.94.0 install --locked --git https://github.com/logos-co/scaffold --rev "$SCAFFOLD_REV" --root "$tools/scaffold" logos-scaffold
export PATH="$tools/scaffold/bin:$tools/rzup/bin:$HOME/.cargo/bin:$HOME/.risc0/bin:$PATH"
rzup install rust "$RISC0_RUST_VERSION"
rzup install cpp "$RISC0_CPP_VERSION"
rzup install r0vm "$RISC0_TOOLS_VERSION"
rzup install cargo-risczero "$RISC0_TOOLS_VERSION"
bash verify/check-builds.sh --preflight
```

`rzup` stores the compiler and tools under its RISC0 home, normally `~/.risc0`. Use `rzup show` to check which versions are selected. The preflight command requires rzup 0.5.0, guest Rust 1.94.1, C++ 2024.1.5, cargo-risczero 3.0.5 and r0vm 3.0.5. If those versions are already installed and selected, use them without running the install commands again.

The scaffold source commit is the fixed reference for release tag `v0.2.0`. That source declares package version 0.1.1, and `lgs --version` prints 0.1.1. The preflight checks its Cargo installation metadata against the source commit. Install it with `cargo install --rev` under a separate `--root`; a copied executable alone does not supply that metadata.

LEZ and SPEL source references remain in `scaffold.toml`. LEZ manifest references must match it. Cargo lockfiles record the exact Rust dependency versions: curve workspaces resolve `risc0-build` and `risc0-zkvm` to 3.0.6, while weighted-math guest builds resolve them to 3.0.5. Those crate versions are separate from the tool versions. Do not change the locks just to make their version labels match.

## Full check

```sh
CARGO_BUILD_JOBS=4 bash verify/check-builds.sh
```

The command checks tools and builds curve guests and the release host/client/CLI. Test stages cover host behavior, weighted-math parity and curve client guest execution. Client guest tests use development receipts. A final probe checks actual release overflow behavior. The command uses `--locked` for direct Cargo commands and `RISC0_BUILD_LOCKED=1` for nested guest builds. It checks that all committed lockfiles stay unchanged, including when a stage fails. Any failed stage stops the command.

Keep each workspace's output directory. Do not set a shared `CARGO_TARGET_DIR`: the client guest tests load the curve artifacts from `methods/target/`. The checker rejects guest-skip, guest-debug, compiler, target-directory and overflow-profile overrides that can bypass its baseline. Clear any reported setting and run again.

Release overflow checks must be set in each consuming workspace. A dependency's release profile does not apply to its caller. The root, methods host, curve guest, math host, math zkVM host and math guest workspaces each set `overflow-checks = true`. The probe compiles an example with the curve guest's own release profile. It first executes a valid sum, then requires an overflow panic for `u64::MAX + 1`. Normal guest embedding excludes examples; the probe is not a deployed program.

The curve and math guest manifests also pass `-Zremap-cwd-prefix=/source` through their RISC0 metadata. The pinned guest compiler supports this option. It replaces absolute paths below each crate's compiler working directory with `/source`; relative source paths stay unchanged. Both guest release profiles strip symbols and disable debug data.

This compiler path remap does not change Cargo's internal IDs for local dependencies outside the guest workspace. Moving the checkout changes those IDs and some internal function names. The factory's code order and image ID can then change. Use image IDs from the matching build.

## Release source path

Release builds use `/tmp/logos-launchpad-release/source`. This fixes the source path seen by Cargo without changing workspace boundaries or tool pins. It does not use a shared `CARGO_TARGET_DIR`. On macOS, the physical path is `/private/tmp/logos-launchpad-release/source`; the checker resolves this system alias.

From a clean checkout of a committed release, prepare a separate build checkout:

```sh
release_parent=/tmp/logos-launchpad-release
mkdir -m 700 "$release_parent" &&
git clone --no-hardlinks "$PWD" "$release_parent/source" &&
cd "$release_parent/source" &&
bash verify/check-builds.sh --release
```

The clone copies committed source only. Commit the release changes before using this recipe; uncommitted changes are not included. If the directory already exists, stop and inspect it. Do not replace or delete it without checking its contents. The checker does not create or overwrite a source checkout.

`--release` rejects other source paths and requires the release source and its parent to be owned by the current user. An atomic directory lock at `/tmp/logos-launchpad-release.lock` permits one release check at a time. An occupied lock is left unchanged. Do not edit or switch the release source while a check runs. The checker removes only its own lock when it exits, including after a failed stage.

All outputs stay in the release checkout's separate workspace `target/` directories. Keep the five guest `.bin` files and their generated image IDs for the release. Save them before removing the build checkout or clearing temporary storage. Clients must use those exact guest files; a local rebuild at another source path is not a replacement with the same identity.

This is a fixed-path workaround. Reproduction from arbitrary checkout paths remains unresolved. Cross-operating-system artifact identity is not established. See [build evidence](build-evidence.md) for the tested environment and current result.


## Build-check regressions

```sh
bash verify/tests/check-builds.sh
bash verify/tests/check-builds.sh compiler-override
bash verify/tests/check-builds.sh release-path
bash verify/tests/check-builds.sh release-busy
bash verify/tests/check-builds.sh release-failed-stage
bash verify/tests/check-builds.sh failed-host-test
bash verify/tests/check-builds.sh failed-math-test
bash verify/tests/check-builds.sh failed-guest-test
bash verify/tests/check-builds.sh changed-lockfile
bash verify/tests/check-builds.sh nested-lock-drift
bash verify/tests/check-builds.sh math-lock-drift
bash verify/tests/check-builds.sh complete-build
bash verify/tests/check-builds.sh repeated-build
```

The default command runs seven small preflight cases with executable fixtures. Failed-stage and changed-lockfile cases use a Cargo fixture and the selected real tools for preflight. Release cases check the fixed-path guard, refusal of an occupied lock and removal of an owned lock after a failed stage. These cases test command failure handling. They do not prove that a real build succeeds.

The two nested-lock cases use real guest tools. They reject a stale guest lock in a temporary source copy without rewriting it. `complete-build` copies the current tracked source and the new build-check sources into a temporary directory. It runs the full real check with no prior outputs or scaffold state, then checks the release CLI and guest results. The temporary directory is removed after the run. Downloaded Cargo and RISC0 caches are shared.

`repeated-build` makes two independent fresh source copies and stages each one at `/tmp/logos-launchpad-release/source` in sequence. Each runs the full public `--release` check and CLI help check with no prior outputs. The packaged guest SHA-256 hashes and generated image IDs must match. The test refuses an existing release directory and removes only the tree it created. Both sets of build outputs do not need to fit on disk at once.

`bash verify/tests/check-builds.sh relocated-build` retains the earlier comparison at two different build paths. This diagnostic is expected to fail for the factory with the current Cargo workspace layout. It is not the release acceptance check.

## Other checks

```sh
cargo fmt --all -- --check
cargo fmt --manifest-path methods/Cargo.toml --all -- --check
cargo fmt --manifest-path methods/guest/Cargo.toml --all -- --check
cargo fmt --manifest-path packages/weighted-math/Cargo.toml --all -- --check
cargo fmt --manifest-path packages/weighted-math/zkvm/Cargo.toml --all -- --check
cargo fmt --manifest-path packages/weighted-math/zkvm/methods/guest/Cargo.toml --all -- --check
RISC0_BUILD_LOCKED=1 cargo clippy --workspace --all-targets --all-features --release --locked -- -D warnings
RISC0_SKIP_BUILD=1 cargo clippy --manifest-path methods/Cargo.toml --all-targets --all-features --release --locked -- -D warnings
cargo clippy --manifest-path packages/weighted-math/Cargo.toml --workspace --all-targets --all-features --release --locked -- -D warnings
RISC0_SKIP_BUILD=1 cargo clippy --manifest-path packages/weighted-math/zkvm/Cargo.toml --workspace --all-targets --all-features --release --locked -- -D warnings
CARGO_BUILD_JOBS=4 bash verify/check-builds.sh
bash verify/check-idl.sh
```

The methods and math zkVM Clippy commands lint host code only. The pinned guest builder inherits Clippy's host compiler wrapper and fails to find the guest standard library if it builds during lint. Run these lint commands after a real guest build. Then run the full checker again without `RISC0_SKIP_BUILD`; the checker rejects that setting. Guest lint is not established by these host-only checks.

IDL checks, fuzzing, CI and live platform tests keep their existing boundaries. See the [workspace guide](../docs/architecture/workspace-guide.md).

## Proof modes

Weighted-math parity and the overflow probe execute guests without generating proofs. The client guest tests use `RISC0_DEV_MODE=1` and development receipts. A passing full check does not establish real-proof generation or production proving acceptance. The full private-buy route still exceeds the pinned LEZ chained-call limit; its expected rejection is not a successful private purchase.
