#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
fixture="$tmp/source"
mkdir -p "$fixture/verify" "$tmp/bin"
cp "$root/verify/check-builds.sh" "$root/verify/check-actor-builds.sh" "$root/verify/actor-tools.env" "$fixture/verify/"
# These fixtures test shell commands and diagnostics, not guest execution or pins.
printf '#!/bin/bash\nprintf "pins %%s\\n" "$*" >> "$COMMAND_LOG"\n' > "$fixture/verify/check-pins.sh"
for lock in Cargo.lock methods/Cargo.lock methods/guest/Cargo.lock packages/lez-compat/Cargo.lock packages/lez-compat/methods/Cargo.lock packages/lez-compat/methods/guest/Cargo.lock packages/lez-compat/execution/Cargo.lock packages/weighted-math/Cargo.lock packages/weighted-math/fuzz/Cargo.lock packages/weighted-math/zkvm/Cargo.lock packages/weighted-math/zkvm/methods/guest/Cargo.lock; do
  mkdir -p "$fixture/$(dirname "$lock")"
  printf 'fixture lock\n' > "$fixture/$lock"
done
for path in methods/guest/src/bin/curve.rs packages/lez-compat/methods/guest/src/bin/compat_token.rs; do
  mkdir -p "$fixture/$(dirname "$path")"
  printf 'fixture source\n' > "$fixture/$path"
done
cat_script='#!/bin/bash
printf "cargo %s\n" "$*" >> "$COMMAND_LOG"
if [[ "$*" == *"metadata --format-version 1 --locked"* ]]; then
  mkdir -p upstream/artifacts/lez/programs upstream/programs/clock/core
  printf binary > upstream/artifacts/lez/programs/clock.bin
  printf "{\"packages\":[{\"name\":\"clock_core\",\"manifest_path\":\"%s/upstream/programs/clock/core/Cargo.toml\"}]}\n" "$PWD"
fi
if [[ -n "${FAIL_COMMAND:-}" && "$*" == *"$FAIL_COMMAND"* ]]; then exit 19; fi
if [[ -n "${DRIFT_LOCK:-}" ]]; then printf "changed\n" >> "$DRIFT_LOCK"; fi
if [[ "$*" == *"build --manifest-path methods/Cargo.toml"* ]]; then
  printf "guest-clock %s\n" "${LAUNCHPAD_MOCK_CLOCK:-unset}" >> "$COMMAND_LOG"
fi
if [[ "${NO_ARTIFACTS:-}" != 1 && "$*" == *"build --manifest-path methods/Cargo.toml"* ]]; then
  mkdir -p methods/target/riscv-guest/fixture/release
  printf binary > methods/target/riscv-guest/fixture/release/curve.bin
  printf binary > methods/target/riscv-guest/fixture/release/factory.bin
  printf binary > methods/target/riscv-guest/fixture/release/graduation_adapter.bin
  printf binary > methods/target/riscv-guest/fixture/release/graduation_fault_adapter.bin
fi
if [[ "${NO_ARTIFACTS:-}" != 1 && "$*" == *"build --manifest-path packages/lez-compat/methods/Cargo.toml"* ]]; then
  mkdir -p packages/lez-compat/methods/target/riscv-guest/fixture/release
  printf binary > packages/lez-compat/methods/target/riscv-guest/fixture/release/compat_token.bin
  printf binary > packages/lez-compat/methods/target/riscv-guest/fixture/release/compat_authority.bin
  printf binary > packages/lez-compat/methods/target/riscv-guest/fixture/release/authority_owner_fixture.bin
fi
'
printf '%s' "$cat_script" > "$tmp/bin/cargo"
chmod +x "$tmp/bin/cargo"
export PATH="$tmp/bin:$PATH" COMMAND_LOG="$tmp/commands"
run_check() {
  : > "$COMMAND_LOG"
  status=0
  output="$(bash "$fixture/verify/check-builds.sh" "$@" 2>&1)" || status=$?
}
assert_contains() {
  [[ "$1" == *"$2"* ]] || { printf 'expected: %s\nactual: %s\n' "$2" "$1" >&2; exit 1; }
}
default_repository() {
  run_check
  [ "$status" = 0 ] || { printf '%s\n' "$output" >&2; exit 1; }
  local commands
  commands="$(< "$COMMAND_LOG")"
  assert_contains "$commands" 'cargo +1.98.1 build --workspace --release --locked'
  assert_contains "$commands" 'cargo +1.98.1 test --workspace --release --locked'
  assert_contains "$commands" 'cargo +1.98.1 build --manifest-path methods/Cargo.toml --release --locked'
  assert_contains "$commands" 'cargo +1.98.1 test --manifest-path packages/lez-compat/Cargo.toml --workspace --release --locked'
  assert_contains "$commands" 'cargo +1.98.1 build --manifest-path packages/lez-compat/methods/Cargo.toml --release --locked'
  assert_contains "$commands" 'cargo +1.98.1 test --manifest-path packages/weighted-math/Cargo.toml --workspace --release --locked'
  assert_contains "$commands" 'cargo +1.98.1 run --manifest-path packages/weighted-math/zkvm/Cargo.toml --release --locked'
  assert_contains "$commands" 'cargo +1.98.1 test --manifest-path methods/Cargo.toml --release --features overflow-tests --test release_overflow --locked'
  assert_contains "$output" 'sale actor execution deferred'
  [[ "$commands" != *guest-tests* && "$commands" != *execution/Cargo.toml* ]] || exit 1
  printf 'default-repository: passed\n'
}
missing_artifacts() {
  export NO_ARTIFACTS=1
  run_check
  [ "$status" = 1 ] || { printf 'missing guest artifacts accepted\n' >&2; exit 1; }
  assert_contains "$output" 'missing guest artifact: curve.bin'
  printf 'missing-artifacts: passed\n'
}
execution_lane() {
  unset LAUNCHPAD_CLOCK_ELF
  run_check --execution
  [ "$status" = 0 ] || exit 1
  assert_contains "$(< "$COMMAND_LOG")" 'cargo +1.98.1 test --manifest-path packages/lez-compat/execution/Cargo.toml --locked'
  assert_contains "$(< "$COMMAND_LOG")" 'cargo +1.98.1 test --manifest-path methods/Cargo.toml --release --locked --features actor-execution-tests --test actor_sale'
  assert_contains "$(< "$COMMAND_LOG")" 'cargo +1.98.1 test -p launchpad-client --release --locked --test role_authority_guest -- --ignored'
  assert_contains "$output" 'selected actor execution fixtures passed; service integration, real proofs and deployment readiness not tested'
  export LAUNCHPAD_CLOCK_ELF="$tmp/missing-clock.bin"
  run_check --execution
  [ "$status" = 1 ] || { printf 'missing role clock accepted\n' >&2; exit 1; }
  assert_contains "$output" 'set LAUNCHPAD_CLOCK_ELF to the pinned upstream'
  [[ "$(< "$COMMAND_LOG")" != *'--test role_authority_guest'* ]] || exit 1
  unset LAUNCHPAD_CLOCK_ELF
  printf 'execution-lane: passed\n'
}
clock_lanes() (
  unset LAUNCHPAD_MOCK_CLOCK
  run_check
  [ "$status" = 0 ] || exit 1
  assert_contains "$(< "$COMMAND_LOG")" 'guest-clock unset'
  export LAUNCHPAD_MOCK_CLOCK=1
  run_check --execution
  [ "$status" = 0 ] || exit 1
  assert_contains "$(< "$COMMAND_LOG")" 'guest-clock 1'
  printf 'clock-lanes: passed\n'
)
regressions() {
  local variable command lock
  status=0
  output="$(PATH="$tmp/empty" /bin/bash "$fixture/verify/check-builds.sh" --preflight 2>&1)" || status=$?
  [ "$status" = 1 ] || exit 1
  assert_contains "$output" 'missing required tool: cargo'
  run_check --preflight
  [ "$status" = 0 ] || exit 1
  [ "$(< "$COMMAND_LOG")" = 'pins --tools' ] || exit 1
  for variable in RISC0_SKIP_BUILD RISC0_BUILD_DEBUG RISC0_RUST_SRC RUSTC RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_TARGET_DIR CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS CC_riscv32im_risc0_zkvm_elf CFLAGS_riscv32im_risc0_zkvm_elf; do
    export "$variable=override"
    run_check --preflight
    unset "$variable"
    [ "$status" = 1 ] || exit 1
    assert_contains "$output" "unset $variable for reproducible builds"
    [ ! -s "$COMMAND_LOG" ] || exit 1
  done
  run_check --release
  [ "$status" = 1 ] || exit 1
  assert_contains "$output" 'release builds require source at /tmp/logos-launchpad-release/source'
  for command in 'build --workspace' 'test --workspace' 'build --manifest-path methods/Cargo.toml' 'test --manifest-path packages/lez-compat/Cargo.toml' 'build --manifest-path packages/lez-compat/methods/Cargo.toml' 'test --manifest-path packages/weighted-math/Cargo.toml' 'run --manifest-path packages/weighted-math/zkvm/Cargo.toml' 'test --manifest-path methods/Cargo.toml'; do
    export FAIL_COMMAND="$command"
    run_check
    unset FAIL_COMMAND
    [ "$status" = 1 ] || exit 1
    assert_contains "$output" 'stage failed:'
    [[ "$output" != *'repository checks passed'* ]] || { printf 'reported success before lock check\n' >&2; exit 1; }
    local last=''
    while IFS= read -r last_line; do last="$last_line"; done < "$COMMAND_LOG"
    [[ "$last" == *"$command"* ]] || exit 1
  done
  for lock in Cargo.lock methods/guest/Cargo.lock packages/lez-compat/methods/guest/Cargo.lock packages/weighted-math/zkvm/methods/guest/Cargo.lock; do
    export DRIFT_LOCK="$lock"
    run_check
    unset DRIFT_LOCK
    [ "$status" = 1 ] || exit 1
    assert_contains "$output" 'lockfiles changed during build'
    [[ "$output" != *'repository checks passed'* ]] || { printf 'reported success before lock check\n' >&2; exit 1; }
  done
  printf '#!/bin/bash\nprintf "pin failure\\n" >&2\nexit 17\n' > "$fixture/verify/check-pins.sh"
  run_check
  [ "$status" = 17 ] || exit 1
  assert_contains "$output" 'pin failure'
  [ ! -s "$COMMAND_LOG" ] || exit 1
  printf 'regressions: passed\n'
}
case "${1:-all}" in
  default-repository) default_repository ;;
  all) default_repository; execution_lane; clock_lanes; regressions ;;
  execution-lane) execution_lane ;;
  clock-lanes) clock_lanes ;;
  regressions) regressions ;;
  missing-artifacts) missing_artifacts ;;
  *) printf 'Unknown test: %s\n' "$1" >&2; exit 2 ;;
esac
