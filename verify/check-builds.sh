#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'check-builds: %s\n' "$1" >&2
  exit 1
}

case "${1:-}" in
  ''|--preflight|--release) [ "$#" -le 1 ] || fail 'usage: bash verify/check-builds.sh [--preflight|--release]' ;;
  *) fail 'usage: bash verify/check-builds.sh [--preflight|--release]' ;;
esac

command -v cargo >/dev/null 2>&1 || fail 'missing required tool: cargo'
for variable in RISC0_SKIP_BUILD RISC0_BUILD_DEBUG RISC0_RUST_SRC RUSTC RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_TARGET_DIR CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS CC_riscv32im_risc0_zkvm_elf CFLAGS_riscv32im_risc0_zkvm_elf; do
  [ -z "$(printenv "$variable" || true)" ] || fail "unset $variable for reproducible builds"
done
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$root"
if [ "${1:-}" = --release ]; then
  release_root="$(cd /tmp && pwd -P)/logos-launchpad-release/source"
  [ "$root" = "$release_root" ] || fail 'release builds require source at /tmp/logos-launchpad-release/source'
  [ -O "$root" ] && [ -O "$root/.." ] || fail 'release source and its parent must be owned by the current user'
fi
bash verify/check-pins.sh --tools
printf 'check-builds: prerequisites passed\n'
[ "${1:-}" != --preflight ] || exit 0

release_lock=''
cleanup_release_lock() {
  [ -z "$release_lock" ] || rmdir "$release_lock"
}
if [ "${1:-}" = --release ]; then
  lock="$(cd /tmp && pwd -P)/logos-launchpad-release.lock"
  mkdir -m 700 "$lock" || fail 'release build lock is occupied; leave it unchanged'
  release_lock="$lock"
  trap cleanup_release_lock EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
fi

export RISC0_BUILD_LOCKED=1
lockfiles=(Cargo.lock methods/Cargo.lock methods/guest/Cargo.lock packages/weighted-math/Cargo.lock packages/weighted-math/fuzz/Cargo.lock packages/weighted-math/zkvm/Cargo.lock packages/weighted-math/zkvm/methods/guest/Cargo.lock)
lock_before="$(shasum -a 256 "${lockfiles[@]}")"
check_locks() {
  local status=$? changed=0
  trap - EXIT
  [ "$lock_before" = "$(shasum -a 256 "${lockfiles[@]}")" ] || changed=1
  cleanup_release_lock || status=1
  [ "$changed" = 0 ] || fail 'lockfiles changed during build'
  if [ "$status" = 0 ]; then
    printf '\ncheck-builds: all checks passed; lockfiles unchanged; real proofs not tested\n'
  fi
  exit "$status"
}
trap check_locks EXIT

run() {
  local stage="$1"
  shift
  printf '\ncheck-builds: %s\n' "$stage"
  "$@" || fail "stage failed: $stage"
}

run 'curve guests (release, locked)' cargo build --manifest-path methods/Cargo.toml --release --locked
run 'weighted-math guest parity (execution only, release, locked)' cargo run --manifest-path packages/weighted-math/zkvm/Cargo.toml --release --locked
run 'host, client and CLI (release, locked)' cargo build --workspace --release --locked
run 'host, client and CLI tests (release, locked)' cargo test --workspace --release --locked
run 'weighted-math host (release, locked)' cargo build --manifest-path packages/weighted-math/Cargo.toml --workspace --release --locked
run 'weighted-math host tests (release, locked)' cargo test --manifest-path packages/weighted-math/Cargo.toml --workspace --release --locked
run 'curve guest tests (development receipts, release, locked)' env RISC0_DEV_MODE=1 cargo test -p launchpad-client --release --features guest-tests --test private_authority --test settlement --test namespace_admin --locked
run 'release overflow tests (host and guest execution, locked)' cargo test --manifest-path methods/Cargo.toml --release --features overflow-tests --test release_overflow --locked
