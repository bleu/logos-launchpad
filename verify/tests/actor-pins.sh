#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
output="$(bash verify/check-pins.sh --actor)"
case "$output" in
  *'actor LEZ pinned at f94ca8a7550fa0012222b9fb5990dfe44c54a3dc'*) ;;
  *) printf 'actor-pins: wrong target\n%s\n' "$output" >&2; exit 1 ;;
esac
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
for file in Cargo.toml Cargo.lock rust-toolchain.toml methods/Cargo.toml methods/Cargo.lock methods/guest/Cargo.toml methods/guest/Cargo.lock verify/check-pins.sh verify/check-actor-pins.sh verify/actor-tools.env packages/lez-compat/Cargo.toml packages/lez-compat/Cargo.lock packages/lez-compat/rust-toolchain.toml packages/lez-compat/methods/Cargo.lock packages/lez-compat/methods/guest/Cargo.toml packages/lez-compat/methods/guest/Cargo.lock packages/lez-compat/execution/Cargo.toml packages/lez-compat/execution/Cargo.lock; do
  mkdir -p "$fixture/$(dirname "$file")"
  cp "$root/$file" "$fixture/$file"
done
python3 - "$fixture/methods/Cargo.toml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
path.write_text(path.read_text().replace('f94ca8a7550fa0012222b9fb5990dfe44c54a3dc', '0' * 40))
PY
if bash "$fixture/verify/check-pins.sh" > "$fixture/output" 2>&1; then
  printf 'actor-pins: accepted a different execution manifest revision\n' >&2; exit 1
fi
grep -q 'actor LEZ revision differs in methods/Cargo.toml' "$fixture/output"
cp "$root/methods/Cargo.toml" "$fixture/methods/Cargo.toml"
python3 - "$fixture/methods/Cargo.lock" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
path.write_text(path.read_text().replace('f94ca8a7550fa0012222b9fb5990dfe44c54a3dc', '0' * 40))
PY
if bash "$fixture/verify/check-pins.sh" > "$fixture/output" 2>&1; then
  echo 'actor-pins: accepted a different execution LEZ revision' >&2; exit 1
fi
grep -q 'actor LEZ lock revision differs in methods/Cargo.lock' "$fixture/output"
cp "$root/methods/Cargo.lock" "$fixture/methods/Cargo.lock"
manifest="$fixture/packages/lez-compat/methods/guest/Cargo.toml"
python3 - "$manifest" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
path.write_text(path.read_text().replace("f94ca8a7550fa0012222b9fb5990dfe44c54a3dc", "0" * 40))
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
path.write_text(path.read_text().replace("f94ca8a7550fa0012222b9fb5990dfe44c54a3dc", "0" * 40))
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
cp "$root/packages/lez-compat/methods/guest/Cargo.lock" "$lock"
python3 - "$fixture/Cargo.lock" <<'PY'
from pathlib import Path
import re
import sys
path = Path(sys.argv[1])
path.write_text(re.sub(r'(name = "risc0-zkos-v1compat"\nversion = ")[^"]+', r'\g<1>2.2.99', path.read_text()))
PY
if bash "$fixture/verify/check-pins.sh" > "$fixture/output" 2>&1; then
  echo 'actor-pins: accepted a different application zkOS wrapper' >&2; exit 1
fi
grep -q 'zkOS version differs in Cargo.lock' "$fixture/output"
cp "$root/Cargo.lock" "$fixture/Cargo.lock"
python3 - "$fixture/rust-toolchain.toml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
path.write_text(path.read_text().replace('1.98.1', '1.97.0'))
PY
if bash "$fixture/verify/check-pins.sh" > "$fixture/output" 2>&1; then
  echo 'actor-pins: accepted a different application Rust pin' >&2; exit 1
fi
grep -q 'actor host Rust pin differs in rust-toolchain.toml' "$fixture/output"
printf 'actor-pins: manifest and lock drift are rejected\n'
case "${1:-}" in
  '') bash verify/check-builds.sh --actor --preflight ;;
  --static) ;;
  *) echo 'usage: actor-pins.sh [--static]' >&2; exit 1 ;;
esac
