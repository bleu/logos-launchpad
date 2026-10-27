# Bonding-curve sale overview

Trade modes and the optional adapter hook are part of the implemented baseline. They are [migrated to the selected actor revision](../../plans/lez-v04-migration.md). Creator and direct pool-owner checks use separate role authority under [ADR 0006](../adr/0006-namespaces-and-authority.md). See the [workspace guide](../architecture/workspace-guide.md#execution-and-evidence-limits) for local execution evidence and its limits. Use the terms in [CONTEXT.md](../../CONTEXT.md).

## Rules

| Topic | Decision |
| --- | --- |
| Creation | [Immutable inputs](../adr/0007-typed-sale-inputs.md) and [allocations](../adr/0008-curve-allocation-and-close.md) |
| Trading | [Pricing and rounding](../adr/0009-curve-arithmetic.md), [trade modes](../adr/0021-modes-and-open-limits.md#sale-modes) and [slippage limits](../adr/0017-quotes-and-slippage.md) |
| Custody | [Namespace, custody, and fee contract](namespace-fee-contract.md) |
| Lifecycle | [Closing](../adr/0025-sale-closing.md) and [manual settlement or adapter hook](../adr/0020-settlement-and-graduation.md) |

Pricing uses virtual reserves; payouts require real reserves. Each trade changes custody by the same amount as its reserve and fee liabilities. Collection reduces custody and fees together, or clears fees already held by the treasury. Surplus deposits do not increase payout rights. Checked arithmetic and transaction rollback preserve these rules. See the [pool code](../../crates/pool/src/lib.rs) and [solvency tests](../../crates/pool/tests/proptest_invariants.rs).

Adapter authors should use the [factory types](../../crates/factory-core/src/lib.rs) and [typed callback contract](../../crates/factory-core/src/hook.rs). See [ADR 0020](../adr/0020-settlement-and-graduation.md) for custody and permission rules. The [generated actor schema](../../idl/launchpad.json) defines the public wire interface.

## Release compatibility

State and message encodings have changed. Rebuild guests and clients together and use fresh local state; no deployed-state migration is provided. The actor schema replaces the old SPEL instruction IDLs. See the [workspace guide](../architecture/workspace-guide.md) for current generation and verification commands.

## Acceptance evidence

Local host tests, guest builds and execution checks passed on the selected actor revision. Seeded curve tests cover buys and deadline checks. Signed role-authority guest tests cover fresh setup, role transfer, funding, manual settlement and unchanged state on rejection. See the [workspace guide](../architecture/workspace-guide.md#execution-and-evidence-limits) for exact coverage and remaining gaps. Relevant sources include:

- [Curve handlers](../../crates/curve-core/src/tests.rs): trade restrictions, fees, slippage and treasury aliases.
- [Role-authority guest tests](../../crates/launchpad-client/tests/role_authority_guest.rs): signed creation, transfer, direct-owner actions and curve factory manual settlement.
- [Factory graduation host tests](../../crates/factory-core/tests/graduation.rs): typed callback and custody checks; not runtime graduation evidence.
- [SDK](../../crates/launchpad-client/src/tests.rs) and [CLI](../../cli/src/main.rs): quotes, resume and transaction construction.
- [Guest settlement](https://github.com/bleu/logos-launchpad/blob/5b6927cbd3b5a12941f3956bcab8458446ed52db/crates/launchpad-client/tests/settlement.rs): historical payout, rollback, retry and recovery cases; not current actor execution evidence.
- [Wire tests](../../crates/launchpad-client/tests/idl_wire.rs): actor schema and message encoding checks.

Guest tests use `RISC0_DEV_MODE=1`, not secure production proofs. Handler tests alone do not prove child-call rollback. Failed-setup recovery and adapter-initiated reentry have no focused guest test. Live DEX integration, automation, payer policy, hook UI and live wallet commands remain unverified or unbuilt.

Old runtime call and cycle counts do not define the selected actor revision's limits. General adapter size, execution limits and sale callback rollback remain unverified. Token-fixture rollback is not sale execution evidence.

See [verification boundaries](../adr/0022-verification-boundaries.md) and the [workspace guide](../architecture/workspace-guide.md) for check commands and required tools.
