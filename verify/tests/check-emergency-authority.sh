#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

missing_authority() {
  local output status=0
  output="$(env -u CURVE_EMERGENCY_AUTHORITY cargo check -p curve-core --locked 2>&1)" || status=$?
  [ "$status" != 0 ] || {
    printf 'missing-authority: build accepted an unset authority\n' >&2
    return 1
  }
  [[ "$output" == *'Set CURVE_EMERGENCY_AUTHORITY to a nonzero 64-character hex account ID'* ]] || {
    printf 'missing-authority: missing diagnostic\n%s\n' "$output" >&2
    return 1
  }
  printf 'missing-authority: passed\n'
}

invalid_length() {
  local authority output status fixture
  fixture="$(< "$root/crates/curve-core/tests/fixtures/emergency-authority.hex")"
  for authority in '' "${fixture:1}" "${fixture}0"; do
    status=0
    output="$(CURVE_EMERGENCY_AUTHORITY="$authority" cargo check -p curve-core --locked 2>&1)" || status=$?
    [ "$status" != 0 ] && [[ "$output" == *'CURVE_EMERGENCY_AUTHORITY must be exactly 64 hex characters'* ]] || {
      printf 'invalid-length: expected a length rejection, got exit %s\n%s\n' "$status" "$output" >&2
      return 1
    }
  done
  printf 'invalid-length: passed\n'
}

invalid_hex() {
  local output status=0 fixture
  fixture="$(< "$root/crates/curve-core/tests/fixtures/emergency-authority.hex")"
  output="$(CURVE_EMERGENCY_AUTHORITY="${fixture:0:63}z" cargo check -p curve-core --locked 2>&1)" || status=$?
  [ "$status" != 0 ] && [[ "$output" == *'CURVE_EMERGENCY_AUTHORITY must contain only hexadecimal digits'* ]] || {
    printf 'invalid-hex: expected a hex rejection, got exit %s\n%s\n' "$status" "$output" >&2
    return 1
  }
  printf 'invalid-hex: passed\n'
}

default_authority() {
  local output status=0
  output="$(CURVE_EMERGENCY_AUTHORITY="$(printf '%064d' 0)" cargo check -p curve-core --locked 2>&1)" || status=$?
  [ "$status" != 0 ] && [[ "$output" == *'CURVE_EMERGENCY_AUTHORITY must not be the default account ID'* ]] || {
    printf 'default-authority: expected a default-account rejection, got exit %s\n%s\n' "$status" "$output" >&2
    return 1
  }
  printf 'default-authority: passed\n'
}

valid_authority() {
  local fixture
  fixture="$(< "$root/crates/curve-core/tests/fixtures/emergency-authority.hex")"
  CURVE_EMERGENCY_AUTHORITY="$fixture" cargo test -p curve-core --test emergency namespace_creation_sets_deployment_emergency_authority_unfrozen --locked
  printf 'valid-authority: passed\n'
}

authority_cache() {
  local authority fixture output first_id='' current_id status=0
  fixture="$(< "$root/crates/curve-core/tests/fixtures/emergency-authority.hex")"
  for authority in "$fixture" "${fixture:0:62}01"; do
    output="$(CURVE_EMERGENCY_AUTHORITY="$authority" RISC0_BUILD_LOCKED=1 cargo build --manifest-path methods/Cargo.toml --release --locked 2>&1)" || {
      printf 'authority-cache: guest build failed\n%s\n' "$output" >&2
      return 1
    }
    output="$(CURVE_EMERGENCY_AUTHORITY="$authority" RISC0_DEV_MODE=1 cargo test -p launchpad-client --features guest-tests --test settlement signed_namespace_creation_uses_build_authority --locked -- --exact --nocapture 2>&1)" || {
      printf 'authority-cache: guest selected the wrong authority\n%s\n' "$output" >&2
      return 1
    }
    current_id="$(printf '%s\n' "$output" | grep '^curve image ID:')"
    if [ -z "$first_id" ]; then
      first_id="$current_id"
    else
      [ "$current_id" != "$first_id" ] || {
        printf 'authority-cache: changed authority kept the old guest image ID\n' >&2
        return 1
      }
    fi
  done
  output="$(env -u CURVE_EMERGENCY_AUTHORITY RISC0_BUILD_LOCKED=1 cargo build --manifest-path methods/Cargo.toml --release --locked 2>&1)" || status=$?
  [ "$status" != 0 ] && [[ "$output" == *'Set CURVE_EMERGENCY_AUTHORITY to a nonzero 64-character hex account ID'* ]] || {
    printf 'authority-cache: unset authority reused a cached guest, exit %s\n%s\n' "$status" "$output" >&2
    return 1
  }
  CURVE_EMERGENCY_AUTHORITY="$fixture" RISC0_BUILD_LOCKED=1 cargo build --manifest-path methods/Cargo.toml --release --locked
  printf 'authority-cache: passed; test-fixture guests restored\n'
}

case "${1:-all}" in
  missing-authority) missing_authority ;;
  invalid-length) invalid_length ;;
  invalid-hex) invalid_hex ;;
  default-authority) default_authority ;;
  valid-authority) valid_authority ;;
  authority-cache) authority_cache ;;
  all) missing_authority; invalid_length; invalid_hex; default_authority; valid_authority ;;
  *) printf 'Unknown test: %s\n' "$1" >&2; exit 2 ;;
esac
