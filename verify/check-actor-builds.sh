#!/usr/bin/env bash
set -euo pipefail
fail() { printf 'check-builds: %s\n' "$1" >&2; exit 1; }
case "${1:-}" in
  ''|--preflight|--execution|--release) [ "$#" -le 1 ] || fail 'usage: check-builds.sh [--preflight|--execution|--release]' ;;
  *) fail 'usage: check-builds.sh [--preflight|--execution|--release]' ;;
esac
command -v cargo >/dev/null 2>&1 || fail 'missing required tool: cargo'
for variable in RISC0_SKIP_BUILD RISC0_BUILD_DEBUG RISC0_RUST_SRC RUSTC RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_TARGET_DIR CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS CC_riscv32im_risc0_zkvm_elf CFLAGS_riscv32im_risc0_zkvm_elf; do
  [ -z "$(printenv "$variable" || true)" ] || fail "unset $variable for reproducible builds"
done
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$root"
source verify/actor-tools.env
if [ "${1:-}" = --release ]; then
  release_root="$(cd /tmp && pwd -P)/logos-launchpad-release/source"
  [ "$root" = "$release_root" ] || fail 'release builds require source at /tmp/logos-launchpad-release/source'
  [ -O "$root" ] && [ -O "$root/.." ] || fail 'release source and its parent must be owned by the current user'
fi
bash verify/check-pins.sh --tools
printf 'check-builds: actor prerequisites passed; scaffold and SPEL CLI are not required\n'
[ "${1:-}" != --preflight ] || exit 0
release_lock=''
cleanup_release_lock() { [ -z "$release_lock" ] || rmdir "$release_lock"; }
if [ "${1:-}" = --release ]; then
  lock="$(cd /tmp && pwd -P)/logos-launchpad-release.lock"
  mkdir -m 700 "$lock" || fail 'release build lock is occupied; leave it unchanged'
  release_lock="$lock"
  trap cleanup_release_lock EXIT
fi
base=packages/lez-compat
lockfiles=(Cargo.lock methods/Cargo.lock methods/guest/Cargo.lock "$base/Cargo.lock" "$base/methods/Cargo.lock" "$base/methods/guest/Cargo.lock" "$base/execution/Cargo.lock" packages/weighted-math/Cargo.lock packages/weighted-math/fuzz/Cargo.lock packages/weighted-math/zkvm/Cargo.lock packages/weighted-math/zkvm/methods/guest/Cargo.lock)
before="$(shasum -a 256 "${lockfiles[@]}")"
check_locks() {
  local status=$? changed=0
  trap - EXIT
  [ "$before" = "$(shasum -a 256 "${lockfiles[@]}")" ] || changed=1
  cleanup_release_lock || status=1
  [ "$changed" = 0 ] || fail 'lockfiles changed during build'
  if [ "$status" = 0 ]; then
    printf 'check-builds: repository checks passed; lockfiles unchanged; real proofs, service integration and deployment readiness not tested\n'
  fi
  exit "$status"
}
trap check_locks EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
export RISC0_BUILD_LOCKED=1
run() {
  local stage="$1"
  shift
  printf '\ncheck-builds: %s\n' "$stage"
  "$@" || fail "stage failed: $stage"
}
check_guest_artifacts() {
  local methods="$1" source guest artifact
  local sources=("$methods"/guest/src/bin/*.rs)
  [ -f "${sources[0]}" ] || fail "missing guest sources: $methods/guest/src/bin"
  for source in "${sources[@]}"; do
    guest="${source##*/}"
    guest="${guest%.rs}"
    artifact="$(find "$methods/target/riscv-guest" -type f -path "*/release/$guest.bin" -size +0c -print -quit 2>/dev/null)" || true
    [ -n "$artifact" ] || fail "missing guest artifact: $guest.bin"
  done
}
run 'host, client and CLI (release, locked)' cargo +"$ACTOR_RUST_VERSION" build --workspace --release --locked
run 'host, client and CLI tests (release, locked)' cargo +"$ACTOR_RUST_VERSION" test --workspace --release --locked
run 'launchpad guests (release, locked)' cargo +"$ACTOR_RUST_VERSION" build --manifest-path methods/Cargo.toml --release --locked
check_guest_artifacts methods
run 'compatibility host and wire tools (release, locked)' cargo +"$ACTOR_RUST_VERSION" build --manifest-path "$base/Cargo.toml" --workspace --release --locked
run 'compatibility host and wire tests (release, locked)' cargo +"$ACTOR_RUST_VERSION" test --manifest-path "$base/Cargo.toml" --workspace --release --locked
run 'compatibility guests (release, locked)' cargo +"$ACTOR_RUST_VERSION" build --manifest-path "$base/methods/Cargo.toml" --release --locked
check_guest_artifacts "$base/methods"
run 'weighted-math host (release, locked)' cargo +"$ACTOR_RUST_VERSION" build --manifest-path packages/weighted-math/Cargo.toml --workspace --release --locked
run 'weighted-math host tests (release, locked)' cargo +"$ACTOR_RUST_VERSION" test --manifest-path packages/weighted-math/Cargo.toml --workspace --release --locked
run 'weighted-math guest parity (execution only, release, locked)' cargo +"$ACTOR_RUST_VERSION" run --manifest-path packages/weighted-math/zkvm/Cargo.toml --release --locked
run 'release overflow tests (host and guest execution, locked)' cargo +"$ACTOR_RUST_VERSION" test --manifest-path methods/Cargo.toml --release --features overflow-tests --test release_overflow --locked
if [ "${1:-}" = --execution ]; then
  run 'node-free token actor execution (locked)' cargo +"$ACTOR_RUST_VERSION" test --manifest-path "$base/execution/Cargo.toml" --locked
  run 'seeded sale trade and fee actor execution (locked)' cargo +"$ACTOR_RUST_VERSION" test --manifest-path methods/Cargo.toml --locked --features actor-execution-tests --test actor_sale
  if [ -z "${LAUNCHPAD_CLOCK_ELF:-}" ]; then
    LAUNCHPAD_CLOCK_ELF="$(cargo +"$ACTOR_RUST_VERSION" metadata --format-version 1 --locked | python3 -c '
import json, sys
from pathlib import Path
package = next(p for p in json.load(sys.stdin)["packages"] if p["name"] == "clock_core")
for parent in Path(package["manifest_path"]).parents:
    artifact = parent / "artifacts/lez/programs/clock.bin"
    if artifact.is_file():
        print(artifact)
        break
')" || fail 'cannot locate pinned upstream clock artifact'
  fi
  [ -s "${LAUNCHPAD_CLOCK_ELF:-}" ] || fail 'set LAUNCHPAD_CLOCK_ELF to the pinned upstream artifacts/lez/programs/clock.bin'
  export LAUNCHPAD_CLOCK_ELF
  for entry in "AUTHORITY:$base/methods:compat_authority" "TOKEN:$base/methods:compat_token" "OWNER:$base/methods:authority_owner_fixture" "CURVE:methods:curve" "FACTORY:methods:factory" "ADAPTER:methods:graduation_adapter" "GRADUATION_FAULT:methods:graduation_fault_adapter"; do
    IFS=: read -r role methods guest <<< "$entry"
    artifact="$(find "$methods/target/riscv-guest" -type f -path "*/release/$guest.bin" -size +0c -print -quit)"
    [ -n "$artifact" ] || fail "missing role execution guest: $guest.bin"
    export "LAUNCHPAD_${role}_ELF=$root/$artifact"
  done
  run 'public curve contract actor execution (locked)' cargo +"$ACTOR_RUST_VERSION" test -p launchpad-client --locked --features actor-execution-tests --test role_authority_guest -- --ignored
  printf 'check-builds: selected actor execution fixtures passed; service integration, real proofs and deployment readiness not tested\n'
else
  printf 'check-builds: token and sale actor execution deferred; select --execution for available real fixtures\n'
fi
