# Basecamp status packaging check

Two separate packages check the Rust-first packaging path: `launchpad_status_app` is a QML mini-app; `launchpad_status_core` is a non-UI Rust backend. The button calls the real `launchpad_status::get_status()` function and expects `Backend OK`. This probe does not implement the production shared core, admin app or configured-sales app in [ADR 0019](../../docs/adr/0019-product-packages.md).

## Environment

The selected target is macOS arm64 (`aarch64-darwin`). The host check used macOS 14.4 (23E214) and Rust/Cargo 1.94.0. Package builds need Nix with `nix-command` and `flakes`, network access and the Xcode command line tools. Install Nix with the user's participation. No LEZ node, RISC0 guest, wallet or launchpad SDK is needed.

| Tool | Selected release | Source revision |
| --- | --- | --- |
| Basecamp | 0.3.1 | `aeb819216c1a54563d758e82fb2264359987dd2c` |
| logos-module-builder | 0.3.2 | `4b7998272c5ec014bcac4bf1c7dbe7602c63a3c1` |
| Portable `lgpm` | 0.3.0 | Release tag: `3133786ea86821e251fccc82a2eefbfc4db7605e` |

The released macOS `lgpm` reports build commit `8994c273ec6c6373ca8e164e1f7bde416640d662`, which differs from its release tag. Its exact archive hash is below. Do not describe this binary as a build of the tag revision. Keep the builder's internal inputs locked together; do not update them separately.

## Build and checks

Run from the repository root. Use `path:` references so Nix sees new files without git staging. The check used Nix 2.35.2. If `nix` is absent from the current shell's `PATH`, use `/nix/var/nix/profiles/default/bin/nix`.

```sh
PROBE="$PWD/packages/basecamp-status"

cargo test --manifest-path "$PROBE/backend/status/Cargo.toml" --locked
cargo fmt --manifest-path "$PROBE/backend/status/Cargo.toml" -- --check
cargo clippy --manifest-path "$PROBE/backend/status/Cargo.toml" --all-targets --locked -- -D warnings
rustfmt --edition 2021 --check "$PROBE/backend/rust-lib/src/lib.rs"

nix --extra-experimental-features 'nix-command flakes' flake lock "path:$PROBE"
nix --extra-experimental-features 'nix-command flakes' build --no-update-lock-file "path:$PROBE#rust-sdk-src" --out-link "$PROBE/backend/logos-rust-sdk-src"
cargo generate-lockfile --manifest-path "$PROBE/backend/rust-lib/Cargo.toml"

nix --extra-experimental-features 'nix-command flakes' build --no-update-lock-file -L "path:$PROBE#backend-lgx" --out-link "$PROBE/result-backend"
nix --extra-experimental-features 'nix-command flakes' build --no-update-lock-file -L "path:$PROBE#app-lgx" --out-link "$PROBE/result-app"
nix --extra-experimental-features 'nix-command flakes' build --no-update-lock-file -L "path:$PROBE#backend-lidl" --out-link "$PROBE/result-lidl"

mkdir -p "$PROBE/dist"
install -m 644 "$PROBE"/result-backend/*.lgx "$PROBE/dist/backend.lgx"
install -m 644 "$PROBE"/result-app/*.lgx "$PROBE/dist/app.lgx"
shasum -a 256 "$PROBE"/dist/*.lgx
tar -tzf "$PROBE/dist/backend.lgx"
tar -xOf "$PROBE/dist/backend.lgx" manifest.json
tar -tzf "$PROBE/dist/app.lgx"
tar -xOf "$PROBE/dist/app.lgx" manifest.json
```

Keep `flake.lock` and both Cargo lockfiles. Lock generation is a setup step; repeat builds must use the saved locks. The native template needs its staged SDK and generated provider before ordinary host Cargo checks can run. The dependency-free status crate tests the same function used by the native module. A passing host test does not prove native integration.

## Install and manual check

Use the released portable tools with the portable packages. Do not use Nix development variants. Download [Basecamp 0.3.1 for macOS arm64](https://github.com/logos-co/logos-basecamp/releases/download/0.3.1/LogosBasecamp-Desktop-v0.3.1-aeb819-aarch64.dmg). It declares macOS 12.0 or later. The disk image SHA-256 is `115102ed5bd17faa62d0a7f21faa73cd510c7610a9d344b2f81e1090f686c565`. The local check uses a copy at `tools/LogosBasecamp.app`; no system-wide install is needed. For a fresh checkout, open the disk image and copy `LogosBasecamp.app` into `packages/basecamp-status/tools/`.

Install into a new test folder. Basecamp must use the same folder:

```sh
PROBE="$PWD/packages/basecamp-status"
mkdir -p "$PROBE/tools"
curl -fL https://github.com/logos-co/logos-package-manager/releases/download/0.3.0/lgpm-aarch64-macos.tar.gz -o "$PROBE/tools/lgpm-aarch64-macos.tar.gz"
printf '15a56ccebe597bbccaaba3cf93e7577cf3592dbb3f80b9e83333016d10794a2e  %s\n' "$PROBE/tools/lgpm-aarch64-macos.tar.gz" | shasum -a 256 -c -
tar -xzf "$PROBE/tools/lgpm-aarch64-macos.tar.gz" -C "$PROBE/tools"
LGPM="$PROBE/tools/lgpm-aarch64-macos/bin/lgpm"
"$LGPM" --version

SESSION="$(mktemp -d "$PROBE/session-XXXXXX")"
"$LGPM" --modules-dir "$SESSION/modules" --ui-plugins-dir "$SESSION/plugins" install --file "$PROBE/dist/backend.lgx"
"$LGPM" --modules-dir "$SESSION/modules" --ui-plugins-dir "$SESSION/plugins" install --file "$PROBE/dist/app.lgx"
"$LGPM" --modules-dir "$SESSION/modules" --ui-plugins-dir "$SESSION/plugins" list
"$LGPM" --modules-dir "$SESSION/modules" --ui-plugins-dir "$SESSION/plugins" deps launchpad_status_app

BASECAMP="$PROBE/tools/LogosBasecamp.app/Contents/MacOS/LogosBasecamp"
"$BASECAMP" --user-dir "$SESSION" > "$SESSION/basecamp.log" 2>&1
```

Use the same `SESSION` for the restart. Keep the package-manager output, `$SESSION/basecamp.log` and Basecamp's `$SESSION/logs/` files if loading or calling fails. Unsigned local packages are expected; do not claim release signing.

- [x] Install the backend, then the QML app, with no variant or dependency errors.
- [x] Open `Launchpad status check` in Basecamp and confirm the screen is visible.
- [x] Click `Test backend` and confirm `Backend OK`.
- [ ] Confirm the button is disabled while waiting and can be used again after the reply.
- [x] Close Basecamp, reopen it with the same `--user-dir`, and repeat the call.
- [x] Record the host, user result and load/call logs. See the evidence below.

## Evidence and contract limits

| Check | Result |
| --- | --- |
| Real Rust public function | Red: empty string failed against `Backend OK`. Green: implementation passed. |
| Status crate formatting and clippy | Passed. |
| Native source formatting | Passed. |
| Root host tests, formatting, all-feature clippy and LEZ pin check | Passed after the workspace exclusions. |
| Released portable `lgpm` | Download hash matched the release; `--version` reports 0.3.0, portable. |
| Basecamp disk image | Download hash matched the release; local app metadata reports 0.3.1. Copied to ignored `tools/LogosBasecamp.app`; launched twice by the user. |
| Nix version and lockfiles | Nix 2.35.2; flake and both Cargo lockfiles generated. |
| Generated module `.lidl` | `get_status() -> tstr`, no arguments or events; matches the contract in the QML archive. |
| Portable QML `.lgx` | Built; `darwin-arm64` payload and backend dependency inspected. |
| Native generated adapter and portable backend `.lgx` | Built the real Rust static library and generated Qt adapter; inspected the `darwin-arm64` package manifest and payload. |
| Isolated install and dependency check | Passed with released portable `lgpm`; `list` shows both packages at 0.1.0 and `deps launchpad_status_app` returns `launchpad_status_core`. Archives are unsigned. |
| Installed native payload | arm64 Mach-O; dynamic library references use system, `@rpath` or `@loader_path` paths; ad-hoc code signature verified. Both installed contracts match the generated `.lidl`. |
| Screen, backend call and restart | User reported success on 2026-10-05 after two launches with the same test folder. Both logs show the backend and QML app loading and `get_status` called with no arguments. No probe call/reply errors were logged. |
| Pending-button state | Implemented in QML; not separately reported by the user. |

| Archive | SHA-256 |
| --- | --- |
| `dist/backend.lgx` | `d9043d1deb6df481033db496579b76fb77afbed8decf35193542a8aba37bc314` |
| `dist/app.lgx` | `d02904813d75bd35fdb95c2b1944dd0a55dee4281fd0f875df71c4d701d79715` |

Both copies match their Nix outputs and are ignored by git.

Pinned Bash 5.3p3 crashes during locale changes in macOS Nix build hooks. A reduced `(LANG=C true)` build reproduced the crash; setting derivation `LC_ALL=C` passed. `build-locale.nix` applies that setting to the affected Logos tools and probe builds. The final backend build passed without shell crashes, including the previously failing Qt generators and headers hook. Source revisions, the lockfile, the sandbox and system settings are unchanged.

Upstream Rust and CMake warnings remain. The bundler also reports embedded Nix strings in diagnostic source paths and OpenSSL resource defaults; dynamic library references were checked separately. This probe does not test those OpenSSL resource paths or prove runtime operation on a host without Nix.

The builder derives the module `.lidl` from `LaunchpadStatusCoreModule`, generates the native provider and Qt adapter, and receives `launchpad_status_core` as the QML dependency input. Do not hand-write a C ABI or commit generated code. The pinned builder copies only `rust-lib`; the flake copies the real sibling status library into that staged directory and uses a relative Cargo path. It removes the copied library's workspace declaration only in staged source. The QML package must include a 256x256 PNG icon. Package inspection must confirm `darwin-arm64`, backend metadata, QML files and the backend dependency contract.

The released Basecamp runtime source defines `logos.callModuleAsync("launchpad_status_core", "get_status", [], callback, 5000)`. The callback receives JSON text: a successful string is a bare JSON string, not a `result` wrapper. Runtime errors are objects with `error` and an optional `message`. The screen shows pending, call errors and reply errors, and prevents overlapping requests. The manual check passed on the recorded macOS host. The launch logs report bundled runtime revision `91dc59b32d4be9230bd8473719a26661867c682e`; the linked bridge source below matches that revision.

The local logs are `logs/basecamp_20261005_141647.log` and `logs/basecamp_20261005_141730.log` under the test folder. They show a call before and after restart, but do not record the returned string. Visible success is user-reported. Basecamp platform warnings are present; no probe call/reply errors were logged.

This probe does not exercise indexed reads, LEZ calls, signing, wallets, private routes, trades or production SDK calls. Build evidence and GUI evidence remain separate under [ADR 0022](../../docs/adr/0022-verification-boundaries.md). The minimal packaging check passed on the recorded host; this is not production shared-core or cross-platform evidence.

## Source and notices

The native scaffold, CMake build file, package metadata and QML layout are adapted from the [Rust module](https://github.com/logos-co/logos-module-builder/tree/4b7998272c5ec014bcac4bf1c7dbe7602c63a3c1/templates/rust-module) and [QML-only](https://github.com/logos-co/logos-module-builder/tree/4b7998272c5ec014bcac4bf1c7dbe7602c63a3c1/templates/ui-qml) templates. The QML icon is copied unchanged from that snapshot. Required upstream notices are retained in `licenses/`. The probe replaces the example greetings with the tested status function and adds locked paired packaging. Runtime payload details are from [Basecamp's pinned bridge source](https://github.com/logos-co/logos-view-module-runtime/blob/91dc59b32d4be9230bd8473719a26661867c682e/src/LogosQmlBridge.cpp).
