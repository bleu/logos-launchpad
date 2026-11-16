#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
source verify/actor-tools.env
output=''
if [ "${1:-}" = --actor ]; then shift; fi
if [ "${1:-}" = --output-dir ]; then
  [ "$#" = 2 ] || { printf 'usage: generate-idl.sh [--output-dir directory]\n' >&2; exit 1; }
  output="$2"
  shift 2
fi
[ "$#" = 0 ] || { printf 'usage: generate-idl.sh [--output-dir directory]\n' >&2; exit 1; }
temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT
cargo +"$ACTOR_RUST_VERSION" run --quiet --manifest-path packages/lez-compat/Cargo.toml -p lez-compat-tools --locked -- idl-source \
  eligibility_policy=crates/eligibility-policy/src/lib.rs \
  curve=crates/curve-core/src/lib.rs \
  pool=crates/pool/src/lib.rs \
  factory=crates/factory-core/src/lib.rs \
  lbp_setup=crates/factory-core/src/lbp_setup.rs \
  hook=crates/factory-core/src/hook.rs \
  lbp=crates/lbp-core/src/lib.rs > "$temporary/launchpad.json"
cargo +"$ACTOR_RUST_VERSION" run --quiet --manifest-path packages/lez-compat/Cargo.toml -p lez-compat-tools --locked -- idl > "$temporary/compat.json"
if [ -n "$output" ]; then
  mkdir -p "$output"
  cp "$temporary/launchpad.json" "$temporary/compat.json" "$output/"
else
  mkdir -p idl packages/lez-compat/idl
  cp "$temporary/launchpad.json" idl/launchpad.json
  cp "$temporary/compat.json" packages/lez-compat/idl/compat.json
fi
