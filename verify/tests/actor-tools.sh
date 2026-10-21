#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
actual="$(cargo +1.98.1 run --quiet --manifest-path packages/lez-compat/Cargo.toml -p lez-compat-tools --locked -- encode-transfer 0101010101010101010101010101010101010101010101010101010101010101 0202020202020202020202020202020202020202020202020202020202020202 30)"
expected="02010101010101010101010101010101010101010101010101010101010101010102020202020202020202020202020202020202020202020202020202020202021e000000000000000000000000000000"
[ "$actual" = "$expected" ] || { echo 'actor-tools: transfer wire bytes differ' >&2; exit 1; }
printf 'actor-tools: transfer matches the Borsh wire fixture\n'
idl="$(mktemp)"
trap 'rm -f "$idl"' EXIT
cargo +1.98.1 run --quiet --manifest-path packages/lez-compat/Cargo.toml -p lez-compat-tools --locked -- idl > "$idl"
python3 - "$idl" <<'PY'
import json
import sys
with open(sys.argv[1]) as source:
    idl = json.load(source)
assert idl["encoding"] == "borsh"
transfer = idl["modules"]["token"]["Message"]["variants"][2]
assert transfer == {"tag": 2, "name": "Transfer", "fields": [
    {"name": "definition", "type": "AccountId"},
    {"name": "recipient", "type": "AccountId"},
    {"name": "amount", "type": "u128"}]}
PY
printf 'actor-tools: generated IDL describes the transfer wire format\n'
bash verify/check-idl.sh --actor
