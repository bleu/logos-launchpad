# Launchpad packages

Two QML mini-apps share one non-UI Rust core. Their roles follow [ADR 0019](../../docs/adr/0019-product-packages.md):

| Package | Role | Flake output |
| --- | --- | --- |
| `launchpad_admin_app` | Namespace administration | `admin-lgx` |
| `launchpad_configured_sales_app` | Creator and participant flows within a configured namespace | `configured-sales-lgx` |
| `launchpad_core` | Shared backend | `core-lgx` |

These are initial product entry points. Both apps currently have only a `Test backend` button. It calls the real `launchpad_core_logic::get_status()` through the generated native module and displays `Backend OK`. Namespace administration, sales workflows, wallets, indexed reads and LEZ calls are not implemented here.

## Prerequisites

The supported build target is macOS arm64 (`aarch64-darwin`). Install the tools yourself before you build or run the apps:

- [Nix](https://nixos.org/download/) with `nix-command` and `flakes`, plus the Xcode command line tools and network access for builds.
- [Basecamp 0.3.1](https://github.com/logos-co/logos-basecamp/releases/tag/0.3.1), using its macOS arm64 disk image. It requires macOS 12.0 or later.
- [Portable lgpm 0.3.0](https://github.com/logos-co/logos-package-manager/releases/tag/0.3.0), using `lgpm-aarch64-macos.tar.gz`. Use the portable release, not a Nix development variant.
- Rust/Cargo 1.94.0 for the host checks below.

Make `nix`, `cargo` and `lgpm` available in your shell. Set `BASECAMP` to the executable in your installed Basecamp app. No LEZ node, RISC0 guest or wallet is needed for the current backend check.

| Tool | Pinned source revision |
| --- | --- |
| Basecamp 0.3.1 | `aeb819216c1a54563d758e82fb2264359987dd2c` |
| logos-module-builder 0.3.2 | `4b7998272c5ec014bcac4bf1c7dbe7602c63a3c1` |
| lgpm 0.3.0 release tag | `3133786ea86821e251fccc82a2eefbfc4db7605e` |

The previously checked portable `lgpm` reports build commit `8994c273ec6c6373ca8e164e1f7bde416640d662`, which differs from its release tag. The download SHA-256 values are `15a56ccebe597bbccaaba3cf93e7577cf3592dbb3f80b9e83333016d10794a2e` for the lgpm archive and `115102ed5bd17faa62d0a7f21faa73cd510c7610a9d344b2f81e1090f686c565` for the Basecamp disk image.

## Build and checks

Run from the repository root. Keep `flake.lock` and both Cargo lockfiles. These commands use saved locks; they do not generate or update them.

```sh
LAUNCHPAD="$PWD/packages/launchpad"

cargo test --manifest-path "$LAUNCHPAD/core/logic/Cargo.toml" --locked
cargo fmt --manifest-path "$LAUNCHPAD/core/logic/Cargo.toml" -- --check
cargo clippy --manifest-path "$LAUNCHPAD/core/logic/Cargo.toml" --all-targets --locked -- -D warnings
rustfmt --edition 2021 --check "$LAUNCHPAD/core/rust-lib/src/lib.rs"

nix build --no-update-lock-file -L "path:$LAUNCHPAD#core-lgx" --out-link "$LAUNCHPAD/result-core"
nix build --no-update-lock-file -L "path:$LAUNCHPAD#admin-lgx" --out-link "$LAUNCHPAD/result-admin"
nix build --no-update-lock-file -L "path:$LAUNCHPAD#configured-sales-lgx" --out-link "$LAUNCHPAD/result-sales"
nix build --no-update-lock-file "path:$LAUNCHPAD#core-lidl" --out-link "$LAUNCHPAD/result-lidl"

mkdir -p "$LAUNCHPAD/dist"
install -m 644 "$LAUNCHPAD"/result-core/*.lgx "$LAUNCHPAD/dist/core.lgx"
install -m 644 "$LAUNCHPAD"/result-admin/*.lgx "$LAUNCHPAD/dist/admin.lgx"
install -m 644 "$LAUNCHPAD"/result-sales/*.lgx "$LAUNCHPAD/dist/configured-sales.lgx"
shasum -a 256 "$LAUNCHPAD"/dist/*.lgx
```

The builder derives the module `.lidl` from `LaunchpadCoreModule`, then generates the Rust provider and Qt adapter. Both apps depend on that contract. The flake stages the pure logic crate inside the native crate because the pinned builder copies only `rust-lib`. Do not hand-write native glue or commit the staged SDK and generated source. Ordinary Cargo checks of the native crate require those generated inputs; host tests above cover the pure function instead.

`build-locale.nix` sets `LC_ALL=C` in affected pinned build tools to avoid a macOS Bash locale crash. It does not change host settings, source pins or the Nix sandbox.

## Install and manual check

Install into a fresh test folder outside the flake source. Basecamp must use the same folder. Run in the same shell as the build commands:

```sh
mkdir -p .local/launchpad
SESSION="$(mktemp -d "$PWD/.local/launchpad/session-XXXXXX")"
lgpm --version
lgpm --modules-dir "$SESSION/modules" --ui-plugins-dir "$SESSION/plugins" install --file "$LAUNCHPAD/dist/core.lgx"
lgpm --modules-dir "$SESSION/modules" --ui-plugins-dir "$SESSION/plugins" install --file "$LAUNCHPAD/dist/admin.lgx"
lgpm --modules-dir "$SESSION/modules" --ui-plugins-dir "$SESSION/plugins" install --file "$LAUNCHPAD/dist/configured-sales.lgx"
lgpm --modules-dir "$SESSION/modules" --ui-plugins-dir "$SESSION/plugins" list
lgpm --modules-dir "$SESSION/modules" --ui-plugins-dir "$SESSION/plugins" deps launchpad_admin_app
lgpm --modules-dir "$SESSION/modules" --ui-plugins-dir "$SESSION/plugins" deps launchpad_configured_sales_app
"$BASECAMP" --user-dir "$SESSION"
```

Open `Launchpad admin`, click `Test backend` and check for `Backend OK`. Do the same in `Launchpad sales`. In each app, check that the button is disabled while waiting and works again after the reply. Close Basecamp, start it again with the same `--user-dir` and repeat both calls. Keep package-manager output and the session's `logs/` files if loading or calling fails. Runtime state, logs and build outputs must remain ignored by git.

## Evidence limits

On 2026-10-05, the renamed pure Rust test, formatting and clippy passed. Root host tests (196), formatting, all-feature clippy and the LEZ pin check also passed. Nix 2.35.2 built all three portable archives and the generated `get_status() -> tstr` contract with the unchanged flake lock. Portable lgpm 0.3.0 installed all three at version 0.1.0 into fresh isolated state; both app dependency checks returned `launchpad_core`. Installed QML matched maintained source, and all three installed contracts matched `core-lidl`. The four native libraries are arm64 Mach-O with valid ad-hoc signatures; inspected load commands use system, `@rpath` or `@loader_path` paths, with no Nix paths.

| Archive | SHA-256 |
| --- | --- |
| `dist/core.lgx` | `b89f561ff59ba8cb1dea00f05caf5eed9bfef197bb4759d74806d862d302c35e` |
| `dist/admin.lgx` | `ca2d9e3bf3eca32df6f66c39886d822130626db90775e33f248e38eafcca22a7` |
| `dist/configured-sales.lgx` | `52ae1b60117c2e768ee4eeae108e636d536d79284c88483a864d31930bde6c7c` |

The check host used macOS 14.4 (23E214) and Rust/Cargo 1.94.0. The earlier single status app passed a user-reported screen, call and restart check on 2026-10-05. That result does not verify these newly named app packages. Their screen, call, pending-button and restart checks remain pending.

Build and GUI evidence are separate under [ADR 0022](../../docs/adr/0022-verification-boundaries.md). A passing host test does not prove native integration. Local archives are unsigned. The bundler can report embedded Nix strings in diagnostic paths and OpenSSL resource defaults; load-path inspection does not test those resource paths or prove operation on a host without Nix. No cross-platform or production SDK behavior is claimed.

## Source and notices

The native scaffold, CMake file, metadata and QML layout are adapted from the pinned builder's [Rust module](https://github.com/logos-co/logos-module-builder/tree/4b7998272c5ec014bcac4bf1c7dbe7602c63a3c1/templates/rust-module) and [QML-only](https://github.com/logos-co/logos-module-builder/tree/4b7998272c5ec014bcac4bf1c7dbe7602c63a3c1/templates/ui-qml) templates. Both icons are copied unchanged. The required MIT notice is retained in `licenses/logos-module-builder-MIT`.

The [Basecamp bridge source](https://github.com/logos-co/logos-view-module-runtime/blob/91dc59b32d4be9230bd8473719a26661867c682e/src/LogosQmlBridge.cpp) matches the runtime revision reported by the previously tested Basecamp release. Its async callback receives JSON text: a successful string is a bare JSON string; errors are objects with `error` and an optional `message`. Both apps preserve the pending state, five-second timeout and call/reply error handling.
