#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
bash verify/generate-idl.sh --output-dir "$fixture"
printf '{}\n' > "$fixture/launchpad.json"
if bash verify/check-idl.sh --schema-dir "$fixture" > "$fixture/output" 2>&1; then
  printf 'application-idl-drift: accepted stale application schema\n' >&2
  exit 1
fi
grep -q 'launchpad IDL is stale' "$fixture/output"
printf 'application-idl-drift: stale application schema is rejected\n'
