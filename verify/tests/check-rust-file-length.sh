#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

git -C "$tmp" init -q
mkdir "$tmp/src"
cd "$tmp/src"

lines() {
  awk -v count="$1" 'BEGIN { for (i = 0; i < count; i++) print "" }'
}

run_check() {
  status=0
  output="$(bash "$root/verify/check-rust-file-length.sh" 2>&1)" || status=$?
  if [[ "$status" != "$1" ]]; then
    printf 'expected exit %s, got %s\n%s\n' "$1" "$status" "$output" >&2
    exit 1
  fi
}

assert_contains() {
  [[ "$output" == *"$1"* ]] || { printf 'expected: %s\nactual: %s\n' "$1" "$output" >&2; exit 1; }
}

lines 1000 > 'at limit.rs'
lines 1001 > untracked.rs
git add -- 'at limit.rs'
run_check 0

{ lines 1000; printf '// no final newline'; } > 'over limit.rs'
{ printf '// ignore-tidy-filelength: \t\r\n'; lines 1000; } > blank.rs
{ printf 'const VALUE: u32 = 0;\n// ignore-tidy-filelength: outside the header\n'; lines 999; } > misplaced.rs
git add -- 'over limit.rs' blank.rs misplaced.rs
run_check 1
assert_contains 'src/over limit.rs: 1001 lines (limit: 1000)'
assert_contains 'src/blank.rs: 1001 lines (limit: 1000)'
assert_contains 'src/misplaced.rs: 1001 lines (limit: 1000)'
assert_contains '// ignore-tidy-filelength: <reason>'

{ printf '\n// Copyright notice\n// ignore-tidy-filelength: Keep the fixture in one file.\n'; lines 998; } > 'over limit.rs'
run_check 1
assert_contains 'src/blank.rs: 1001 lines (limit: 1000)'
[[ "$output" != *'src/over limit.rs:'* ]]
lines 1000 > blank.rs
lines 1000 > misplaced.rs
run_check 0

mv 'at limit.rs' "$tmp/removed"
run_check 1
assert_contains 'cannot read src/at limit.rs'

printf 'invalid index\n' > "$tmp/broken-index"
GIT_INDEX_FILE="$tmp/broken-index" run_check 128
[[ "$output" != *'Rust file lengths: passed'* ]]

printf 'Rust file-length regression tests: passed\n'
