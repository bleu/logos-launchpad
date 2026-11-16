#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
source "$root/verify/actor-tools.env"
cargo +"$ACTOR_RUST_VERSION" metadata --manifest-path "$root/Cargo.toml" --no-deps --format-version 1 --locked | python3 -c '
import json, sys
metadata = json.load(sys.stdin)
defaults = set(metadata["workspace_default_members"])
names = {p["name"] for p in metadata["packages"] if p["id"] in defaults}
expected = {"curve-core", "curve-math", "eligibility-policy", "factory-core", "lbp-core", "pool", "private-flow-core"}
assert names == expected, f"unexpected default packages: {names}"
'
python3 - "$root/.github/workflows/ci.yml" <<'PY'
import pathlib, re, sys
workflow = pathlib.Path(sys.argv[1]).read_text()
checks = workflow.split("\n  checks:\n", 1)[1].split("\n  builds:\n", 1)[0]
full = workflow.split("\n  builds:\n", 1)[1]
assert "run: bash verify/check-dev.sh" in checks
assert not re.search(r"^\s+(?:run: )?bash verify/(?:check-builds|check-idl)\.sh", checks, re.M)
assert "rzup install" not in checks
assert "if: ${{ github.event_name == 'workflow_dispatch' && inputs.full }}" in full
assert "run: bash verify/check-builds.sh --execution" in full
assert re.search(r"      full:\n(?:        .*\n)*        default: false", workflow)
print("light defaults and manual full CI: passed")
PY

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
fixture="$tmp/source"
mkdir -p "$fixture/verify" "$fixture/crates/curve-core/tests/fixtures" "$tmp/bin"
cp "$root/verify/check-dev.sh" "$root/verify/actor-tools.env" "$fixture/verify/"
cp "$root/crates/curve-core/tests/fixtures/emergency-authority.hex" "$fixture/crates/curve-core/tests/fixtures/"
lockfiles=(Cargo.lock packages/lez-compat/Cargo.lock packages/weighted-math/Cargo.lock packages/launchpad/core/logic/Cargo.lock)
for lock in "${lockfiles[@]}"; do
  mkdir -p "$fixture/$(dirname "$lock")"
  printf 'fixture lock\n' > "$fixture/$lock"
done
cat_script='#!/bin/bash
printf "%s\n" "$*" >> "$COMMAND_LOG"
printf "%s\n" "$CURVE_EMERGENCY_AUTHORITY" >> "$AUTHORITY_LOG"
if [[ -n "${DRIFT_LOCK:-}" ]]; then printf "changed\n" >> "$DRIFT_LOCK"; fi
if [[ -n "${FAIL_COMMAND:-}" && "$*" == *"$FAIL_COMMAND"* ]]; then exit 19; fi
'
printf '%s' "$cat_script" > "$tmp/bin/cargo"
chmod +x "$tmp/bin/cargo"
export PATH="$tmp/bin:/usr/bin:/bin" COMMAND_LOG="$tmp/commands" AUTHORITY_LOG="$tmp/authority"
run_check() {
  : > "$COMMAND_LOG"
  : > "$AUTHORITY_LOG"
  status=0
  output="$(bash "$fixture/verify/check-dev.sh" "$@" 2>&1)" || status=$?
}
assert_contains() {
  [[ "$1" == *"$2"* ]] || { printf 'expected: %s\nactual: %s\n' "$2" "$1" >&2; exit 1; }
}
unset CURVE_EMERGENCY_AUTHORITY
run_check
[ "$status" = 0 ] || { printf '%s\n' "$output" >&2; exit 1; }
expected="+$ACTOR_RUST_VERSION test --locked
+$ACTOR_RUST_VERSION test --manifest-path packages/lez-compat/Cargo.toml --workspace --locked
+$ACTOR_RUST_VERSION test --manifest-path packages/weighted-math/Cargo.toml --workspace --locked
+$ACTOR_RUST_VERSION test --manifest-path packages/launchpad/core/logic/Cargo.toml --locked"
[ "$(< "$COMMAND_LOG")" = "$expected" ] || { printf 'unexpected host test commands\n' >&2; exit 1; }
assert_contains "$output" 'host tests passed; lockfiles unchanged'
expected_authority="$(< "$root/crates/curve-core/tests/fixtures/emergency-authority.hex")"
while IFS= read -r authority; do [ "$authority" = "$expected_authority" ] || exit 1; done < "$AUTHORITY_LOG"
printf 'light host commands and default authority: passed\n'

export CURVE_EMERGENCY_AUTHORITY="${expected_authority:0:62}01"
run_check
[ "$status" = 0 ] || exit 1
while IFS= read -r authority; do [ "$authority" = "$CURVE_EMERGENCY_AUTHORITY" ] || exit 1; done < "$AUTHORITY_LOG"
export CURVE_EMERGENCY_AUTHORITY=''
run_check
[ "$status" = 0 ] || exit 1
[ -z "$(< "$AUTHORITY_LOG")" ] || exit 1
printf 'explicit authority values preserved: passed\n'

for command in 'test --locked' 'packages/lez-compat/Cargo.toml' 'packages/weighted-math/Cargo.toml' 'packages/launchpad/core/logic/Cargo.toml'; do
  export FAIL_COMMAND="$command"
  run_check
  unset FAIL_COMMAND
  [ "$status" = 19 ] || exit 1
  [[ "$output" != *'host tests passed'* ]] || exit 1
  last=''
  while IFS= read -r line; do last="$line"; done < "$COMMAND_LOG"
  assert_contains "$last" "$command"
done
printf 'failed tests stop the light check: passed\n'

for lock in "${lockfiles[@]}"; do
  export DRIFT_LOCK="$lock"
  run_check
  unset DRIFT_LOCK
  [ "$status" = 1 ] || exit 1
  assert_contains "$output" 'lockfiles changed during tests'
  [[ "$output" != *'host tests passed'* ]] || exit 1
done
export DRIFT_LOCK=Cargo.lock FAIL_COMMAND='test --locked'
run_check
unset DRIFT_LOCK FAIL_COMMAND
[ "$status" = 1 ] || exit 1
assert_contains "$output" 'lockfiles changed during tests'
run_check --full
[ "$status" = 2 ] && [ ! -s "$COMMAND_LOG" ] || exit 1
assert_contains "$output" 'usage: check-dev.sh'
printf 'lockfile drift and unexpected arguments rejected: passed\n'
