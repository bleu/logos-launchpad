#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d)"
release_parent=''
fixture_lock=''
cleanup() {
  [ -z "$fixture_lock" ] || rm -rf "$fixture_lock"
  [ -z "$release_parent" ] || rm -rf "$release_parent"
  rm -rf "$tmp"
}
trap cleanup EXIT

copy_source() {
  local destination="$1"
  mkdir -p "$destination"
  {
    git -C "$root" ls-files -z
    printf '%s\0' verify/check-builds.sh verify/build-tools.env verify/tests/check-builds.sh methods/tests/release_overflow.rs methods/guest/examples/release_overflow.rs
  } | (cd "$root" && tar -cf - --null -T -) | (cd "$destination" && tar -xf -)
}

missing_tool() {
  local output status=0
  output="$(PATH="$tmp" /bin/bash "$root/verify/check-builds.sh" --preflight 2>&1)" || status=$?
  [ "$status" = 1 ] || {
    printf 'missing-tool: expected exit 1, got %s\n%s\n' "$status" "$output" >&2
    return 1
  }
  case "$output" in
    *'check-builds: missing required tool: cargo'*) ;;
    *) printf 'missing-tool: missing diagnostic\n%s\n' "$output" >&2; return 1 ;;
  esac
  printf 'missing-tool: passed\n'
}

wrong_tool() {
  printf '#!/bin/bash\nprintf "cargo 1.93.0\\n"\n' > "$tmp/cargo"
  printf '#!/bin/bash\nprintf "rustc 1.93.0\\n"\n' > "$tmp/rustc"
  chmod +x "$tmp/cargo" "$tmp/rustc"
  local output status=0
  output="$(PATH="$tmp:/usr/bin:/bin" /bin/bash "$root/verify/check-builds.sh" --preflight 2>&1)" || status=$?
  [ "$status" = 1 ] || {
    printf 'wrong-tool: expected exit 1, got %s\n%s\n' "$status" "$output" >&2
    return 1
  }
  case "$output" in
    *'expected Rust 1.94.0, found 1.93.0'*) ;;
    *) printf 'wrong-tool: missing diagnostic\n%s\n' "$output" >&2; return 1 ;;
  esac
  printf 'wrong-tool: passed\n'
}

wrong_scaffold() {
  printf '#!/bin/bash\nprintf "cargo 1.94.0\\n"\n' > "$tmp/cargo"
  printf '#!/bin/bash\nprintf "rustc 1.94.0\\n"\n' > "$tmp/rustc"
  mkdir -p "$tmp/scaffold/bin"
  printf '#!/bin/bash\nprintf "logos-scaffold 0.3.0\\n"\n' > "$tmp/scaffold/bin/lgs"
  printf '"logos-scaffold 0.3.0 (git+https://github.com/logos-co/scaffold?tag=v0.3.0#6b5f125d12513a446379bf3056e742b19b773cb6)" = ["lgs"]\n' > "$tmp/scaffold/.crates.toml"
  chmod +x "$tmp/cargo" "$tmp/rustc" "$tmp/scaffold/bin/lgs"
  local output status=0
  output="$(PATH="$tmp/scaffold/bin:$tmp:/usr/bin:/bin" /bin/bash "$root/verify/check-builds.sh" --preflight 2>&1)" || status=$?
  [ "$status" = 1 ] || {
    printf 'wrong-scaffold: expected exit 1, got %s\n%s\n' "$status" "$output" >&2
    return 1
  }
  case "$output" in
    *'expected scaffold revision f919fd03b064a91a4f261093cf2b0b4f8150e27c'*) ;;
    *) printf 'wrong-scaffold: missing diagnostic\n%s\n' "$output" >&2; return 1 ;;
  esac
  printf 'wrong-scaffold: passed\n'
}

unpinned_math() {
  local snapshot="$tmp/snapshot"
  mkdir -p "$snapshot/verify" "$snapshot/methods/guest" "$snapshot/packages/weighted-math/zkvm"
  cp "$root/verify/check-builds.sh" "$root/verify/check-pins.sh" "$root/verify/build-tools.env" "$snapshot/verify/"
  cp "$root/Cargo.toml" "$root/scaffold.toml" "$root/rust-toolchain.toml" "$snapshot/"
  cp "$root/methods/guest/Cargo.toml" "$snapshot/methods/guest/"
  printf '[toolchain]\nchannel = "stable"\n' > "$snapshot/packages/weighted-math/zkvm/rust-toolchain.toml"
  printf '#!/bin/bash\nprintf "cargo 1.94.0\\n"\n' > "$tmp/cargo"
  printf '#!/bin/bash\nprintf "rustc 1.94.0\\n"\n' > "$tmp/rustc"
  mkdir -p "$tmp/scaffold/bin"
  printf '#!/bin/bash\nprintf "logos-scaffold 0.1.1\\n"\n' > "$tmp/scaffold/bin/lgs"
  printf '"logos-scaffold 0.1.1 (git+https://github.com/logos-co/scaffold?rev=f919fd03b064a91a4f261093cf2b0b4f8150e27c#f919fd03b064a91a4f261093cf2b0b4f8150e27c)" = ["lgs"]\n' > "$tmp/scaffold/.crates.toml"
  chmod +x "$tmp/cargo" "$tmp/rustc" "$tmp/scaffold/bin/lgs"
  local output status=0
  output="$(PATH="$tmp/scaffold/bin:$tmp:/usr/bin:/bin" /bin/bash "$snapshot/verify/check-builds.sh" --preflight 2>&1)" || status=$?
  [ "$status" = 1 ] || {
    printf 'unpinned-math: expected exit 1, got %s\n%s\n' "$status" "$output" >&2
    return 1
  }
  case "$output" in
    *'math zkVM must pin Rust 1.94.0, found stable'*) ;;
    *) printf 'unpinned-math: missing diagnostic\n%s\n' "$output" >&2; return 1 ;;
  esac
  printf 'unpinned-math: passed\n'
}

wrong_executor() {
  printf '#!/bin/bash\nprintf "cargo 1.94.0\\n"\n' > "$tmp/cargo"
  printf '#!/bin/bash\nprintf "rustc 1.94.0\\n"\n' > "$tmp/rustc"
  mkdir -p "$tmp/scaffold/bin"
  printf '#!/bin/bash\nprintf "logos-scaffold 0.1.1\\n"\n' > "$tmp/scaffold/bin/lgs"
  printf '"logos-scaffold 0.1.1 (git+https://github.com/logos-co/scaffold?rev=f919fd03b064a91a4f261093cf2b0b4f8150e27c#f919fd03b064a91a4f261093cf2b0b4f8150e27c)" = ["lgs"]\n' > "$tmp/scaffold/.crates.toml"
  printf '#!/bin/bash\nprintf "r0vm 0.0.0\\n"\n' > "$tmp/r0vm"
  chmod +x "$tmp/cargo" "$tmp/rustc" "$tmp/scaffold/bin/lgs" "$tmp/r0vm"
  local output status=0
  output="$(RISC0_SERVER_PATH="$tmp/r0vm" PATH="$tmp/scaffold/bin:$tmp:/usr/bin:/bin" /bin/bash "$root/verify/check-builds.sh" --preflight 2>&1)" || status=$?
  [ "$status" = 1 ] || {
    printf 'wrong-executor: expected exit 1, got %s\n%s\n' "$status" "$output" >&2
    return 1
  }
  case "$output" in
    *'expected r0vm 3.0.5, found 0.0.0'*) ;;
    *) printf 'wrong-executor: missing diagnostic\n%s\n' "$output" >&2; return 1 ;;
  esac
  printf 'wrong-executor: passed\n'
}

skip_guests() {
  printf '#!/bin/bash\nexit 0\n' > "$tmp/cargo"
  chmod +x "$tmp/cargo"
  local output status=0
  output="$(RISC0_SKIP_BUILD=1 PATH="$tmp:/usr/bin:/bin" /bin/bash "$root/verify/check-builds.sh" --preflight 2>&1)" || status=$?
  [ "$status" = 1 ] || {
    printf 'skip-guests: expected exit 1, got %s\n%s\n' "$status" "$output" >&2
    return 1
  }
  case "$output" in
    *'unset RISC0_SKIP_BUILD for reproducible builds'*) ;;
    *) printf 'skip-guests: missing diagnostic\n%s\n' "$output" >&2; return 1 ;;
  esac
  printf 'skip-guests: passed\n'
}

compiler_override() {
  printf '#!/bin/bash\nexit 0\n' > "$tmp/cargo"
  chmod +x "$tmp/cargo"
  local output status variable value
  for variable in CC_riscv32im_risc0_zkvm_elf CFLAGS_riscv32im_risc0_zkvm_elf RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER; do
    value=/usr/bin/clang
    [ "$variable" != CFLAGS_riscv32im_risc0_zkvm_elf ] || value=-O0
    status=0
    output="$(env "$variable=$value" PATH="$tmp:/usr/bin:/bin" /bin/bash "$root/verify/check-builds.sh" --preflight 2>&1)" || status=$?
    [ "$status" = 1 ] && [[ "$output" == *"unset $variable for reproducible builds"* ]] || {
      printf 'compiler-override: expected rejection of %s, got exit %s\n%s\n' "$variable" "$status" "$output" >&2
      return 1
    }
  done
  printf 'compiler-override: passed\n'
}

release_path() {
  local output status=0
  output="$(bash "$root/verify/check-builds.sh" --release 2>&1)" || status=$?
  [ "$status" = 1 ] && [[ "$output" == *'release builds require source at /tmp/logos-launchpad-release/source'* ]] || {
    printf 'release-path: expected fixed-path rejection, got exit %s\n%s\n' "$status" "$output" >&2
    return 1
  }
  printf 'release-path: passed\n'
}

prepare_release_source() {
  local parent=/tmp/logos-launchpad-release
  mkdir -m 700 "$parent" || {
    printf 'check-builds tests: release directory is occupied; left unchanged\n' >&2
    return 1
  }
  release_parent="$parent"
  if [ "$#" = 0 ]; then
    copy_source "$release_parent/source"
  else
    mkdir "$release_parent/source"
    (cd "$1" && tar -cf - .) | (cd "$release_parent/source" && tar -xf -)
  fi
}

release_busy() {
  prepare_release_source
  mkdir -m 700 /tmp/logos-launchpad-release.lock
  fixture_lock=/tmp/logos-launchpad-release.lock
  printf 'keep this lock\n' > "$fixture_lock/owner"
  mkdir -p "$tmp/commands"
  printf '#!/bin/bash\ncase "$*" in\n  --version) printf "cargo 1.94.0\\n" ;;\n  "risczero --version") printf "cargo-risczero 3.0.5\\n" ;;\n  *) exit 0 ;;\nesac\n' > "$tmp/commands/cargo"
  chmod +x "$tmp/commands/cargo"
  local output status=0
  output="$(PATH="$tmp/commands:$PATH" bash "$release_parent/source/verify/check-builds.sh" --release 2>&1)" || status=$?
  [ "$status" = 1 ] && [[ "$output" == *'release build lock is occupied'* ]] || {
    printf 'release-busy: expected occupied-lock rejection, got exit %s\n%s\n' "$status" "$output" >&2
    return 1
  }
  grep -q 'keep this lock' "$fixture_lock/owner"
  printf 'release-busy: passed; existing lock unchanged\n'
}

release_failed_stage() {
  prepare_release_source
  mkdir -p "$tmp/commands"
  printf '#!/bin/bash\ncase "$*" in\n  --version) printf "cargo 1.94.0\\n" ;;\n  "risczero --version") printf "cargo-risczero 3.0.5\\n" ;;\n  "test --workspace --release --locked") exit 19 ;;\n  *) exit 0 ;;\nesac\n' > "$tmp/commands/cargo"
  chmod +x "$tmp/commands/cargo"
  local output status=0
  output="$(PATH="$tmp/commands:$PATH" bash "$release_parent/source/verify/check-builds.sh" --release 2>&1)" || status=$?
  [ "$status" = 1 ] && [[ "$output" == *'stage failed: host, client and CLI tests (release, locked)'* ]] || {
    printf 'release-failed-stage: expected host failure, got exit %s\n%s\n' "$status" "$output" >&2
    return 1
  }
  [ ! -e /tmp/logos-launchpad-release.lock ] && [ ! -L /tmp/logos-launchpad-release.lock ] || {
    printf 'release-failed-stage: owned lock was not removed\n' >&2
    return 1
  }
  printf 'release-failed-stage: passed; owned lock removed\n'
}

nested_lock_drift() {
  local manifest=methods/guest/Cargo.toml lock=methods/guest/Cargo.lock
  if [ "${1:-curve}" = math ]; then
    manifest=packages/weighted-math/zkvm/methods/guest/Cargo.toml
    lock=packages/weighted-math/zkvm/methods/guest/Cargo.lock
  fi
  local snapshot="$tmp/drift-snapshot"
  copy_source "$snapshot"
  cp "$snapshot/$lock" "$tmp/guest.lock"
  perl -pi -e 's/^version = "[0-9.]+"$/version = "9.9.9"/' "$snapshot/$manifest"
  local status=0
  bash "$snapshot/verify/check-builds.sh" > "$tmp/drift.log" 2>&1 || status=$?
  [ "$status" != 0 ] || {
    printf 'nested-lock-drift: build accepted a stale guest lockfile\n' >&2
    return 1
  }
  grep -q -- '--locked' "$tmp/drift.log" && grep -q 'lock file' "$tmp/drift.log" || {
    printf 'nested-lock-drift: failure was not a locked build rejection\n' >&2
    return 1
  }
  cmp "$tmp/guest.lock" "$snapshot/$lock"
  printf 'nested-lock-drift: passed\n'
}

failed_host_test() {
  mkdir -p "$tmp/commands"
  printf '#!/bin/bash\ncase "$*" in\n  --version) printf "cargo 1.94.0\\n" ;;\n  "risczero --version") printf "cargo-risczero 3.0.5\\n" ;;\n  "test --workspace --release --locked") exit 19 ;;\n  *) exit 0 ;;\nesac\n' > "$tmp/commands/cargo"
  chmod +x "$tmp/commands/cargo"
  local status=0
  PATH="$tmp/commands:$PATH" bash "$root/verify/check-builds.sh" > "$tmp/failed-host.log" 2>&1 || status=$?
  [ "$status" = 1 ] && grep -q 'stage failed: host, client and CLI tests (release, locked)' "$tmp/failed-host.log" || {
    printf 'failed-host-test: failed host test was not reported\n' >&2
    return 1
  }
  printf 'failed-host-test: passed\n'
}

failed_math_test() {
  mkdir -p "$tmp/commands"
  printf '#!/bin/bash\ncase "$*" in\n  --version) printf "cargo 1.94.0\\n" ;;\n  "risczero --version") printf "cargo-risczero 3.0.5\\n" ;;\n  "test --manifest-path packages/weighted-math/Cargo.toml --workspace --release --locked") exit 19 ;;\n  *) exit 0 ;;\nesac\n' > "$tmp/commands/cargo"
  chmod +x "$tmp/commands/cargo"
  local status=0
  PATH="$tmp/commands:$PATH" bash "$root/verify/check-builds.sh" > "$tmp/failed-math.log" 2>&1 || status=$?
  [ "$status" = 1 ] && grep -q 'stage failed: weighted-math host tests (release, locked)' "$tmp/failed-math.log" || {
    printf 'failed-math-test: failed math test was not reported\n' >&2
    return 1
  }
  printf 'failed-math-test: passed\n'
}

failed_guest_test() {
  mkdir -p "$tmp/commands"
  printf '#!/bin/bash\ncase "$*" in\n  --version) printf "cargo 1.94.0\\n" ;;\n  "risczero --version") printf "cargo-risczero 3.0.5\\n" ;;\n  "test -p launchpad-client --release --features guest-tests --test private_authority --test settlement --locked") [ "${RISC0_DEV_MODE:-}" != 1 ] || exit 19 ;;\n  *) exit 0 ;;\nesac\n' > "$tmp/commands/cargo"
  chmod +x "$tmp/commands/cargo"
  local status=0
  PATH="$tmp/commands:$PATH" bash "$root/verify/check-builds.sh" > "$tmp/failed-guest.log" 2>&1 || status=$?
  [ "$status" = 1 ] && grep -q 'stage failed: curve guest tests (development receipts, release, locked)' "$tmp/failed-guest.log" || {
    printf 'failed-guest-test: failed development guest test was not reported\n' >&2
    return 1
  }
  printf 'failed-guest-test: passed\n'
}

changed_lockfile() {
  local snapshot="$tmp/lock-snapshot"
  mkdir -p "$tmp/commands"
  copy_source "$snapshot"
  printf '#!/bin/bash\ncase "$*" in\n  --version) printf "cargo 1.94.0\\n" ;;\n  "risczero --version") printf "cargo-risczero 3.0.5\\n" ;;\n  "build --workspace --release --locked") printf "\\n" >> Cargo.lock ;;\n  *) exit 0 ;;\nesac\n' > "$tmp/commands/cargo"
  chmod +x "$tmp/commands/cargo"
  local status=0
  PATH="$tmp/commands:$PATH" bash "$snapshot/verify/check-builds.sh" > "$tmp/changed-lock.log" 2>&1 || status=$?
  [ "$status" = 1 ] && grep -q 'lockfiles changed during build' "$tmp/changed-lock.log" || {
    printf 'changed-lockfile: lockfile change was not reported\n' >&2
    return 1
  }
  printf 'changed-lockfile: passed\n'
}

complete_build() {
  local snapshot="$tmp/complete-snapshot"
  copy_source "$snapshot"
  bash "$snapshot/verify/check-builds.sh" 2>&1 | tee "$tmp/build.log"
  [ -x "$snapshot/target/release/launchpad" ] || {
    printf 'complete-build: release CLI is missing\n' >&2
    return 1
  }
  "$snapshot/target/release/launchpad" --help > "$tmp/help"
  grep -q 'Usage:' "$tmp/help"
  grep -q 'all outputs bit-identical host vs guest' "$tmp/build.log"
  grep -q 'guest_release_arithmetic_rejects_overflow ... ok' "$tmp/build.log"
  (
    cd "$snapshot"
    printf 'Guest artifact SHA-256:\n'
    shasum -a 256 methods/target/riscv-guest/launchpad_methods/launchpad_programs/riscv32im-risc0-zkvm-elf/release/*.bin packages/weighted-math/zkvm/target/riscv-guest/zkvm-methods/weighted-math-guest/riscv32im-risc0-zkvm-elf/release/*.bin
    find methods/target/release/build packages/weighted-math/zkvm/target/release/build -name methods.rs -exec grep -h '^pub const .*_ID:' {} + | sort -u
  )
  printf 'complete-build: passed in a fresh source snapshot\n'

}

relocated_build() {
  local name snapshot
  for name in first second; do
    snapshot="$(mktemp -d "$tmp/$name.XXXXXXXX")/source"
    [ "$name" != second ] || snapshot="$snapshot/nested"
    copy_source "$snapshot"
    if ! bash "$snapshot/verify/check-builds.sh" > "$tmp/$name.log" 2>&1; then
      printf 'relocated-build: build failed in %s source\n' "$name" >&2
      grep -E 'check-builds:|error:|FAILED' "$tmp/$name.log" >&2 || true
      return 1
    fi
    printf 'Guest artifact SHA-256 (%s source):\n' "$name"
    (
      cd "$snapshot"
      shasum -a 256 methods/target/riscv-guest/launchpad_methods/launchpad_programs/riscv32im-risc0-zkvm-elf/release/*.bin packages/weighted-math/zkvm/target/riscv-guest/zkvm-methods/weighted-math-guest/riscv32im-risc0-zkvm-elf/release/*.bin
    ) | tee "$tmp/$name.sha256"
    printf 'relocated-build: %s source passed all checks\n' "$name"
    rm -rf "$snapshot"
  done
  cmp -s "$tmp/first.sha256" "$tmp/second.sha256" || {
    printf 'relocated-build: guest artifact hashes differ between source paths\n' >&2
    diff -u "$tmp/first.sha256" "$tmp/second.sha256" >&2 || true
    return 1
  }
  printf 'relocated-build: identical guest artifacts across source paths\n'
}

repeated_build() {
  local name snapshot source
  for name in first second; do
    snapshot="$(mktemp -d "$tmp/$name.XXXXXXXX")/source"
    [ "$name" != second ] || snapshot="$snapshot/nested"
    copy_source "$snapshot"
    if [ "$name" = first ]; then
      prepare_release_source "$snapshot"
    else
      mkdir "$release_parent/source"
      (cd "$snapshot" && tar -cf - .) | (cd "$release_parent/source" && tar -xf -)
    fi
    source="$release_parent/source"
    if ! bash "$source/verify/check-builds.sh" --release > "$tmp/$name.log" 2>&1; then
      printf 'repeated-build: release check failed for %s source\n' "$name" >&2
      grep -E 'check-builds:|error:|FAILED' "$tmp/$name.log" >&2 || true
      return 1
    fi
    "$source/target/release/launchpad" --help > "$tmp/$name.help"
    grep -q 'Usage:' "$tmp/$name.help"
    grep -q 'all outputs bit-identical host vs guest' "$tmp/$name.log"
    grep -q 'guest_release_arithmetic_rejects_overflow ... ok' "$tmp/$name.log"
    printf 'Guest artifact SHA-256 (%s source, fixed release path):\n' "$name"
    (
      cd "$source"
      shasum -a 256 methods/target/riscv-guest/launchpad_methods/launchpad_programs/riscv32im-risc0-zkvm-elf/release/*.bin packages/weighted-math/zkvm/target/riscv-guest/zkvm-methods/weighted-math-guest/riscv32im-risc0-zkvm-elf/release/*.bin
    ) | tee "$tmp/$name.sha256"
    [ "$(wc -l < "$tmp/$name.sha256" | tr -d ' ')" = 5 ]
    printf 'Guest image IDs (%s source):\n' "$name"
    find "$source/methods/target/release/build" "$source/packages/weighted-math/zkvm/target/release/build" -name methods.rs -exec grep -h '^pub const .*_ID:' {} + | sort -u | tee "$tmp/$name.ids"
    [ "$(wc -l < "$tmp/$name.ids" | tr -d ' ')" = 5 ]
    grep '^test result: ok' "$tmp/$name.log"
    printf 'repeated-build: %s source passed all checks and CLI help\n' "$name"
    rm -rf "$source"
  done
  cmp -s "$tmp/first.sha256" "$tmp/second.sha256" && cmp -s "$tmp/first.ids" "$tmp/second.ids" || {
    printf 'repeated-build: guest identities differ at the fixed release path\n' >&2
    diff -u "$tmp/first.sha256" "$tmp/second.sha256" >&2 || true
    diff -u "$tmp/first.ids" "$tmp/second.ids" >&2 || true
    return 1
  }
  printf 'repeated-build: identical guest artifacts and image IDs at /tmp/logos-launchpad-release/source\n'
}

case "${1:-all}" in
  missing-tool) missing_tool ;;
  wrong-tool) wrong_tool ;;
  wrong-scaffold) wrong_scaffold ;;
  unpinned-math) unpinned_math ;;
  wrong-executor) wrong_executor ;;
  skip-guests) skip_guests ;;
  compiler-override) compiler_override ;;
  release-path) release_path ;;
  release-busy) release_busy ;;
  release-failed-stage) release_failed_stage ;;
  nested-lock-drift) nested_lock_drift ;;
  math-lock-drift) nested_lock_drift math ;;
  failed-host-test) failed_host_test ;;
  failed-math-test) failed_math_test ;;
  failed-guest-test) failed_guest_test ;;
  changed-lockfile) changed_lockfile ;;
  repeated-build) repeated_build ;;
  relocated-build) relocated_build ;;
  complete-build) complete_build ;;
  all) missing_tool; wrong_tool; wrong_scaffold; unpinned_math; wrong_executor; skip_guests; compiler_override ;;
  *) printf 'Unknown test: %s\n' "$1" >&2; exit 2 ;;
esac
