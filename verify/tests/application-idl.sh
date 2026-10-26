#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
output="$(mktemp -d)"
trap 'rm -rf "$output"' EXIT
bash verify/generate-idl.sh --output-dir "$output"
python3 - "$output" <<'PY'
import json
from pathlib import Path
import sys
root = Path(sys.argv[1])
schema = json.loads((root / 'launchpad.json').read_text())
assert schema['encoding'] == 'borsh'
messages = schema['modules']['curve']['Message']['variants']
assert any(message['name'] == 'InitializeNamespace' for message in messages)
assert 'Pool' in schema['modules']['pool']
assert 'Error' not in schema['modules']['curve']
compat = json.loads((root / 'compat.json').read_text())
assert 'token' in compat['modules']
assert not (root / 'private_buy.json').exists()
PY
printf 'application-idl: generated actor messages and dependencies\n'
