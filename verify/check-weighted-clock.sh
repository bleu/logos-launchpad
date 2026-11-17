#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source verify/actor-tools.env
export CURVE_EMERGENCY_AUTHORITY="${CURVE_EMERGENCY_AUTHORITY:-$(cat crates/curve-core/tests/fixtures/emergency-authority.hex)}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
printf 'Checking weighted buys with the pinned upstream clock.\n'
export RISC0_BUILD_LOCKED=1
cargo +"$ACTOR_RUST_VERSION" test --locked -p lbp-core
cargo +"$ACTOR_RUST_VERSION" test --manifest-path methods/Cargo.toml --locked --features actor-execution-tests --test actor_sale
