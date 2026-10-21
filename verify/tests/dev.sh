#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/bin"
cat > "$fixture/bin/cargo" <<'PY'
#!/usr/bin/env python3
import json
import os
import sys
keys = ['CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG', 'CARGO_INCREMENTAL',
        'CARGO_PROFILE_DEV_INCREMENTAL', 'CARGO_PROFILE_TEST_INCREMENTAL',
        'CARGO_TARGET_DIR', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
        'SCCACHE_DIR', 'SCCACHE_CACHE_SIZE', 'SCCACHE_SERVER_UDS', 'CURVE_EMERGENCY_AUTHORITY']
with open(os.environ['DEV_TEST_LOG'], 'w') as log:
    json.dump({'args': sys.argv[1:], 'env': {key: os.environ.get(key) for key in keys}}, log)
sys.exit(int(os.environ.get('DEV_TEST_EXIT', '0')))
PY
chmod +x "$fixture/bin/cargo"
export PATH="$fixture/bin:$PATH"
export DEV_TEST_LOG="$fixture/cargo.json"
export LAUNCHPAD_DEV_CACHE=off
export CARGO_TARGET_DIR="$fixture/unrelated-target"
unset CURVE_EMERGENCY_AUTHORITY
bash "$root/verify/dev.sh" actor check
python3 - "$DEV_TEST_LOG" "$root" <<'PY'
import json
import sys
with open(sys.argv[1]) as source:
    run = json.load(source)
assert run['args'] == ['+1.98.1', 'check', '--manifest-path', sys.argv[2] + '/packages/lez-compat/Cargo.toml',
                       '--locked', '--target-dir', sys.argv[2] + '/.local/launchpad/dev/actor/target']
for key in ['CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG']:
    assert run['env'][key] == 'line-tables-only'
assert run['env']['CARGO_INCREMENTAL'] == '0'
assert run['env']['CARGO_PROFILE_DEV_INCREMENTAL'] == 'false'
assert run['env']['CARGO_PROFILE_TEST_INCREMENTAL'] == 'false'
assert run['env']['CARGO_TARGET_DIR'] is None
assert not run['env']['RUSTC_WRAPPER']
assert not run['env']['RUSTC_WORKSPACE_WRAPPER']
PY
[ "$CARGO_TARGET_DIR" = "$fixture/unrelated-target" ]
printf 'dev: compact host profile and separate target passed\n'
cat > "$fixture/bin/sccache" <<'SH'
#!/usr/bin/env bash
printf 'fixture cache stats\n'
SH
chmod +x "$fixture/bin/sccache"
export LAUNCHPAD_DEV_CACHE=required
export LAUNCHPAD_DEV_CACHE_DIR="$fixture/shared-cache"
for branch in one 'branch two'; do
  checkout="$fixture/$branch"
  mkdir -p "$checkout/verify"
  cp "$root/verify/dev.sh" "$root/verify/actor-tools.env" "$checkout/verify/"
  cp "$root/rust-toolchain.toml" "$checkout/"
  DEV_TEST_LOG="$fixture/$branch.json" bash "$checkout/verify/dev.sh" actor check
done
python3 - "$fixture" <<'PY'
import json
from pathlib import Path
import sys
base = Path(sys.argv[1])
one = json.loads((base / 'one.json').read_text())
two = json.loads((base / 'branch two.json').read_text())
assert one['args'][-1] != two['args'][-1]
for run in [one, two]:
    assert run['env']['RUSTC_WRAPPER'] == str(base / 'bin/sccache')
    assert run['env']['SCCACHE_DIR'] == str(base / 'shared-cache/objects')
    assert run['env']['SCCACHE_CACHE_SIZE'] == '5G'
    assert run['env']['SCCACHE_SERVER_UDS'] == str(base / 'shared-cache/server.sock')
PY
printf 'dev: worktrees use separate output and one bounded cache\n'
for flag in --release -r --profile=release --target-dir=/tmp/elsewhere --manifest-path=methods/Cargo.toml --config=build.incremental=true; do
  if bash "$root/verify/dev.sh" actor test "$flag" > "$fixture/rejected" 2>&1; then
    printf 'dev: accepted forbidden option %s\n' "$flag" >&2; exit 1
  fi
  grep -q 'use the verification commands' "$fixture/rejected"
done
printf 'dev: guest, release and output overrides are rejected\n'
LAUNCHPAD_DEV_CACHE=off bash "$root/verify/dev.sh" app test -p curve-math -- --nocapture
python3 - "$DEV_TEST_LOG" "$root" <<'PY'
import json
import sys
with open(sys.argv[1]) as source:
    run = json.load(source)
assert run['args'][0:2] == ['+1.94.0', 'test']
assert run['args'][-4:] == ['-p', 'curve-math', '--', '--nocapture']
assert run['env']['CURVE_EMERGENCY_AUTHORITY'] == open(sys.argv[2] + '/crates/curve-core/tests/fixtures/emergency-authority.hex').read().strip()
PY
LAUNCHPAD_DEV_CACHE=off bash "$root/verify/dev.sh" math check
python3 - "$DEV_TEST_LOG" "$root" <<'PY'
import json
import sys
with open(sys.argv[1]) as source:
    run = json.load(source)
assert run['args'][0:2] == ['+1.94.0', 'check']
assert run['args'][3] == sys.argv[2] + '/packages/weighted-math/Cargo.toml'
PY
bash "$root/verify/dev.sh" actor clean
python3 - "$DEV_TEST_LOG" "$root" <<'PY'
import json
import sys
with open(sys.argv[1]) as source:
    run = json.load(source)
assert run['args'][1] == 'clean'
assert run['args'][-1] == sys.argv[2] + '/.local/launchpad/dev/actor/target'
assert not run['env']['RUSTC_WRAPPER']
PY
printf 'dev: workspace selection, focused tests and scoped clean passed\n'
[ "$(bash "$root/verify/dev.sh" cache-stats)" = 'fixture cache stats' ]
mkdir -p "$fixture/no-cache"
for utility in dirname awk mkdir; do
  ln -s "$(command -v "$utility")" "$fixture/no-cache/$utility"
done
ln -s "$(python3 -c 'import sys; print(sys.executable)')" "$fixture/no-cache/python3"
cp "$fixture/bin/cargo" "$fixture/no-cache/cargo"
PATH="$fixture/no-cache" LAUNCHPAD_DEV_CACHE=auto /bin/bash "$root/verify/dev.sh" actor check > "$fixture/fallback" 2>&1
grep -q 'shared caching is off' "$fixture/fallback"
if PATH="$fixture/no-cache" LAUNCHPAD_DEV_CACHE=required /bin/bash "$root/verify/dev.sh" actor check > "$fixture/missing" 2>&1; then
  echo 'dev: required cache did not fail when sccache was absent' >&2; exit 1
fi
grep -q 'sccache is required' "$fixture/missing"
status=0
DEV_TEST_EXIT=42 LAUNCHPAD_DEV_CACHE=off bash "$root/verify/dev.sh" actor test > "$fixture/cargo-failure" 2>&1 || status=$?
[ "$status" = 42 ]
printf 'dev: cache availability, stats and Cargo errors passed\n'
