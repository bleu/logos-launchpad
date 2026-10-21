#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
source verify/actor-tools.env
fail() { printf 'check-builds: %s\n' "$1" >&2; exit 1; }
case "${1:-}" in ''|--preflight|--execution) [ "$#" -le 1 ] || fail 'usage: check-builds.sh --actor [--preflight|--execution]' ;; *) fail 'usage: check-builds.sh --actor [--preflight|--execution]' ;; esac
command -v cargo >/dev/null 2>&1 || fail 'missing required tool: cargo'
for variable in RISC0_SKIP_BUILD RISC0_BUILD_DEBUG RISC0_RUST_SRC RUSTC RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_TARGET_DIR CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS CC_riscv32im_risc0_zkvm_elf CFLAGS_riscv32im_risc0_zkvm_elf; do
  [ -z "$(printenv "$variable" || true)" ] || fail "unset $variable for reproducible builds"
done
bash verify/check-pins.sh --actor --tools
printf 'check-builds: actor prerequisites passed; scaffold and SPEL CLI are not required\n'
[ "${1:-}" != --preflight ] || exit 0
base=packages/lez-compat
lockfiles=("$base/Cargo.lock" "$base/methods/Cargo.lock" "$base/methods/guest/Cargo.lock" "$base/execution/Cargo.lock")
before="$(shasum -a 256 "${lockfiles[@]}")"
check_locks() {
  local status=$?
  trap - EXIT
  [ "$before" = "$(shasum -a 256 "${lockfiles[@]}")" ] || fail 'actor lockfiles changed'
  exit "$status"
}
trap check_locks EXIT
for manifest in "$base/Cargo.toml" "$base/methods/Cargo.toml" "$base/methods/guest/Cargo.toml" "$base/execution/Cargo.toml"; do
  cargo +"$ACTOR_RUST_VERSION" fmt --manifest-path "$manifest" --all -- --check
done
cargo +"$ACTOR_RUST_VERSION" test --manifest-path "$base/Cargo.toml" --workspace --locked
cargo +"$ACTOR_RUST_VERSION" clippy --manifest-path "$base/Cargo.toml" --workspace --all-targets --locked -- -D warnings
RISC0_BUILD_LOCKED=1 cargo +"$ACTOR_RUST_VERSION" build --manifest-path "$base/methods/Cargo.toml" --release --locked
bash verify/tests/actor-tools.sh
bash verify/tests/actor-pins.sh
if [ "${1:-}" = --execution ]; then
  RISC0_BUILD_LOCKED=1 cargo +"$ACTOR_RUST_VERSION" test --manifest-path "$base/execution/Cargo.toml" --locked
fi
printf 'check-builds: actor foundation passed; launchpad migration and real proofs remain separate\n'
