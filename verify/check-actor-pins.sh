#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
source verify/actor-tools.env
fail() { printf 'check-pins: %s\n' "$1" >&2; exit 1; }
case "${1:-}" in ''|--tools) [ "$#" -le 1 ] || fail 'usage: check-actor-pins.sh [--tools]' ;; *) fail 'usage: check-actor-pins.sh [--tools]' ;; esac
base=packages/lez-compat
for manifest in "$base/Cargo.toml" "$base/methods/guest/Cargo.toml" "$base/execution/Cargo.toml"; do
  lines="$(grep 'git = "https://github.com/logos-blockchain/logos-execution-zone.git"' "$manifest")" || fail "missing actor LEZ dependency: $manifest"
  if printf '%s\n' "$lines" | grep -vF "rev = \"$ACTOR_LEZ_REV\"" >/dev/null; then
    fail "actor LEZ revision differs in $manifest"
  fi
done
for lock in "$base/Cargo.lock" "$base/methods/guest/Cargo.lock" "$base/execution/Cargo.lock"; do
  lines="$(grep '^source = "git+https://github.com/logos-blockchain/logos-execution-zone.git' "$lock")" || fail "missing actor LEZ lock source: $lock"
  if printf '%s\n' "$lines" | grep -vF "?rev=$ACTOR_LEZ_REV#$ACTOR_LEZ_REV\"" >/dev/null; then
    fail "actor LEZ lock revision differs in $lock"
  fi
done
for lock in "$base/Cargo.lock" "$base/methods/Cargo.lock" "$base/methods/guest/Cargo.lock" "$base/execution/Cargo.lock"; do
  awk -v expected="$ACTOR_RISC0_VERSION" '
    /^name = / {name=$3}
    /^version = / && (name == "\"risc0-zkvm\"" || name == "\"risc0-build\"") {
      version=$3; gsub(/"/, "", version); found=1; if (version != expected) exit 1
    }
    END {if (!found) exit 1}
  ' "$lock" || fail "RISC0 version differs in $lock"
  awk -v expected="$ACTOR_ZKOS_VERSION" '
    /^name = / {name=$3}
    /^version = / && name == "\"risc0-zkos-v1compat\"" {
      version=$3; gsub(/"/, "", version); found=1; if (version != expected) exit 1
    }
    END {if (!found) exit 1}
  ' "$lock" || fail "zkOS version differs in $lock"
done
grep -Fx "channel = \"$ACTOR_RUST_VERSION\"" "$base/rust-toolchain.toml" >/dev/null || fail 'actor host Rust pin differs'
if [ "${1:-}" = --tools ]; then
  actual="$(rustc +"$ACTOR_RUST_VERSION" --version | cut -d ' ' -f 2)"
  [ "$actual" = "$ACTOR_RUST_VERSION" ] || fail "expected actor Rust $ACTOR_RUST_VERSION, found $actual"
  actual="$(cargo +"$ACTOR_RUST_VERSION" --version | cut -d ' ' -f 2)"
  [ "$actual" = "$ACTOR_RUST_VERSION" ] || fail "expected actor Cargo $ACTOR_RUST_VERSION, found $actual"
  executor="${RISC0_SERVER_PATH:-$(command -v r0vm || true)}"
  [ -n "$executor" ] && [ -x "$executor" ] || fail 'missing required tool: r0vm'
  actual="$("$executor" --version | cut -d ' ' -f 2)"
  [ "$actual" = "$ACTOR_RISC0_VERSION" ] || fail "expected r0vm $ACTOR_RISC0_VERSION, found $actual"
  actual="$(cargo +"$ACTOR_RUST_VERSION" risczero --version | cut -d ' ' -f 2)"
  [ "$actual" = "$ACTOR_RISC0_VERSION" ] || fail "expected cargo-risczero $ACTOR_RISC0_VERSION, found $actual"
  actual="$(rzup --version | cut -d ' ' -f 2)"
  [ "$actual" = "$ACTOR_RZUP_VERSION" ] || fail "expected rzup $ACTOR_RZUP_VERSION, found $actual"
  components="$(rzup show)"
  for pair in "rust:$ACTOR_GUEST_RUST_VERSION" "cpp:$ACTOR_CPP_VERSION" "r0vm:$ACTOR_RISC0_VERSION" "cargo-risczero:$ACTOR_RISC0_VERSION"; do
    component="${pair%%:*}"
    expected="${pair#*:}"
    actual="$(printf '%s\n' "$components" | awk -v component="$component" '$0 == component {active=1; next} active && $1 == "*" {print $2; exit} active && NF == 0 {active=0}')"
    [ "$actual" = "$expected" ] || fail "expected active RISC0 $component $expected, found ${actual:-none}"
  done
fi
printf 'check-pins: actor LEZ pinned at %s; RISC0 %s; host Rust %s\n' "$ACTOR_LEZ_REV" "$ACTOR_RISC0_VERSION" "$ACTOR_RUST_VERSION"
