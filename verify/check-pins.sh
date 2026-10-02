#!/usr/bin/env bash
# The LEZ revision is duplicated across two workspaces with two lockfiles, and cargo has
# no way to factor it out. Drift fails silently at runtime: the host and the guest
# disagree on how state serialises and on what PDA seeds hash to. `lgs doctor` does not
# catch it, because it compares the configured pin against scaffold's default rather than
# against our manifests.
#
# Written for bash 3.2, which is what macOS ships.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

manifests="Cargo.toml methods/guest/Cargo.toml"

fail() {
  echo "check-pins: $1" >&2
  exit 1
}

if [ "${1:-}" = --tools ]; then
  expected_rust="$(awk -F '"' '/^channel = / {print $2}' rust-toolchain.toml)"
  command -v rustc >/dev/null 2>&1 || fail 'missing required tool: rustc'
  actual_rust="$(rustc --version | cut -d ' ' -f 2)"
  [ "$actual_rust" = "$expected_rust" ] || fail "expected Rust $expected_rust, found $actual_rust"
  actual_cargo="$(cargo --version | cut -d ' ' -f 2)"
  [ "$actual_cargo" = "$expected_rust" ] || fail "expected Cargo $expected_rust, found $actual_cargo"
  math_rust="$(awk -F '"' '/^channel = / {print $2}' packages/weighted-math/zkvm/rust-toolchain.toml)"
  [ "$math_rust" = "$expected_rust" ] || fail "math zkVM must pin Rust $expected_rust, found $math_rust"
  source verify/build-tools.env
  command -v lgs >/dev/null 2>&1 || fail 'missing required tool: lgs'
  scaffold_root="$(cd "$(dirname "$(command -v lgs)")/.." && pwd)"
  scaffold_metadata="$scaffold_root/.crates.toml"
  if [ ! -f "$scaffold_metadata" ] || ! grep '^"logos-scaffold ' "$scaffold_metadata" | grep -Fq "#$SCAFFOLD_REV)"; then
    fail "expected scaffold revision $SCAFFOLD_REV; install it with cargo install --rev under a separate --root"
  fi
  printf 'check-pins: %s, source %s\n' "$(lgs --version)" "$SCAFFOLD_REV"
  executor="${RISC0_SERVER_PATH:-$(command -v r0vm || true)}"
  [ -n "$executor" ] && [ -x "$executor" ] || fail 'missing required tool: r0vm'
  actual_r0vm="$("$executor" --version | cut -d ' ' -f 2)"
  [ "$actual_r0vm" = "$RISC0_TOOLS_VERSION" ] || fail "expected r0vm $RISC0_TOOLS_VERSION, found $actual_r0vm"
  command -v rzup >/dev/null 2>&1 || fail 'missing required tool: rzup'
  actual_rzup="$(rzup --version | cut -d ' ' -f 2)"
  [ "$actual_rzup" = "$RZUP_VERSION" ] || fail "expected rzup $RZUP_VERSION, found $actual_rzup"
  actual_risczero="$(cargo risczero --version | cut -d ' ' -f 2)"
  [ "$actual_risczero" = "$RISC0_TOOLS_VERSION" ] || fail "expected cargo-risczero $RISC0_TOOLS_VERSION, found $actual_risczero"
  components="$(rzup show)"
  for pair in "rust:$RISC0_RUST_VERSION" "cpp:$RISC0_CPP_VERSION" "r0vm:$RISC0_TOOLS_VERSION" "cargo-risczero:$RISC0_TOOLS_VERSION"; do
    component="${pair%%:*}"
    expected="${pair#*:}"
    actual="$(printf '%s\n' "$components" | awk -v component="$component" '$0 == component {active=1; next} active && $1 == "*" {print $2; exit} active && NF == 0 {active=0}')"
    [ "$actual" = "$expected" ] || fail "expected active RISC0 $component $expected, found ${actual:-none}; run rzup install $component $expected"
  done
  printf 'check-pins: RISC0 tools %s, guest Rust %s, C++ %s\n' "$RISC0_TOOLS_VERSION" "$RISC0_RUST_VERSION" "$RISC0_CPP_VERSION"
fi

# One rev per dependency line, from every manifest that names the LEZ repository.
revs="$(
  grep -h 'logos-execution-zone' $manifests |
    grep -oE 'rev = "[0-9a-f]{40}"' |
    grep -oE '[0-9a-f]{40}' |
    sort -u
)"

# `scaffold.toml` holds pins for spel, basecamp and lgpm too, so read only the lez block.
pin="$(awk '/^\[repos\.lez\]/{f=1;next} /^\[/{f=0} f' scaffold.toml |
  grep -oE '[0-9a-f]{40}' | head -1)"

[ -n "$pin" ] || fail "no pin found under [repos.lez] in scaffold.toml"
[ -n "$revs" ] || fail "no LEZ rev found in $manifests"

count="$(printf '%s\n' "$revs" | wc -l | tr -d ' ')"
[ "$count" = 1 ] || fail "manifests disagree on the LEZ rev: $(echo "$revs" | tr '\n' ' ')"
[ "$revs" = "$pin" ] || fail "scaffold.toml pins $pin but the manifests use $revs"

echo "check-pins: LEZ pinned at $pin across $manifests and scaffold.toml"
