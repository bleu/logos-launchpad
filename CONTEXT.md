# Context

This page defines the shared domain language. The [glossary](docs/glossary.md) explains technical terms. [ADRs](docs/adr/README.md) define current decisions and pending approvals. The [workspace guide](docs/architecture/workspace-guide.md#execution-and-evidence-limits) states what local checks do and do not prove.

## Domain terms

| Term | Meaning |
| --- | --- |
| Namespace | Independent launchpad identity with an `AccountId` administrator, protocol fee rate and treasury. Sales stay bound to their creation namespace. |
| Namespace state | Logical administration and configuration state for one namespace. It does not select a registry, physical schema or cross-factory account owner. Its data-format version does not expose role-authority data. |
| Role authority | Separate authority state for a namespace administrator, sale creator or direct pool owner. Shared authority code does not share permissions. |
| Authority NFT | Legacy PoC role mechanism. Launchpad roles use role authority instead. |
| Emergency authority | Namespace role that controls swap freeze. |
| Treasury | Current namespace destination for collected protocol fees. |
| Fixed-price sale | Sale with a positive exact price in collateral and launch-token smallest units. |
| Bonding-curve sale | Sale that uses the curve pool for pricing and custody, with factory allocation and ending rules. |
| Weighted sale | Sale priced from token balances and weights that change on a set schedule. |
| Curve | Bounded constant-product AMM with virtual reserves. Factory policy turns it into a bonding-curve sale. |
| Pool | Mechanism state and token custody. A curve pool holds one ordered token pair. Direct curve pools are internal primitives and test surfaces. |
| Token0 / token1 | Ordered curve token roles. Bonding-curve sales use the launch token as token0 and collateral as token1. |
| Real reserve | Tokens available to back swap output. Accrued fees are separate. |
| Virtual reserve | Pricing-only curve state. The creation-time product is `initial_k`; current `k` can grow through retained sell collateral. |
| Factory | Shared launch adapter for matching creation, allocation, namespace-binding and setup rules. It shares policy decisions without combining pricing state or custody. |
| `D` | Purchasable bonding-curve sale allocation. |
| Graduation policy | Immutable adapter shares of unsold launch inventory and recorded collateral. The current creator receives the remainder. See [ADR 0020](docs/adr/0020-settlement-and-graduation.md). |
| Weight checkpoint | Observation of a weighted sale's pricing second and token weight. It does not control pricing or settlement. |
| Recorded collateral | Collateral counted by sale accounting, separate from the vault's actual balance. Unsolicited transfers are surplus. |
| Pricing collateral | Weighted-sale collateral used for pricing. It includes the seed and net trade changes, but excludes accrued namespace fees and surplus. |
| Weighted math | Independent arithmetic for weighted-sale pricing. It has no custody, namespace or sale policy. It is unaudited. |
| Accrued fees | Collateral-denominated protocol fees owed to the treasury, separate from reserves and creator proceeds. |
| Retained sell fee | Sell collateral kept in reserves or recorded proceeds. It is separate from collectible namespace fees. |
| Fee collection | Permissionless transfer of all accrued fees to the current treasury. Repeating a completed collection cannot pay twice. |
| Creator | Sale authority holder responsible for creation and permitted lifecycle actions. Each sale has a separate creator role. |
| Participant | Person or client that uses the configured-sales app to trade. |
| Settlement | Post-close collection of remaining fees and permitted transfers of allocations or proceeds. |
| Ending policy | Immutable `Manual`, `Scheduled`, `Depletion` or `Either` rule. Supported choices depend on the sale type. |
| Pause policy | Immutable choice to disable pause or allow the creator or direct pool owner to stop and resume trades. |
| Trade mode | Immutable `BuyOnly` or `BuyAndSell` choice for a factory sale. Both allow exact-input and exact-output buys; the latter also allows exact-input sells. |
| Graduation hook | Public post-close call to an adapter saved at creation. |
| Adapter custody | Adapter-owned PDA whose canonical ATAs receive hook payouts. |
| Sale event | Versioned public program output for a committed change. It identifies the namespace, pricing engine and pool or launch. It does not prove indexer delivery or stored history. |
| Graduated | Final factory state after successful hook transfers and callback. It does not prove DEX liquidity. |
| Private route | Planned SDK-managed deshield, public trade and return of received tokens, unused input and unused gas to participant-controlled private accounts in one transaction. Actual fees are excluded. Public trade effects remain visible. |
| Pricing engine | Bonding-curve, fixed-price or weighted calculation of trade amounts. Shared policies do not combine these calculations. |
| Sale allowlist | Optional immutable buy policy. The actual trader must prove membership and control of an approved public trading account. Wallet address rotation is a privacy preference; approval permits repeat buys without a cap. |
| Admin app | Separate Basecamp QML mini-app for namespace administration in `packages/launchpad/apps/admin/`. |
| Configured-sales app | Separate Basecamp QML mini-app for creator and participant flows in a configured namespace, in `packages/launchpad/apps/configured-sales/`. |
| Shared core | Non-UI reusable SDK logic in `packages/launchpad/core/`. |
| Module `.lidl` contract | Machine-readable module interface used by the generated module adapter. It is separate from an on-chain program IDL. |
| On-chain program IDL | Machine-readable program interface used to generate client transaction artifacts. |
| ATA | Associated token account derived from an owner and token definition. |
| PoC | Proof of concept. Source and tests can provide evidence, but do not establish production readiness. |

## Boundaries

Nothing is deployed to testnet or production. There are no users or important stored data to preserve. Breaking changes to code, stored state and client interfaces are allowed. No migration or backward compatibility is required. The project targets one fixed LEZ development revision, not a second legacy platform. See [ADR 0034](docs/adr/0034-lez-v04-migration.md).

Each pricing engine keeps its own state, custody and lifecycle implementation. Share helpers only where behavior matches. Namespace administration does not grant creator rights. App placement does not change authority. See [shared factory boundaries](docs/adr/0031-shared-launch-factory.md) and [roles](docs/adr/0006-namespaces-and-authority.md).

Public sale timestamps use Unix seconds. Lifecycle deadlines use LEZ inclusion-block validity windows, with inclusive starts and exclusive ends. Saved clock time is not the current block timestamp. See [ending rules](docs/adr/0025-sale-closing.md) and [weighted pricing time](docs/adr/0026-lbp-clock-and-poke.md).

The SDK owns quotes and transaction construction. CLI and QML clients must not copy protocol logic. QML reads indexed data through a backend module API. The Wallet Provider SDK will own account selection, approval, signing and submission. The shared core does not hold user keys. See the [system view](docs/architecture/system.md) and [private-route contract](docs/architecture/private-participation.md).

An accepted internal design is not proof of implementation or external approval. Keep RFP departures and open decisions visible in the [ADR index](docs/adr/README.md). Keep execution coverage, unsupported private routes, clock restrictions and other readiness limits in the [workspace guide](docs/architecture/workspace-guide.md#execution-and-evidence-limits).

The [distribution adapter](docs/adr/0036-graduation-adapter-delivery.md) uses immutable percentage lists for full adapter custody balances. Later permissionless distributions require successful graduation. It supports plain transfers only.
