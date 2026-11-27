# Launchpad packages

The admin app is for namespace administration. The configured-sales app is for creators and participants within a configured namespace. Both use one shared Rust core. See [package roles](../../docs/adr/0019-product-packages.md).

Currently, both apps have only a `Test backend` button that displays `Backend OK`. Administration and sales workflows are not implemented yet.

## Requirements

Use macOS arm64. Install these tools yourself:

- [Nix](https://nixos.org/download/) with `nix-command` and `flakes` enabled, plus Xcode command line tools, to build the packages.
- [Basecamp 0.3.1](https://github.com/logos-co/logos-basecamp/releases/tag/0.3.1), macOS arm64.
- [Portable lgpm 0.3.0](https://github.com/logos-co/logos-package-manager/releases/tag/0.3.0), `lgpm-aarch64-macos.tar.gz`.

Make `nix` and `lgpm` available in your shell. Set `BASECAMP` to your installed Basecamp executable. Run the commands below from the repository root, in the same shell.

## Build

One command builds all three `.lgx` archives:

```sh
nix --extra-experimental-features 'nix-command flakes' build --no-update-lock-file path:./packages/launchpad --out-link packages/launchpad/result-packages
```

Building the archives does not check module loading, wallet compatibility or sale execution. See the [workspace guide](../../docs/architecture/workspace-guide.md#execution-and-evidence-limits) for what needs separate verification.

## Install

Use a separate local data folder to keep your normal Basecamp data unchanged:

```sh
SESSION="$PWD/.local/launchpad/session"
lgpm --modules-dir "$SESSION/modules" --ui-plugins-dir "$SESSION/plugins" install --dir packages/launchpad/result-packages
```

## Run

```sh
"$BASECAMP" --user-dir "$SESSION"
```

Open `Launchpad admin` or `Launchpad sales` and click `Test backend`. Expect `Backend OK`. Use the same launch command after closing Basecamp to keep the installed apps and data.
