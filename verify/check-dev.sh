#!/usr/bin/env bash
set -euo pipefail
[ "$#" = 0 ] || { printf 'usage: check-dev.sh\n' >&2; exit 2; }
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$root"
source verify/actor-tools.env
if [ "${CURVE_EMERGENCY_AUTHORITY+x}" != x ]; then
  CURVE_EMERGENCY_AUTHORITY="$(< crates/curve-core/tests/fixtures/emergency-authority.hex)"
fi
export CURVE_EMERGENCY_AUTHORITY
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
lockfiles=(Cargo.lock packages/lez-compat/Cargo.lock packages/weighted-math/Cargo.lock packages/launchpad/core/logic/Cargo.lock)
before="$(shasum -a 256 "${lockfiles[@]}")"
check_locks() {
  local status=$?
  trap - EXIT
  if [ "$before" != "$(shasum -a 256 "${lockfiles[@]}")" ]; then
    printf 'check-dev: lockfiles changed during tests\n' >&2
    exit 1
  fi
  if [ "$status" = 0 ]; then
    printf 'check-dev: host tests passed; lockfiles unchanged; client, CLI, guests and execution checks are manual\n'
  fi
  exit "$status"
}
trap check_locks EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
run() {
  local stage="$1"
  shift
  printf '\ncheck-dev: %s\n' "$stage"
  "$@"
}
run 'core host tests (locked)' cargo +"$ACTOR_RUST_VERSION" test --locked
# TEMPORARY: exercise clock replies with an explicit fake provider after normal builds.
run 'weighted host tests with local fake clock (locked)' env LAUNCHPAD_MOCK_CLOCK=1 cargo +"$ACTOR_RUST_VERSION" test --locked -p lbp-core
run 'compatibility host tests (locked)' cargo +"$ACTOR_RUST_VERSION" test --manifest-path packages/lez-compat/Cargo.toml --workspace --locked
run 'weighted-math host tests (locked)' cargo +"$ACTOR_RUST_VERSION" test --manifest-path packages/weighted-math/Cargo.toml --workspace --locked
run 'launchpad core logic tests (locked)' cargo +"$ACTOR_RUST_VERSION" test --manifest-path packages/launchpad/core/logic/Cargo.toml --locked
