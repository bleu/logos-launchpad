# Context

This is the shared language for both launchpads. The repository contains an imported PoC baseline and design decisions. The baseline does not implement the full production specification. [ADRs](docs/adr/README.md) state which decisions are accepted or still open. For plain-English explanations of technical shorthand, see the [glossary](docs/glossary.md); the terms here and ADR decisions remain canonical.

## Domain terms

| Term | Meaning |
| --- | --- |
| Namespace | Independent launchpad identity with an `AccountId` administrator, protocol fee rate and treasury. Sales stay bound to their creation namespace. |
| Namespace state | Logical per-namespace administration and configuration state used for creation validation and applicable runtime checks. It does not select a registry, shared physical schema or cross-factory account owner. |
| Authority NFT | Unique transferable NFT master that grants a role. Its token definition is the stable role identity; its current holder proves control. It can support a creator or pool-owner role, but does not administer a namespace. Target-platform compatibility still needs verification. |
| Treasury | Current namespace destination for collected protocol fees. |
| Curve | Neutral bounded constant-product AMM with virtual reserves. Curve factory policy turns it into a token sale. |
| Pool | Mechanism state and token custody. A curve pool is an ordered token pair with reserves, owner and lifecycle. Direct curve pools are internal primitives and test surfaces. |
| Token0 / token1 | Ordered curve token roles. Curve factory sales use the launch token as token0 and collateral as token1. |
| Real reserve | Tokens available to back swap output. Accrued fees are separate. |
| Virtual reserve | Pricing-only curve state that moves with swaps. Its creation-time product is the immutable pricing invariant `k`. |
| Factory | Mechanism-specific launch adapter. The curve factory owns curve supply, allocations, creator authority, closure and settlement policy. The LBP factory independently owns LBP creation validation, sale-to-namespace binding and LBP sale policy. |
| `D` | Purchasable curve sale allocation. The curve closes when all `D` tokens are purchased. |
| `R` | Owner-reserved curve allocation designated as the DEX-seed reserve. It remains untouched until close; the curve factory can transfer it to the creator after close when it is not used for graduation. |
| LBP | Liquidity bootstrapping pool. Weighted mode uses a fixed-total linear schedule derived from chain time; fixed-price mode uses a positive rational price. Both support exact-input and exact-output buys. It closes at its configured end timestamp. Its state machine remains to be built. |
| Weight checkpoint | Saved trusted timestamp and scheduled weight pair. A successful buy or permissionless poke updates it only when the timestamp changes. It helps observers; current time and reserves determine pricing. |
| Pricing collateral | LBP weighted-pricing record containing the collateral seed plus full successful buyer inputs. Fee collection does not reduce it. It is distinct from actual vault collateral and cannot authorize an unbacked payout. |
| Weighted math | Pure arithmetic kernel used by the LBP. It has no namespace, account custody or sale policy. It is unaudited. |
| Accrued fees | Protocol fees denominated in collateral and tracked separately from reserves or creator proceeds. Curve fees are deducted from buy input or sell output. LBP fees are creator-borne claims on collateral proceeds and do not reduce buyer pricing input. |
| Fee collection | Permissionless, full, idempotent transfer of accrued fees to the current treasury for both mechanisms. The caller cannot choose or redirect the treasury. Anytime LBP collection and live LBP rates are accepted internally but have external status `To be reviewed by Logos team` because they depart from the LBP RFP baseline. |
| Creator | Sale authority holder responsible for creation and permitted lifecycle actions. This role is separate from namespace administration. |
| Participant | Person or client using the configured-sales app to trade. |
| Settlement | Post-close collection of remaining fees and permitted allocation/proceeds transfers. Curve sales have creator-authorized manual settlement and an optional permissionless adapter hook. This does not claim a live DEX integration. |
| Trade mode | Required curve sale input that cannot change after creation. `BuyOnly` permits both buy forms. `BuyAndSell` also permits exact-input sells. There is no program default or exact-output sell. |
| Graduation hook | Separate public post-close call to an adapter saved at creation. Its program and configuration cannot change. It transfers full `R` and recorded real collateral, excluding fees, surplus and creator allocation. |
| Adapter custody | PDA derived under the saved adapter program from the saved sale identity. The adapter must initialize and claim it before factory ATA preparation. Its canonical ATAs receive hook amounts. The callback receives no factory signing rights. |
| Graduated | Final factory hook state. It records successful transfers and callback. It does not prove a DEX liquidity result. |
| Private route | SDK-managed deshield, public trade and return of received tokens, unused input and unused gas to participant-controlled private accounts within one transaction. Actual fees are excluded from returns. Public pool state and trade effects remain visible. |
| LBP allowlist | Private list of approved participants with one common public collateral limit. Product policy is accepted; the proof method remains open. |
| Purchase allowance | One successful LBP buy per approved participant per sale across public and private routes, when an allowlist is enabled. |
| Admin app | Separately packaged Basecamp QML mini-app for namespace administration only. |
| Configured-sales app | Separately packaged Basecamp QML mini-app for creator and participant flows within a configured namespace. |
| Shared core | Planned non-UI reusable SDK logic exposed through a core module. Its planned C ABI and generated module-adapter integration need compatibility verification and are separate from on-chain program handlers. |
| Module `.lidl` contract | Machine-readable module interface used by the planned generated module adapter. It is distinct from an on-chain program IDL. |
| On-chain program IDL | Machine-readable program interface used to generate client-facing transaction artifacts. |
| ATA | Associated token account derived from an owner and token definition. |
| PoC | Proof of concept. Useful source and test evidence, not a production-readiness claim. |

## Boundaries

The curve pool owns neutral pricing and custody rules. The curve factory owns curve launch policy. The independent LBP factory owns LBP creation validation and namespace binding; the LBP pool owns LBP trading, custody and state rules. A curve sale closes when `D` is exhausted; its optional expiry mode can close at `D` exhaustion or its configured end timestamp. The contract does not validate a minimum duration for optional expiry. This is an accepted internal departure from RFP-015 with external status `To be reviewed by Logos team`; `D` exhaustion still closes immediately. Creators cannot close factory curve sales early. Settlement burns unsold `D` after expiry; `R` and the separate creator allocation keep their post-close rules. LBP state and lifecycle remain separate from curve state and lifecycle, and an LBP closes at its schedule endpoint. Typed sale creation inputs are immutable; namespace settings such as the fee rate and treasury can remain live.

Each factory binds a sale to its creation namespace. Namespace state is logical per-namespace state, not a selected registry or shared physical schema. The administrator manages namespace configuration only. A creator acts through the configured-sales app and does not need administrator approval for every permitted action.

SDK code owns transaction construction, fresh quotes and the single-transaction private route, including minimal participant-local progress and reconciliation without replacement while an outcome is unknown. CLI and QML call that code. See the [private participation contract](docs/architecture/private-participation.md). QML reads indexed data through a backend module read API instead of calling the indexer directly. The admin app handles namespace administration only. The configured-sales app contains creator and participant flows. The apps ship separately from the non-UI shared core, and their placement does not change authority roles. The LP-0021 Wallet Provider SDK owns account selection, user approval, signing and submission once developed and integrated; its final API is unavailable, and support for the combined private transaction and recovery remains unverified. The shared core does not custody user keys. See [ADR 0029](docs/adr/0029-wallet-integration.md).

See the [curve contract](docs/product/curve-contract.md) for fixed creation-time `k`, checked arithmetic, fee-inclusive quotes, reserve safety, modes and settlement stages. [ADR 0020](docs/adr/0020-settlement-and-graduation.md) replaces the inactive-hook policy. Host, guest and generated-IDL checks passed; these do not prove production readiness. Closure does not execute the hook. A client submits a separate transaction. The first actual payout selects the settlement route. Failure rolls back that transaction. Earlier completed stages stay complete.

See the [namespace, custody, and fee contract](docs/product/namespace-fee-contract.md) for shared account checks and accounting rules. The [LBP overview](docs/product/lbp-contract.md) explains its modes, clock, buys and pricing collateral, with links to the governing ADRs. See [planned ownership](docs/architecture/repository-layout.md) for package and artifact responsibilities. Keep the LBP fee RFP departures, curve expiry-validation departure, deferred allocation ceiling, private-route verification and open allowlist proof design visible in the [ADR index](docs/adr/README.md).
