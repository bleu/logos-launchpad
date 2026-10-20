#!/usr/bin/env bash
set -euo pipefail

if [ "${1:-}" = --actor ]; then
  [ "$#" = 1 ] || { echo 'usage: check-idl.sh --actor' >&2; exit 1; }
  root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  cd "$root"
  source verify/actor-tools.env
  generated="$(mktemp)"
  trap 'rm -f "$generated"' EXIT
  cargo +"$ACTOR_RUST_VERSION" run --quiet --manifest-path packages/lez-compat/Cargo.toml -p lez-compat-tools --locked -- idl > "$generated"
  cmp -s "$generated" packages/lez-compat/idl/compat.json || {
    echo 'actor IDL is stale; run bash verify/generate-idl.sh --actor' >&2
    exit 1
  }
  cargo +"$ACTOR_RUST_VERSION" test --manifest-path packages/lez-compat/Cargo.toml -p lez-compat-core --test wire --locked
  exit 0
fi

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

for program in curve factory lbp private_buy; do
  generated=$(mktemp)
  trap 'rm -f "$generated"' EXIT
  lgs spel -- generate-idl "idl-src/${program}.rs" > "$generated"
  cmp --silent "$generated" "idl/${program}.json" || {
    echo "IDL for ${program} is stale; run ./verify/generate-idl.sh" >&2
    exit 1
  }
  rm -f "$generated"
  trap - EXIT
done

cargo test -p launchpad-client --features idl-tests --test idl_wire --locked
