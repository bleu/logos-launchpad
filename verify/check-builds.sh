#!/usr/bin/env bash
set -euo pipefail
# Retain the old flag as an alias for the single supported platform.
[ "${1:-}" != --actor ] || shift
exec /bin/bash "${BASH_SOURCE[0]%/*}/check-actor-builds.sh" "$@"
