# Context

This is the shared language for the launchpad and its three sale types. The repository contains an imported PoC baseline and design decisions. The baseline does not implement the full production specification. [ADRs](docs/adr/README.md) state which decisions are accepted or still open. For plain-English explanations of technical shorthand, see the [glossary](docs/glossary.md); the terms here and ADR decisions remain canonical.

## Domain terms

| Term | Meaning |
| --- | --- |
| Namespace | Independent launchpad identity with an `AccountId` administrator, protocol fee rate and treasury. Sales stay bound to their creation namespace. |
| Namespace state | Logical per-namespace administration and configuration state used for creation validation and applicable runtime checks. It does not select a registry, shared physical schema or cross-factory account owner. Its readonly account-data format version identifies the supported namespace data form; it does not expose or duplicate role-authority bytes. |
| Role authority | Separate `spel-admin-authority` state for each namespace administrator, fixed-price sale creator or direct pool-owner role. Sharing the library does not share permissions. Role data remains hidden behind its authority API. Creator and pool-owner migration is not implemented for existing curve roles. |
| Authority NFT | Legacy PoC role mechanism, replaced in the selected design by role authority. Existing creator and direct pool-owner code still uses NFT holder checks pending migration. |
| Emergency authority | Separate namespace role that controls swap freeze. See [ADR 0028](docs/adr/0028-token-custody-and-freeze.md#namespace-emergency-control). |
| Treasury | Current namespace destination for collected protocol fees. |
| Fixed-price sale | Sale with a positive exact price in collateral and launch-token smallest units. It supports four-stage setup, public buys and permissionless fee collection on the actor interface. Its end time is optional; buys stop at that end or sellout. Close and payout handlers remain pending. |
| Bonding-curve sale | Sale that uses the curve pool for pricing and custody, with factory allocation and closing rules. |
| Weighted sale | Sale whose pricing uses token balances and weights that change on a set schedule. Weighted-sale execution, accounting, lifecycle and fees remain future work. |
| Curve | Neutral bounded constant-product AMM with virtual reserves. Curve factory policy turns it into a bonding-curve sale. |
| Pool | Mechanism state and token custody. A curve pool is an ordered token pair with reserves, owner and lifecycle. Direct curve pools are internal primitives and test surfaces. |
| Token0 / token1 | Ordered curve token roles. Bonding-curve sales use the launch token as token0 and collateral as token1. |
| Real reserve | Tokens available to back swap output. Accrued fees are separate. |
| Virtual reserve | Pricing-only curve state that moves with swaps. Its creation-time product is the immutable pricing invariant `k`. |
| Factory | Shared launch adapter for common new-token creation, allocation, namespace binding and setup checks when the rules match. Fixed-price, bonding-curve and weighted sale typed inputs remain separate, as do their pool pricing, custody, lifecycle and fee rules. |
| `D` | Purchasable bonding-curve sale allocation. The curve closes when all `D` tokens are purchased. |
| `R` | Owner-reserved bonding-curve allocation designated as the DEX-seed reserve. It remains untouched until close; the curve factory can transfer it to the creator after close when it is not used for graduation. |
| Weight checkpoint | Future weighted-sale observation record of a validity window and selected pricing pair. It must not claim an exact execution timestamp or select a later buy's price. See [ADR 0026](docs/adr/0026-lbp-clock-and-poke.md). |
| Recorded collateral | Collateral counted by sale accounting, separate from the actual vault balance. Fixed-price setup keeps it at zero; unsolicited transfers remain surplus. See [ADR 0033](docs/adr/0033-fixed-price-lbp-recorded-collateral.md). |
| Pricing collateral | Weighted-sale record containing the collateral seed plus full successful buyer inputs. The implementation keeps it separate from actual vault collateral and does not authorize an unbacked payout. |
| Weighted math | Pure arithmetic kernel for weighted-sale pricing. It has no namespace, account custody or sale policy. It is unaudited. |
| Accrued fees | Protocol fees denominated in collateral and tracked separately from reserves or creator proceeds. Curve fees are deducted from buy input or sell output. Fixed-price fees and weighted-sale fees are creator-borne claims on collateral proceeds and do not reduce buyer pricing input. |
| Fee collection | Curve and fixed-price fee collection are permissionless, full and idempotent transfers of accrued fees to the current treasury. The caller cannot choose or redirect the treasury. Fixed-price and future weighted-sale collection and live rates are accepted internally but have external status `To be reviewed by Logos team` because they depart from the RFP-016 baseline. |
| Creator | Sale authority holder responsible for creation and permitted lifecycle actions. A fixed-price sale has separate per-sale creator role authority initialized by the creator signer. |
| Participant | Person or client using the configured-sales app to trade. |
| Settlement | Post-close collection of remaining fees and permitted allocation/proceeds transfers. See [ADR 0020](docs/adr/0020-settlement-and-graduation.md) for manual and adapter routes. |
| Trade mode | Immutable curve setting that selects permitted trade directions. See [ADR 0021](docs/adr/0021-modes-and-open-limits.md#sale-modes). |
| Graduation hook | Public post-close call to an adapter saved at creation. |
| Adapter custody | Adapter-owned PDA whose canonical ATAs receive hook payouts. |
| Graduated | Final factory state after successful hook transfers and callback; not proof of DEX liquidity. |
| Private route | SDK-managed deshield, public trade and return of received tokens, unused input and unused gas to participant-controlled private accounts within one transaction. Actual fees are excluded from returns. Public pool state and trade effects remain visible. |
| Sale allowlist | Future fixed-price and weighted sale policy for a private list of approved participants with one common public collateral limit. The proof method remains open. |
| Purchase allowance | Future fixed-price and weighted sale rule for one successful buy per approved participant per sale across public and private routes, when an allowlist is enabled. |
| Admin app | Separately packaged Basecamp QML mini-app in `packages/launchpad/apps/admin/` for namespace administration only. |
| Configured-sales app | Separately packaged Basecamp QML mini-app in `packages/launchpad/apps/configured-sales/` for creator and participant flows within a configured namespace. |
| Shared core | Non-UI reusable SDK logic exposed through a core module in `packages/launchpad/core/`. |
| Module `.lidl` contract | Machine-readable module interface used by the generated module adapter. It is distinct from an on-chain program IDL. |
| On-chain program IDL | Machine-readable program interface used to generate client-facing transaction artifacts. |
| ATA | Associated token account derived from an owner and token definition. |
| PoC | Proof of concept. Useful source and test evidence, not a production-readiness claim. |

## Boundaries

Public sale timestamps use Unix seconds.

Nothing from this project is deployed to testnet or production. There is no deployed state to preserve or transfer. The application is [migrated to a fixed LEZ development revision](plans/lez-v04-migration.md) and requires a later official-release compatibility check. The [actor development base](packages/lez-compat/README.md) supplies temporary token, ATA and role-authority support under [ADR 0034](docs/adr/0034-lez-v04-migration.md). Its host tests, locked guest builds and node-free token check do not establish sale-flow compatibility or rollback. Root handlers use the actor interface. Local repository checks passed, including seeded curve trade and expiry tests and fixed-price buy, fee-collection and rollback tests; setup execution and package loading remain unverified. The old platform is not a second supported lane. Private helpers return explicit unsupported errors because compatibility token holdings are public only.

The curve pool owns neutral pricing and custody rules. The shared launch factory owns common setup only where behavior matches. Fixed-price, bonding-curve and weighted sale pools retain separate pricing, custody, lifecycle and fee rules; no generic lifecycle is selected. A bonding-curve sale closes when `D` is exhausted; its optional expiry mode can close at `D` exhaustion or its configured end timestamp. The contract does not validate a minimum duration for optional expiry. This is an accepted internal departure from RFP-015 with external status `To be reviewed by Logos team`; `D` exhaustion still closes immediately. Creators cannot close factory bonding-curve sales early. Settlement burns unsold `D` after expiry; `R` and the separate creator allocation keep their post-close rules. Fixed-price and weighted sale setup implement four stages: claim an exact mint and creator role, split inventory and allocation escrow, prepare the pool, vaults and immediate recipient ATA, then fund inventory, pay an `AfterSetup` allocation and complete setup. Weighted setup also transfers the full collateral seed from the creator and activates pricing collateral. Weighted schedules use normalized weights with a total of `10^18`; each side has at least 1%. An `AfterEnd` allocation remains in escrow for a future payout handler. Fixed-price buys use full collateral input for pricing, record net creator proceeds and track protocol fees separately. Collection pays the current namespace treasury. An optional end limits setup and buys; sellout stops buys even without an end. Close and payout handlers, weighted observations, private routes, real proofs and deployment readiness remain pending. Weighted public buys use buyer-selected validity windows under ADR 0026. Typed sale creation inputs are immutable; namespace settings such as the fee rate and treasury can remain live.

The shared launch factory binds each sale to its creation namespace. Namespace state is logical per-namespace state, not a selected registry or shared physical schema. The administrator manages namespace configuration only and does not receive creator rights. A creator acts through the configured-sales app and does not need administrator approval for every permitted action.

SDK code owns transaction construction, fresh quotes and the single-transaction private route, including minimal participant-local progress and reconciliation without replacement while an outcome is unknown. CLI and QML call that code. See the [private participation contract](docs/architecture/private-participation.md). QML reads indexed data through a backend module read API instead of calling the indexer directly. The admin app handles namespace administration only. The configured-sales app contains creator and participant flows. The apps ship separately from the non-UI shared core, and their placement does not change authority roles. The LP-0021 Wallet Provider SDK owns account selection, user approval, signing and submission once developed and integrated; its final API is unavailable, and support for the combined private transaction and recovery remains unverified. The shared core does not custody user keys. See [ADR 0029](docs/adr/0029-wallet-integration.md).

See the [bonding-curve overview](docs/product/curve-contract.md) for its decisions and evidence limits.

See the [namespace, custody, and fee contract](docs/product/namespace-fee-contract.md) for shared account checks and accounting rules. The [fixed-price and weighted sale overview](docs/product/lbp-contract.md) describes the fixed-price and weighted setup and buys, with links to the governing ADRs. See [planned ownership](docs/architecture/repository-layout.md) for package and artifact responsibilities. Keep the fixed-price and weighted sale fee RFP departures, curve expiry-validation departure, deferred allocation ceiling, private-route verification and open allowlist proof design visible in the [ADR index](docs/adr/README.md).
