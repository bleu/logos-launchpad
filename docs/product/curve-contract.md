# Bonding-curve sale overview

Trade modes and the optional adapter hook are implemented. This page links the governing decisions and checks; it does not claim production readiness. Use the terms in [CONTEXT.md](../../CONTEXT.md).

## Rules

| Topic | Decision |
| --- | --- |
| Creation | [Immutable inputs](../adr/0007-typed-sale-inputs.md) and [allocations](../adr/0008-curve-allocation-and-close.md) |
| Trading | [Pricing and rounding](../adr/0009-curve-arithmetic.md), [trade modes](../adr/0021-modes-and-open-limits.md#sale-modes) and [slippage limits](../adr/0017-quotes-and-slippage.md) |
| Custody | [Namespace, custody, and fee contract](namespace-fee-contract.md) |
| Lifecycle | [Closing](../adr/0025-sale-closing.md) and [manual settlement or adapter hook](../adr/0020-settlement-and-graduation.md) |

Pricing uses virtual reserves; payouts require real reserves. Each trade changes custody by the same amount as its reserve and fee liabilities. Collection reduces custody and fees together, or clears fees already held by the treasury. Surplus deposits do not increase payout rights. Checked arithmetic and transaction rollback preserve these rules. See the [pool code](../../crates/pool/src/lib.rs) and [solvency tests](../../crates/pool/tests/proptest_invariants.rs).

Adapter authors should use the [factory types and handler](../../crates/factory-core/src/lib.rs), [account checks](../../crates/factory-core/src/hook.rs) and [test adapter](../../methods/guest/src/bin/graduation_adapter.rs) for the callback format and custody setup. The [generated IDL](../../idl/factory.json) defines the public wire interface.

## Release compatibility

State and instruction encodings have changed. Rebuild guests and clients together and use fresh local state; no deployed-state migration is provided. The pinned `lgs` parser cannot encode an empty hook configuration. Use the SDK or project CLI for that case; see the [wire tests](../../crates/launchpad-client/tests/idl_wire.rs).

## Acceptance evidence

Host tests, strict Clippy, formatting, dependency pins, guest builds and generated-IDL checks passed for this implementation. Coverage lives with the tests:

- [Curve handlers](../../crates/curve-core/src/tests.rs): trade restrictions, fees, slippage and treasury aliases.
- [Factory handlers](../../crates/factory-core/src/hook_tests.rs): saved settings, custody, authority and settlement guards.
- [SDK](../../crates/launchpad-client/src/tests.rs) and [CLI](../../cli/src/main.rs): quotes, resume and transaction construction.
- [Guest settlement](../../crates/launchpad-client/tests/settlement.rs): exact payouts, failed-callback rollback, retry, manual recovery and zero amounts.
- [Wire tests](../../crates/launchpad-client/tests/idl_wire.rs): generated-CLI encoding and parser limits.

Guest tests use `RISC0_DEV_MODE=1`, not secure production proofs. Handler tests alone do not prove child-call rollback. Failed-setup recovery and adapter-initiated reentry have no focused guest test. Live DEX integration, automation, payer policy, hook UI and live wallet commands remain unverified or unbuilt.

The pinned runtime allows [10 chained calls](https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lee/state_machine/core/src/program.rs) and [`32 * 1024 * 1024` cycles per public execution](https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lee/state_machine/src/program.rs). A nonzero two-token hook payout uses five calls before adapter descendants. General adapter size and execution limits remain unverified.

See [verification boundaries](../adr/0022-verification-boundaries.md) and the [workspace guide](../architecture/workspace-guide.md) for check commands and required tools.
