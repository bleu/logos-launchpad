#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
export CURVE_EMERGENCY_AUTHORITY="$(< crates/curve-core/tests/fixtures/emergency-authority.hex)"

check_destination() {
  EXPECTED_LAUNCHPAD_AUTHORITY_PROGRAM_ACCOUNT="$1" cargo test -p curve-core --locked \
    --test namespace_admin namespace_signer_creates_its_config_without_an_authority_nft -- --exact
}

check_invalid() {
  local output
  if output="$(LAUNCHPAD_AUTHORITY_PROGRAM_ACCOUNT="$1" cargo check -p curve-core --locked 2>&1)"; then
    printf 'role-authority-address: build accepted an invalid address\n' >&2
    return 1
  fi
  if [[ "$output" != *"LAUNCHPAD_AUTHORITY_PROGRAM_ACCOUNT $2"* ]]; then
    printf 'role-authority-address: missing diagnostic\n%s\n' "$output" >&2
    return 1
  fi
}

unset LAUNCHPAD_AUTHORITY_PROGRAM_ACCOUNT
check_destination 7N9DTgzWP2o7PFW9EYHja9z39EdvnVoDWYqGC1g17a3C
LAUNCHPAD_AUTHORITY_PROGRAM_ACCOUNT="$(printf '%064x' 1)" \
  check_destination 11111111111111111111111111111112
check_invalid '' 'must be exactly 64 hex characters'
check_invalid "$(printf '%063x' 1)z" 'must contain only hexadecimal digits'
check_invalid "$(printf '%064x' 0)" 'must not be the default account ID'
check_destination 7N9DTgzWP2o7PFW9EYHja9z39EdvnVoDWYqGC1g17a3C
printf 'role-authority-address: passed\n'
