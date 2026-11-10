# Allowlist program integration

Status: Public program slice complete with local development evidence, 2026-11-10. Sections 1-5 pass their checks. Wallet work in section 6 and private execution in section 7 remain incomplete.

## Public execution evidence

After merging current main, the locked signed actor suite passes all 32 tests. It covers fresh factory setup for all three engines, both buy forms, actual-trader control, missing and copied membership, changed signed messages, admission-only Key/PDA attempts, native replay, repeat buys and full state rollback on late output failure. Authority relies on the current narrow forwarding rule: only authorized `Execute` forwards buys to another pool. Preserve this rule when adding messages.

Complete allowlisted buys passed the unchanged 10,000,000-cycle budget:

| Engine | Exact input | Exact output | Buy after fresh setup |
| --- | ---: | ---: | ---: |
| Bonding curve | 658,565 | 658,680 | 665,182 |
| Fixed price | 758,814 | 758,859 | 758,828 |
| Weighted, fake clock | 838,557 | 840,447 | 870,899-874,364 |

Verification passed offline with locked dependencies: `verify/check-dev.sh`, IDL generation and drift checks, 104 client tests, CLI compilation, `verify/check-weighted-clock.sh`, the signed actor suite, release overflow checks, scoped strict Rust lint and formatting. The 31 ELF-dependent client guest tests remain ignored; signed execution evidence comes from the separate actor suite. Local document links and whitespace checks pass. No native private proof or deployment was run.

These are local signed public executions, not native privacy proofs or deployment evidence. Weighted setup covers both allocation-release choices; normal weighted builds still lack a supported clock provider. The locked host regression script passes with unchanged lockfiles. Public rollback does not establish the separate private-return contract.

## Starting point

[ADR 0021](../docs/adr/0021-modes-and-open-limits.md) sets the product rules and the public Merkle-path candidate. The [design guide](../docs/architecture/allowlist-proof.md) explains the account model. Participants enroll public trading-address batches before sale creation; the creator locks the final context/root in the initial terms. No enrollment changes are allowed during staged setup. Membership permits repeat buys without a cap, counter or allowance. Private accounts remain funding and return endpoints.

The earlier [isolated experiment](../docs/architecture/allowlist-proof.md#earlier-measurements) passed the unchanged 10,000,000-cycle charged public-action gate at three and 100,000 addresses. The larger fixture used 260,365 cycles, with a 580-byte path and about 0.14 ms path preparation. This establishes signed self-execution only. It does not establish production pool caller control, a complete buy's cost or private-route support. Research sources are preserved separately from this program change.

Implement the public admission boundary first. Keep complete private purchases blocked until the private holdings, wallet and atomic-return work is verified. Do not turn an interim public-only implementation into a claim that private participation is complete.

The maintained module is `crates/eligibility-policy`. Production guests use its typed admission interface. The saved experiment used an adapter to the same verifier. The sale-policy ID is the canonical pool account ID. Pool addresses do not depend on the root, so the creator can compute this ID before tree construction. Factory creation, pool preparation and canonical address checks validate the namespace and pool binding.

## 1. Establish control of the actual trader

Start with the native authorization trace, before changing pool messages. Follow root signing, actor wrappers, direct calls, token-transfer authority and delegated/PDA callers. `ReceiveInput.from` identifies the sender actor; `is_authorized` refers to the receiver. Do not substitute receiver authorization for evidence that the actual approved trader approved this purchase. Token spending alone does not settle this rule.

The current wrappers in `crates/curve-core/src/lib.rs` and `crates/lbp-core/src/lib.rs` check receiver authorization before forwarding. The direct buy handlers check same-program senders and use the sender as payer and public output recipient. Confirm which native checks bind that trader to this pool, operation, amount, bounds, recipient and complete transaction. Keep that existing payer/recipient relationship; no proof-supplied identity, alternate approval account or creator signature is added.

Acceptance: a copied membership path cannot buy under another trader; altered transaction contents require fresh authority; unauthorized direct, delegated or PDA routes fail; a new authorized transaction may reuse membership; native replay fails. If native control cannot be established on the pinned platform, stop program integration and record the blocker. Do not add a pretend signature check to bypass it.

## 2. Add a small shared buy-admission boundary

Introduce a typed `Open | Allowlist` policy and bounded membership evidence in a shared policy module. Select the final module location during the implementation review. Move the tested verifier out of its experiment package into that maintained boundary, retaining its cross-language vector and exact encoding rules. Production guests must not depend on a research package.

Admission receives trusted stored policy, actual trader and optional witness. Open pools need no witness. Allowlisted buys require the exact supported version, context and 17-level path. Fail closed for unsupported versions, malformed evidence, missing policy or mismatched namespace/sale/trader. No membership-consumption state is needed. Set payload bounds before allocating or decoding untrusted data.

This is the eligibility slice of [ADR 0031](../docs/adr/0031-shared-launch-factory.md) and LOG-54. It neither requires nor implements the complete fee/ending-policy refactor. Keep pricing math, reserves, custody, settlement and each engine's existing fee behavior separate and unchanged.

## 3. Carry immutable policy through creation

Add the typed policy to initial factory sale inputs and state in `crates/factory-core/src/lib.rs` and `crates/factory-core/src/lbp_setup.rs`. Carry it into the corresponding pool state in `crates/curve-core/src/lib.rs` and `crates/lbp-core/src/lib.rs`. Fixed-price and weighted setup must retain the same bytes through all four stages. No later setup message, retry, resume or role transfer may replace the policy or downgrade it to Open.

Choose the independent sale-policy ID before tree construction. Validate the namespace against creation authority and store the full policy in that sale's immutable terms. Bind the pool to those same terms and its actual namespace; if derived addresses include root-bearing terms, derive them only after the root exists. Do not calculate the Merkle context from a root-dependent pool address. Copying terms to another pool must not create an authority bypass; validate canonical pool identity and factory binding through the native-control work in step 1.

Direct neutral pools get an explicit Open choice by default. Configured direct pools must enforce their policy too. Reject inconsistent configuration at creation rather than interpret missing fields as Open on an allowlisted sale. No migration or backward compatibility is required by `CONTEXT.md`, but generated wire artifacts must change together.

## 4. Enforce admission in all three engines

Use `crates/curve-core/src/pool_swap.rs::{begin,finish}` and `crates/lbp-core/src/trading.rs::{begin,Pending::finish}`. Check eligibility before buy effects or external token calls. Carry the verified policy/trader association through pending state and validate the callback so later completion cannot swap the trader or purchase. Direct pool calls must pass the same checks as app/factory routes.

For bonding curves, gate collateral-in buys in both exact-input and exact-output forms. Do not gate permitted token-in curve sells or change `BuyOnly`/`BuyAndSell`. Gate both buy forms for fixed-price and weighted sales. Preserve current reserve, balance, inventory, fee, pause/freeze, end-time, deadline and rounding checks. An approved trader may buy all inventory if those rules permit it.

## 5. Change messages, clients and IDL together

Add the bounded witness to typed buy messages and policy to creation messages. Update client builders, generated `idl/launchpad.json` and wire checks in the same slice. Use `verify/generate-idl.sh` and `verify/check-idl.sh`; do not hand-edit generated output. Quotes still come from fresh pool state and do not reserve execution.

Keep the TypeScript enrollment builder's encoding equal to the maintained Rust verifier. Client validation can give clear errors, but it cannot replace pool enforcement. Do not add a per-buy creator service or a remote prover.

## Approved test seams and order

These public seams were approved before implementation. Work red-before-green, one behavior at a time through public interfaces. Signed execution uses `methods/tests/actor_sale.rs`; client wire checks use `crates/launchpad-client/tests/idl_wire.rs`. Public curve handlers, factory tests, LBP setup/trading tests, the maintained eligibility interface and source-IDL tests cover the remaining approved seams. Tests do not call private helpers.

1. Native trader control through wrappers and direct calls, including copied paths, invalid signatures, unauthorized delegated/PDA actors, changed messages, fresh authorized reuse and native replay.
2. Creation through every setup stage, with unchanged Open/Allowlist policy, valid namespace/pool binding and rejected policy replacement or downgrade.
3. Open, approved and unapproved exact-input/exact-output buys for every engine; direct-call bypass attempts; wrong root, context, trader, salt, path, size and version; permitted curve sells unchanged.
4. Economic regression cases: current fees, bounds, balance/reserve limits, deadlines, pause/freeze and endings. A late output-token failure must roll back purchase effects. Observe returned transaction diffs and actual account effects, not only a helper result.
5. Updated wire encoding and generated IDL agreement, then complete signed buy cycle charges at the existing platform limit. The isolated membership gate does not guarantee complete-buy headroom.

Weighted guest checks need the existing mock-clock support and must remain labeled synthetic. Normal weighted buys still require a supported clock provider. Keep host-only evidence distinct from guest execution and later real-proof evidence.

## 6. Integrate enrollment and wallet recovery

The creator collects only regular public trading addresses. Validate unique canonical addresses, build the salted tree, create the sale with the final policy, and deliver each participant's membership bundle through a protected channel. Do not log the full roster or unused salts. Public paths expose the used salt, index and sibling hashes; do not promise hidden roster size or unlinkability.

The wallet verifies control and target-platform usability of each account, validates its bundle against the sale policy, and stores keys and membership data inside the protected wallet boundary. Shared core, creator service and app do not custody keys. Prefer an unused approved address and reserve it for one attempt. If none remain, report that limit; deliberate reuse needs a privacy warning. Approval is not consumed on-chain.

Retain the attempt/account association through restart. Unknown outcome means reconcile the original transaction, not switch addresses and buy again. Confirmed purchases with delayed private synchronization must not be repeated. Changed contents require new approval. The final LP-0021 API and these recovery capabilities remain unverified; do not invent provider methods as completed support.

## 7. Complete the separate private-route work

`crates/private-flow-core/src/lib.rs` and `launchpad-client::submit_private_buy` currently return unsupported. Compatibility token holdings are public only. Removing the old eligibility ZK proof does not supply native private holdings, token authority, gas ownership, wallet support or native privacy proofs.

Require deshield funding, public buy through the actual approved trader, and shielding of output, unused input and unused gas into participant-controlled private accounts in one transaction. Actual fees are excluded. Validate source and destination control and asset compatibility. Reject the full purchase's application changes on any required late return failure; report actual transaction charges separately. No separate completed buy followed by a shielding retry is permitted.

Agree private-route test seams separately before implementation. Require complete success for every supported engine, late output/input/gas-return failures, incompatible destinations, total-call/cycle limits, native replay/spend checks and recovery after lost submission results. Do not publish inner receipts, private journals or witnesses. Keep proving local. Only then measure full-route proof time and cost and claim private support.

## Completion boundary

The public program slice is complete only after native trader control, immutable creation policy, all-engine direct-entry enforcement, wire checks, economic regressions and complete-buy charged-budget checks pass. The feature's required private route remains incomplete until step 7 passes. No deployment, live funds, platform-limit changes or tool installation is authorized by this plan.
