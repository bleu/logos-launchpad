#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
for file in verify/check-pins.sh verify/check-actor-pins.sh verify/actor-tools.env Cargo.toml Cargo.lock rust-toolchain.toml methods/Cargo.toml methods/Cargo.lock methods/guest/Cargo.toml methods/guest/Cargo.lock packages/lez-compat/Cargo.toml packages/lez-compat/Cargo.lock packages/lez-compat/rust-toolchain.toml packages/lez-compat/methods/Cargo.lock packages/lez-compat/methods/guest/Cargo.toml packages/lez-compat/methods/guest/Cargo.lock packages/lez-compat/execution/Cargo.toml packages/lez-compat/execution/Cargo.lock; do
  mkdir -p "$fixture/$(dirname "$file")"
  cp "$root/$file" "$fixture/$file"
done
python3 - "$fixture" <<'PY'
from pathlib import Path
import re
import sys
root = Path(sys.argv[1])
revision = '578ce58a71b79c49d48e97e51c520bf71f0252d9'
for relative in ['Cargo.toml', 'methods/guest/Cargo.toml']:
    path = root / relative
    path.write_text(re.sub(r'(logos-execution-zone.git", rev = ")[0-9a-f]{40}', rf'\g<1>{revision}', path.read_text()))
for relative in ['Cargo.lock', 'methods/guest/Cargo.lock']:
    path = root / relative
    path.write_text(re.sub(r'(git\+https://github.com/logos-blockchain/logos-execution-zone.git\?)(?:rev|tag)=[^"\n]+', rf'\g<1>rev={revision}#{revision}', path.read_text()))
PY
output="$(bash "$fixture/verify/check-pins.sh")"
case "$output" in
  *'actor LEZ pinned at 578ce58a71b79c49d48e97e51c520bf71f0252d9'*) ;;
  *) printf 'application-pins: default command did not check the actor target\n%s\n' "$output" >&2; exit 1 ;;
esac
printf 'application-pins: default checks accept the pinned actor application\n'
