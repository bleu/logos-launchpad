#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$root"
fail() { printf 'dev: %s\n' "$1" >&2; exit 1; }
setup_cache() {
  local mode="${1:-${LAUNCHPAD_DEV_CACHE:-auto}}" wrapper cache_root
  case "$mode" in off) return ;; auto|required) ;; *) fail 'LAUNCHPAD_DEV_CACHE must be auto, required or off' ;; esac
  wrapper="$(command -v sccache || true)"
  if [ -z "$wrapper" ]; then
    [ "$mode" != required ] || fail 'sccache is required; install it or set LAUNCHPAD_DEV_CACHE=off'
    printf 'dev: sccache is unavailable; shared caching is off\n' >&2
    return
  fi
  cache_root="${LAUNCHPAD_DEV_CACHE_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/logos-launchpad}"
  case "$cache_root" in /*) ;; *) fail 'LAUNCHPAD_DEV_CACHE_DIR must be absolute' ;; esac
  mkdir -p "$cache_root"
  export SCCACHE_DIR="$cache_root/objects"
  export SCCACHE_CACHE_SIZE=5G
  export SCCACHE_SERVER_UDS="$cache_root/server.sock"
  export SCCACHE_CONF=/dev/null
  export SCCACHE_MULTILEVEL_CHAIN=disk
  export RUSTC_WRAPPER="$wrapper"
}
if [ "${1:-}" = cache-stats ]; then
  [ "$#" = 1 ] || fail 'usage: bash verify/dev.sh cache-stats'
  setup_cache required
  exec "$RUSTC_WRAPPER" --show-stats
fi
[ "$#" -ge 2 ] || fail 'usage: bash verify/dev.sh <app|actor|math> <check|test|clippy|clean> [Cargo arguments]'
workspace="$1"
action="$2"
shift 2
toolchain="$(awk -F '"' '/^channel = / {print $2}' rust-toolchain.toml)"
case "$workspace" in
  app)
    manifest="$root/Cargo.toml"
    export CURVE_EMERGENCY_AUTHORITY="${CURVE_EMERGENCY_AUTHORITY:-$(< crates/curve-core/tests/fixtures/emergency-authority.hex)}"
    ;;
  actor)
    source verify/actor-tools.env
    toolchain="$ACTOR_RUST_VERSION"
    manifest="$root/packages/lez-compat/Cargo.toml"
    ;;
  math) manifest="$root/packages/weighted-math/Cargo.toml" ;;
  *) fail "unknown host workspace: $workspace" ;;
esac
case "$action" in check|test|clippy|clean) ;; *) fail "unknown host action: $action" ;; esac
for argument in "$@"; do
  [ "$argument" != -- ] || break
  case "$argument" in
    -r|--release|--profile|--profile=*|--target|--target=*|--target-dir|--target-dir=*|--manifest-path|--manifest-path=*|--config|--config=*|-Z*)
      fail "use the verification commands for $argument"
      ;;
  esac
done
if [ "$action" = clean ] && [ "$#" != 0 ]; then fail 'clean takes no Cargo arguments'; fi
unset CARGO_TARGET_DIR CARGO_BUILD_TARGET_DIR
export RUSTC_WRAPPER=''
export RUSTC_WORKSPACE_WRAPPER=''
export CARGO_PROFILE_DEV_DEBUG=line-tables-only
export CARGO_PROFILE_TEST_DEBUG=line-tables-only
export CARGO_PROFILE_DEV_INCREMENTAL=false
export CARGO_PROFILE_TEST_INCREMENTAL=false
export CARGO_INCREMENTAL=0
target="$root/.local/launchpad/dev/$workspace/target"
if [ "$action" != clean ]; then setup_cache; fi
printf 'dev: %s %s; compact host output in %s\n' "$workspace" "$action" "$target" >&2
exec cargo +"$toolchain" "$action" --manifest-path "$manifest" --locked --target-dir "$target" "$@"
