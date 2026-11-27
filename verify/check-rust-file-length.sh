#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
cd "$root"
limit=1000

git ls-files -z -- '*.rs' | {
  failed=0
  while IFS= read -r -d '' path; do
    if ! lines="$(awk '
      BEGIN { header = 1 }
      !/^[[:space:]]*($|\/\/)/ { header = 0 }
      header && /^\/\/ ignore-tidy-filelength:[[:space:]]*[^[:space:]]/ { exempt = 1 }
      END { if (!exempt) print NR }
    ' < "$path")"; then
      printf 'check-rust-file-length: cannot read %s\n' "$path" >&2
      exit 1
    fi
    if [[ -n "$lines" && "$lines" -gt "$limit" ]]; then
      printf '%s: %s lines (limit: %s)\n' "$path" "$lines" "$limit" >&2
      failed=1
    fi
  done
  if (( failed )); then
    printf 'Split the file or add // ignore-tidy-filelength: <reason> to its opening line comments.\n' >&2
  fi
  exit "$failed"
}
printf 'Rust file lengths: passed\n'
