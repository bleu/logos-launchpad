#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
source verify/actor-tools.env
application=idl/launchpad.json
compat=packages/lez-compat/idl/compat.json
if [ "${1:-}" = --actor ]; then shift; fi
if [ "${1:-}" = --schema-dir ]; then
  [ "$#" = 2 ] || { printf 'usage: check-idl.sh [--schema-dir directory]\n' >&2; exit 1; }
  application="$2/launchpad.json"
  compat="$2/compat.json"
  shift 2
fi
[ "$#" = 0 ] || { printf 'usage: check-idl.sh [--schema-dir directory]\n' >&2; exit 1; }
generated="$(mktemp -d)"
trap 'rm -rf "$generated"' EXIT
bash verify/generate-idl.sh --output-dir "$generated"
for entry in "launchpad:$application" "compat:$compat"; do
  name="${entry%%:*}"
  target="${entry#*:}"
  cmp -s "$generated/$name.json" "$target" || {
    printf '%s IDL is stale; run bash verify/generate-idl.sh\n' "$name" >&2
    exit 1
  }
done
cargo +"$ACTOR_RUST_VERSION" test --manifest-path packages/lez-compat/Cargo.toml -p lez-compat-core --test wire --locked
cargo +"$ACTOR_RUST_VERSION" test --manifest-path packages/lez-compat/Cargo.toml -p lez-compat-tools --test source_idl --locked
cargo +"$ACTOR_RUST_VERSION" test -p launchpad-client --test idl_wire --locked
