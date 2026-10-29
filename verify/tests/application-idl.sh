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
factory = schema['modules']['factory']
creation = next(message for message in factory['Message']['variants'] if message['name'] == 'CreateFactoryPool')
assert 'creator' in {field['name'] for field in creation['fields']}
assert 'creator_nft_definition' not in {field['name'] for field in creation['fields']}
state_fields = {field['name'] for field in factory['FactoryState']['fields']}
assert 'creator_role_id' in state_fields
assert not {'creator_nft_definition', 'creator_commitment'} & state_fields
bootstrap = compat['modules']['authority']['Message']['variants'][-1]
assert bootstrap['name'] == 'BootstrapTransferOnly'
assert bootstrap['tag'] == 12
assert [field['name'] for field in bootstrap['fields']] == ['authority']
assert not (root / 'private_buy.json').exists()
PY
printf 'application-idl: generated actor messages and dependencies\n'
