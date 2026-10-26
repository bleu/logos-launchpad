#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = --actor ]; then
  shift
fi
exec bash "$(dirname "${BASH_SOURCE[0]}")/check-actor-pins.sh" "$@"
