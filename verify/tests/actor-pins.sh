#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
output="$(bash verify/check-pins.sh --actor)"
case "$output" in
  *'actor LEZ pinned at 578ce58a71b79c49d48e97e51c520bf71f0252d9'*) ;;
  *) printf 'actor-pins: wrong target\n%s\n' "$output" >&2; exit 1 ;;
esac
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
for file in verify/check-pins.sh verify/check-actor-pins.sh verify/actor-tools.env packages/lez-compat/Cargo.toml packages/lez-compat/Cargo.lock packages/lez-compat/rust-toolchain.toml packages/lez-compat/methods/Cargo.lock packages/lez-compat/methods/guest/Cargo.toml packages/lez-compat/methods/guest/Cargo.lock packages/lez-compat/execution/Cargo.toml packages/lez-compat/execution/Cargo.lock; do
  mkdir -p "$fixture/$(dirname "$file")"
  cp "$root/$file" "$fixture/$file"
done
manifest="$fixture/packages/lez-compat/methods/guest/Cargo.toml"
python3 - "$manifest" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
path.write_text(path.read_text().replace("578ce58a71b79c49d48e97e51c520bf71f0252d9", "0" * 40))
PY
if bash "$fixture/verify/check-pins.sh" --actor > "$fixture/output" 2>&1; then
  echo 'actor-pins: accepted a different guest revision' >&2; exit 1
fi
grep -q 'actor LEZ revision differs' "$fixture/output"
cp "$root/packages/lez-compat/methods/guest/Cargo.toml" "$manifest"
lock="$fixture/packages/lez-compat/methods/guest/Cargo.lock"
python3 - "$lock" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
path.write_text(path.read_text().replace("578ce58a71b79c49d48e97e51c520bf71f0252d9", "0" * 40))
PY
if bash "$fixture/verify/check-pins.sh" --actor > "$fixture/output" 2>&1; then
  echo 'actor-pins: accepted a different locked revision' >&2; exit 1
fi
grep -q 'actor LEZ lock revision differs' "$fixture/output"
cp "$root/packages/lez-compat/methods/guest/Cargo.lock" "$lock"
python3 - "$lock" <<'PY'
from pathlib import Path
import re
import sys
path = Path(sys.argv[1])
path.write_text(re.sub(r'(name = "risc0-zkos-v1compat"\nversion = ")[^"]+', r'\g<1>2.2.99', path.read_text()))
PY
if bash "$fixture/verify/check-pins.sh" --actor > "$fixture/output" 2>&1; then
  echo 'actor-pins: accepted a different zkOS wrapper' >&2; exit 1
fi
grep -q 'zkOS version differs' "$fixture/output"
printf 'actor-pins: manifest and lock drift are rejected\n'
case "${1:-}" in
  '') bash verify/check-builds.sh --actor --preflight ;;
  --static) ;;
  *) echo 'usage: actor-pins.sh [--static]' >&2; exit 1 ;;
esac
