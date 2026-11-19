# Bonding-curve sale overview

Bonding-curve sales support public trades, controls, fee collection and shared settlement on the selected actor revision. Manual settlement requires no configured adapter; otherwise the saved graduation hook is mandatory. Creator and direct pool-owner checks use separate role authority. See the [workspace guide](../architecture/workspace-guide.md#execution-and-evidence-limits) for local execution evidence and its limits. Use the terms in [CONTEXT.md](../../CONTEXT.md).

## Rules

| Topic | Decision |
| --- | --- |
| Creation | [Shared setup](../adr/0031-shared-launch-factory.md), [immutable inputs](../adr/0007-typed-sale-inputs.md) and [allocations](../adr/0008-curve-allocation-and-close.md) |
| Trading | [Pricing and rounding](../adr/0009-curve-arithmetic.md), [trade modes](../adr/0021-modes-and-open-limits.md#sale-modes) and [slippage limits](../adr/0017-quotes-and-slippage.md) |
| Custody and fees | [Custody and freeze](../adr/0028-token-custody-and-freeze.md) and [fee accounting](../adr/0015-fee-accounting.md) |
| Authority | [Namespace, creator and pool-owner roles](../adr/0006-namespaces-and-authority.md) |
| Lifecycle | [Closing](../adr/0025-sale-closing.md) and [manual settlement or adapter hook](../adr/0020-settlement-and-graduation.md) |

Pricing uses virtual reserves; payouts require real reserves. The separate [graduation reserve `R`](../adr/0020-settlement-and-graduation.md#graduation-reserve) stays outside trading. Surplus deposits do not increase payout rights. See the [pool code](../../crates/pool/src/lib.rs) and [solvency tests](../../crates/pool/tests/proptest_invariants.rs).

## Integration

Adapter authors should use the [factory types](../../crates/factory-core/src/lib.rs) and [typed callback contract](../../crates/factory-core/src/hook.rs). [ADR 0020](../adr/0020-settlement-and-graduation.md) defines custody and permission rules. A successful callback does not prove DEX liquidity or support for arbitrary adapters.

The [generated actor schema](../../idl/launchpad.json) defines the public wire interface. Rebuild guests and clients together after an encoding change and use fresh local state. No deployed-state migration is required. See the [workspace guide](../architecture/workspace-guide.md) for generation and verification commands and the [creation sequence](../architecture/sequence-diagrams/create-curve-pool.md) for the flow.

## Acceptance evidence

The workspace guide is the current evidence record. Host tests alone do not prove child-call rollback; development receipts are not secure production proofs. Private routes remain unsupported, and live DEX integration remains unverified. See [verification boundaries](../adr/0022-verification-boundaries.md).

See [ADR 0036](../adr/0036-graduation-adapter-delivery.md) for the plain-transfer distribution adapter and later deposits.

Configured expiry requires at least 60 seconds remaining through factory setup and activation. Inclusion bounds enforce this rule. This operational minimum is not a verified private-path latency guarantee. See [ending policy](../adr/0025-sale-closing.md).
