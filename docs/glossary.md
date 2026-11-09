# Plain-English glossary

This page explains technical shorthand used in this repository. [CONTEXT.md](../CONTEXT.md) is the canonical domain language, and the [ADRs](adr/README.md) decide behavior and status. This glossary does not add product rules, implementation claims or policy.

## Product and sale terms

| Term | Plain-English meaning |
| --- | --- |
| Launchpad | A product for creating and trading token sales. |
| Mechanism | One set of pricing, state and lifecycle rules. The three sale types are fixed-price sale, bonding-curve sale and weighted sale. |
| Bonding-curve sale | A sale whose price follows a mathematical curve as tokens are bought or sold. |
| AMM | Automated market maker: a program that prices trades from a formula and token balances. |
| Pool | Mechanism state and token custody. A curve pool is state and custody for one ordered token pair. A direct curve pool is an internal primitive, not a separate sale product. |
| Factory | Shared launch adapter that can create new tokens, set allocations, bind a sale to its namespace and validate setup where rules match. Fixed-price, bonding-curve and weighted sale pools keep separate pricing, custody, lifecycle and fee rules. |
| Token0 / token1 | The ordered token roles in a curve pool. A curve factory sale uses the launch token as token0 and collateral as token1. |
| Collateral | The token used to buy the sale token and pay protocol fees. |
| Real reserve / virtual reserve | Real reserves are tokens that can fund swap output. Virtual reserves are pricing-only values. |
| Invariant / `k` | A formula value that pricing must preserve under its stated rules. The curve's creation-time virtual-reserve product is `k`. |
| `D` / `R` | `D` is the purchasable curve allocation. `R` is the curve allocation reserved for possible DEX seeding after close. |
| Allocation / supply | Tokens assigned to a purpose in a sale, or the total tokens available for that purpose. |
| Fixed-price sale | A sale with a strictly positive exact price in collateral and launch-token smallest units; token decimals are for display. Locked local verification of its four-stage setup passed using development receipts; trading, real proofs and deployment readiness remain pending. See [ADR 0032](adr/0032-fixed-price-lbp-setup.md). |
| Weighted sale | A sale whose pricing uses token balances and weights that change on a set schedule. Setup, buys, controls, settlement and observer checkpoints are implemented with a temporary pricing clock. |
| Weight / weight schedule | A pricing input that changes over time for a weighted sale. The schedule defines its start, end and progress. |
| Quote | A price calculation from current state. It does not reserve that price for later execution. |
| Exact-input / exact-output | A trade where the input is fixed and output is calculated, or where the desired output is fixed and required input is calculated. |
| Slippage limit | A participant-selected execution bound, such as minimum output or maximum input, that rejects an unfavourable trade. |
| Protocol fee / accrued fee | The protocol charge on a trade, and the amount recorded but not yet transferred to the treasury. |
| Fee-first buy | A buy that deducts the collateral fee before pricing tokens. Fixed-price exact-output buys gross up the required net collateral; creator proceeds are net. See [ADR 0035](adr/0035-fixed-price-swap-fees.md). |
| Creator-borne fee | A future weighted-sale fee claim against creator proceeds. It does not reduce the buyer input used for pricing. |
| Live fee rate / current treasury | A namespace setting read at execution or collection time, rather than a value frozen when the sale was created. |
| Treasury | The current namespace destination for collected protocol fees. |
| Lifecycle / close / settlement | A sale's allowed state changes; ending trading; and the separate post-close transfer of allowed fees, allocations or proceeds. |
| Graduation / DEX | Moving liquidity to a decentralized exchange. |
| Graduation hook | A call to another program after a sale closes. Its rules are in [ADR 0020](adr/0020-settlement-and-graduation.md). |
| Expiry / end timestamp | A configured time that can end a bonding-curve sale or the scheduled end of a fixed-price or weighted sale. |
| `poke` | A permissionless weighted-sale call that reads the pricing clock and records the current token weight as an observer checkpoint without changing the price schedule. |
| Pause / freeze | Controls that stop specified actions. Their platform integration and policy remain separate from namespace administration. |
| Allowlist / proof / nullifier | A list of permitted participants, evidence that a participant is permitted, and a value used to prevent proof reuse. Fixed-price and weighted sale allowlist product rules are accepted; the proof method remains open. |
| Purchase allowance | Permission for one successful fixed-price or weighted sale buy per approved participant per sale, shared across public and private routes when an allowlist is enabled. |
| Dust / oversized trade / input envelope | Very small trades, trades that are too large, and the tested range of values that an implementation accepts safely. |

## Roles, accounts and custody

| Term | Plain-English meaning |
| --- | --- |
| Namespace | An independent launchpad identity with its own administrator, fee rate and treasury. Sales remain bound to their creation namespace. |
| Namespace state | Logical per-namespace administration and configuration state used for creation validation and applicable runtime checks. This term does not select a registry, shared physical schema or cross-factory account owner. |
| Namespace administrator | The holder of namespace-configuration rights. This role does not receive creator rights or approve every permitted creator action. |
| Creator | The sale authority holder with permitted creation and lifecycle actions. Curve and fixed-price sales create a separate per-sale role for their creator. |
| Participant | A person or client that uses a configured sale to inspect, quote or trade. |
| Pool owner | A separate authority role for an applicable pool. |
| Authority | Permission to perform a defined action. It is checked per action and role. |
| App placement | Which mini-app presents a flow. The admin app presents namespace administration only, while the configured-sales app presents creator and participant flows. Placement does not change authority. |
| `AccountId` | The account identifier used for namespace administration in the planned design. |
| Role authority | A role's own authority state. Roles share code, not permissions. Current holders can transfer curve creator and direct pool-owner roles with successor consent. Those two roles cannot renounce. |
| Authority NFT / NFT master | The legacy PoC role mechanism: a unique token whose holder could act. Launchpad role checks now use separate role authority. |
| Token definition / mint authority | The token's defining record, and authority to create or manage that token definition when platform rules allow it. |
| Custody / vault | Control of tokens, and an account that holds tokens for a program or sale. |
| ATA | Associated token account: an account derived for an owner and token definition. |
| PDA | Program Derived Address: an address derived by a program rather than controlled by a normal private key. |
| Account schema / account derivation | The required accounts for an operation, and the rules used to calculate or validate their addresses. |
| Permissionless / idempotent / atomic | Callable by any user; safe to repeat without changing a completed result; and completed fully or not at all within one transaction. |

## Privacy terms

| Term | Plain-English meaning |
| --- | --- |
| Private route | The planned SDK-managed transaction that deshields input, trades publicly, and returns received tokens, unused input and unused gas to participant-controlled private accounts. Actual fees are excluded. |
| Shielded / deshield / re-shield | Protected private balance state; moving assets to a public account; and moving received or unused assets back to protected state. |
| Fresh single-use public account | A new public account used for the trade within one private-route transaction. |
| Public trade effects | Pool state and trade results visible on the public chain. The route does not make them private. |
| ZK / zero knowledge / zkVM | A proof system that can show a computation was valid without exposing all inputs; a zkVM is a virtual machine that can produce such proofs. RISC Zero is a zkVM project name, but this repository does not select or implement it. |
| Proving readiness | Evidence that an implementation can meet the proof-related requirements that apply to it. Existing documents make no such production claim. |

## Platform, interfaces and packages

| Term | Plain-English meaning |
| --- | --- |
| LEZ | Logos Execution Zone, the target platform named by this repository. |
| SPEL / `spel-admin-authority` | The name used for a cited platform component family and its administration component. This glossary does not expand the acronym. |
| LP-0012, LP-0013, LP-0014, LP-0015 | Identifiers for platform interfaces or conventions cited by the ADRs. `LP` is not expanded here because the repository does not define it. |
| On-chain program / handler | Code that runs on the chain, and the code path that processes a specific program operation. |
| Guest entry point / guest build | A thin entry point and build for the constrained program target. It dispatches to handlers. |
| Cross-program call / platform call | A call from one on-chain program to another program or platform facility. |
| SDK | Software development kit: reusable client code for quotes, transaction construction and supported private flows. |
| Wallet integration boundary | The LP-0021 Wallet Provider SDK boundary. Its specification requires account selection, user approval, signing and submission; integration awaits SDK development and its final API is unavailable. Support for the combined private transaction remains unverified. See [ADR 0029](adr/0029-wallet-integration.md). |
| API | Application programming interface: callable functions and data types offered to another part of a program. |
| ABI / C ABI | Application binary interface: the calling and data-layout rules at a compiled-code boundary. A C ABI is a language-neutral form of that contract. In the planned Rust module build path, it connects the generated module adapter to the shared core. Compatibility is unverified. |
| Generated module adapter | Planned generated connection code from the QML and Basecamp module API to the shared core. It uses the planned C ABI. It is not a runtime service and is different from the Basecamp QML runtime bridge. |
| QML runtime bridge | A Basecamp platform runtime facility for QML module calls. It is different from the planned generated module adapter and does not make the C ABI integration verified. |
| Shared core / non-UI core | Planned reusable client logic without a user interface. It is separate from on-chain handlers. |
| Module `.lidl` contract | A machine-readable module interface used to generate the planned module adapter. It is distinct from an on-chain program IDL. |
| On-chain program IDL / source IDL / generated IDL | A machine-readable on-chain program interface and its source or generated build artifact. It describes program instructions and client encoding; it is not a runtime service. |
| Backend module read API | The planned module boundary through which QML apps request indexed listings, history and monitoring. It uses the existing indexer and does not authorize actions. |
| Indexed state | Data read from the existing indexer. It can lag behind current chain state, which program handlers enforce when a transaction executes. |
| Interface contract / compatibility / drift | An agreed boundary between components; whether two sides work together; and an unintended difference between generated output and its source. |
| QML | A declarative user-interface language for the planned Basecamp mini-apps. |
| Basecamp mini-app | A separately packaged QML user interface for the Basecamp environment. |
| CLI | Command-line interface: a text-command client that calls shared SDK logic. |
| `.lgx` | The package extension used for planned Basecamp modules and mini-apps. |
| Rust / crate / package | The implementation language; a Rust compilation and distribution unit; and a distributable unit of code or artifacts. |
| Cargo / Cargo workspace | Rust's build and dependency tool; a group of related Rust packages built under one configuration. Nested workspaces can need separate checks. |
| Dependency / local path dependency | Reusable code a package needs; a dependency loaded from another path in this repository rather than a registry release. |
| Artifact / binary / module | A build output; an executable build output; and a reusable packaged unit. |
| Lockfile / toolchain pin | A recorded dependency version set; and a fixed compiler or tool version used for repeatable builds. |
| Host / host-guest parity | The ordinary build or test environment; and a check that host and guest execution give the same deterministic result. |
| Tail-call settlement | A cited platform pattern where settlement continues through a final call. It requires platform verification before use. |

## Math and test terms

| Term | Plain-English meaning |
| --- | --- |
| Weighted math / kernel | The standalone arithmetic for weighted-sale pricing. It has no custody or namespace policy and remains unaudited. |
| Constant-product arithmetic | Pricing math based on preserving a product of reserves under its stated rounding and fee rules. |
| Fixed-point / scale / precision | Integer representation of fractional values; the multiplier that supplies fractional digits; and how much fractional detail that representation keeps. |
| `u128` / `i128` | Unsigned and signed 128-bit integer types. |
| Widening / `128 x 128 -> 256` | Using a larger temporary integer type for a multiplication so its result can fit safely. |
| Overflow / wrap / halt | A result too large for its type; incorrect rollover after overflow; and stopping rather than returning a wrapped result. |
| Rounding / floor / ceiling / conservative rounding | Choosing a whole-number result below or above a fractional result; and choosing a direction that protects the pool under the stated rule. |
| Quantization / ULP / error budget | Error from representing a continuous value in fixed steps; one smallest representable step; and the allowed total error. |
| Pad / cancellation | A small intentional safety adjustment; and loss of precision when near-equal values are subtracted. |
| Logarithm / exponential / Taylor series / Horner evaluation | Mathematical methods used by the weighted-math PoC to calculate powers with integers. |
| mpmath / oracle fixture | A high-precision Python math library used as an accuracy reference; and a saved expected-value test input and output. |
| Balancer | An external AMM implementation used as a secondary comparison reference, not a bit-identical target. |
| Invariant referee | An independent exact-arithmetic checker for the weighted invariant. |
| Property test | A test that checks a general rule across many generated inputs. |
| Fuzz test / fuzz target | A test that sends many unexpected or random inputs to code; the code path configured for that testing. |
| Harness / fixture / parity | Test support code; saved test data; and agreement between two implementations or build targets. |
| Solvency | The condition that a pool has enough real reserves for allowed withdrawals. |

## Documentation, review and delivery terms

| Term | Plain-English meaning |
| --- | --- |
| ADR | Architecture Decision Record: a document that states context, decision, consequences and status. |
| RFP | Request for Proposals: an external request that the product proposals answer. |
| RFP departure | An accepted internal design that differs from an RFP. It can still require Logos-team review. |
| PoC | Proof of concept: useful source and test evidence, but not a production-readiness claim. |
| Source snapshot / provenance | A reviewed source revision; and the record of where copied source came from and how it changed. |
| Fork / direct source reuse | A repository copy with shared history; and copying reviewed source files while retaining required notices. The repository chooses direct reuse. |
| Pull request / PR | A proposed reviewed change to the repository. |
| CI | Continuous integration: automated checks run for repository changes. |
| E2E | End-to-end: a check that exercises a complete user or system flow. |
| Smoke check / integration check | A small check that a build or package can start or be called; and a check that components work together. |
| Build, install and package evidence | Proof that planned artifacts can be produced, installed and used. It does not by itself prove production readiness. |
| Status labels | Terms such as `Accepted`, `To be reviewed by Logos team`, `unaudited`, and `unverified` that state decision and evidence limits. |
