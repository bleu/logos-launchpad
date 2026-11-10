# Superseded allowlist proof proposal

Status: Superseded on 2026-11-10. Unimplemented historical proposal, not current protocol guidance.

Date: 2026-11-09; superseded: 2026-11-10

The 2026-11-10 amendment to [ADR 0021](../adr/0021-modes-and-open-limits.md) replaces this proposal's account model with pre-approved batches of public trading addresses. Read the [current plain-English design](allowlist-proof.md). Private funding and complete private returns remain required.

The separate private checking actor, private approved-account secret derivation and mandatory private eligibility guest below are no longer requirements. The formats, hash rules, acceptance checks and release gates below describe only the former candidate. They do not select the new membership format or verifier and must not be used as current implementation instructions. Historical measurements and source references are retained without claiming present compatibility or performance.

## Encoding and fixed policy

These are proposed protocol bytes, not an existing on-chain interface. `H` means SHA-256. A quoted domain is its literal ASCII bytes, including the final zero byte written as `\0`. `||` means byte concatenation. All integers are unsigned little-endian. Digests and account IDs are raw 32-byte values, not text. An `ActorRef` is `account_id[32] || program_account_id[32]` in that order. An image ID is its eight `u32` words in little-endian order. Fixed records have the field order below, with no padding or length prefix. Reject trailing bytes, unknown tags and unsupported versions. These rules match the corresponding fixed-field Borsh layout.

An enabled pool stores `PolicyV1` and `root[32]` before buying opens:

| `PolicyV1` field, in order | Encoding |
| --- | --- |
| Protocol version | `u16`, value 1 |
| Deployment domain | `[u8; 32]`, fixed for the intended network and deployment |
| Pool | `ActorRef` |
| Namespace | `[u8; 32]` |
| Factory program | `[u8; 32]` |
| Sale context | `[u8; 32]`, creation seed; independent of the roster root |
| Pricing engine | `u8`: 0 bonding-curve, 1 fixed-price, 2 weighted |
| Eligibility program account | `[u8; 32]` |
| Eligibility program image | Eight `u32` words |
| Tree hash | `u8`, value 0 for SHA-256 |
| Tree depth | `u8`, value 17 |

`policy_id = H("LP/allow-policy/v1\0" || encode(PolicyV1))`. The descriptor excludes the root to avoid circular hashing. Direct pools use their configured namespace/factory/sale context, with explicit zero values only where those roles are absent. The pool and deployment still distinguish them. Open pools use an explicit disabled-policy variant; a zero root is not an open-policy shortcut.

The deployment domain must come from trusted deployment configuration, not a buyer's request. Creation must validate the pool, context and supported immutable eligibility code. Store policy and root immutably. No root update, revocation or policy downgrade is part of this version. Program address alone is insufficient if its code can change: the deployment must enforce the recorded image binding or reject this design.

## Enrollment and tree

Candidate account kinds are regular key-controlled public accounts (`0`) and regular key-controlled private accounts (`1`). PDA and other program-controlled accounts need a separate control protocol; they are not supported by this candidate.

The creator rejects duplicate `(kind, account_id)` entries, empty enabled lists and lists above 131,072 members. Each member receives an independent 32-byte blinder from a cryptographically secure random source. Do not derive it from an address, reuse it across sales or give it to other members.

```text
leaf = H("LP/allow-leaf/v1\0" || policy_id || kind:u8 || account_id[32] || blinder[32])
padding(i) = H("LP/allow-pad/v1\0" || policy_id || i:u32)
node(left, right) = H("LP/allow-node/v1\0" || left[32] || right[32])
```

Sort real leaf hashes in unsigned byte order, reject equal hashes, then place them in slots `0..member_count`. Fill the remaining slots through 131,071 with `padding(slot_index)`. Hash ordered adjacent pairs for 17 levels. Do not sort sibling pairs. A path contains a `u32` index below 131,072 and exactly 17 sibling digests, starting at the leaf. At level `j`, bit `j` of the index chooses whether the current hash is the left child (`0`) or right child (`1`). The final hash must equal the stored root.

Deliver `PolicyV1`, root, account kind, blinder, index and siblings through an authenticated confidential channel to the participant's protected local storage. The participant checks their account ID, trusted on-chain policy and recomputed root. Delivery data never grants authority by itself. No signing or authorization secret goes to the creator.

The creator necessarily knows the roster. Salted leaves prevent a simple search over likely addresses, but do not hide membership from the creator or prevent facts being inferred from trades. Participant count is not a public policy field. Tree correctness and duplicate rejection require enrollment checks; the public root alone cannot prove that the creator followed them. Duplicate approval does not grant more rights because there is no allowance.

## Purchase intent

The private checker accepts a canonical `BuyIntentV1`:

| Field, in order | Encoding |
| --- | --- |
| Protocol version | `u16`, value 1 |
| Policy ID and roster root | Two `[u8; 32]` values |
| Pool | `ActorRef` |
| Pricing engine | `u8`, same tags as policy |
| Operation | `u8`: 0 exact input, 1 exact output |
| Collateral and output token definitions | Two `[u8; 32]` values |
| Specified amount | `u128`: full input for exact input; desired output for exact output |
| Trade limit | `u128`: minimum output for exact input; maximum full input for exact output |
| Trader, collateral payer and output recipient | Three `ActorRef` values |
| Funding mode | `u8`: 0 public, 1 private |
| Attempt binding | `[u8; 32]` |
| Route commitment | `[u8; 32]` |

`purchase_hash = H("LP/allow-buy/v1\0" || encode(BuyIntentV1))`. The policy ID binds deployment, sale context and eligibility code. Amounts are raw token units. Full input includes the participant's applicable protocol fee. The intent authorizes bounds, not a stale quote or a fixed realized weighted price. The pool computes actual input, output and fees from current state and checks those bounds. Existing time, direction, reserve, pause, ending and arithmetic checks remain. Execution-time validity or clock inputs must not become unsigned caller overrides through this adapter.

For each attempt, generate independent 32-byte `attempt_blind` and a fresh private regular checking actor. Define `attempt_binding = H("LP/allow-attempt/v1\0" || encode(checking_actor) || attempt_blind)`. The guest recomputes this from its actual receiver, not a caller's claim about the receiver. Keep the canonical checking-account ID and blinder private; public sender presentation must use a fresh blinded address.

Require native authorization of the checking actor and an unused initial state. On its one purchase entry, store the attempt's purchase hash in private checking state and emit at most one buy. Reject later purchase requests against that state. This is per-attempt state, not an approved-account or sale-level allowance. Its native initialization/update nullifiers prevent a completed checking attempt from being executed again. Do not expose its private state or retain a public participant marker.

The complete native transaction still binds its signatures, public nonces, private inputs and synchronous calls under LEZ validation. Do not put the final transaction hash or proof bytes into the intent: that would create a circular dependency. Validate the fresh-checking-account lifecycle and replay binding before accepting this protocol.

## Approved-account control

The membership witness contains the account kind, derived approved account ID, blinder, index and path. It also contains the following control evidence. All of this remains inside private execution.

For a public approved account, the wallet produces a BIP340 signature over the 32-byte `purchase_hash`. The checker strictly validates the x-only secp256k1 public key and signature, derives the LEZ public account ID from that key, and checks its leaf. The purchase hash includes the attempt binding; the guest verifies that binding against its real receiver. Copying the signature to a fresh checking actor therefore fails. The wallet does not export the signing key to the creator, app or shared core. Signature API compatibility and guest cost remain unverified.

For a private approved account, the protected wallet/prover supplies the authorization secret and viewing public key. The checker uses the pinned LEZ derivations: authorization secret to nullifier secret key, nullifier secret key to nullifier public key, then `AccountId::for_regular_private_account` with the viewing public key. It checks the leaf for that derived ID. The old prototype's private-account identifier format must not be reused.

This private-account check proves knowledge of control material. It is not native `is_authorized` for the approved account, does not consume that account's current state and grants no authority to spend its funds. It needs a security review and a wallet interface that keeps the secret entirely within the protected local proving boundary. Membership data or a viewing key alone must not pass. If the wallet cannot supply this proof safely, private-approved support is blocked; a public-only fallback does not meet the product rules.

Do not use the approved private account itself as the checking actor. Native execution produces an account-update nullifier even for unchanged witnessed state. Doing that for every buy would make otherwise independent purchases compete for the same state.

## Funding and private returns

Define `RoutePlanV1` with the following field order: version `u16 = 1`; funding mode `u8`; collateral definition `[32]`; output definition `[32]`; collateral source `ActorRef`; native-gas source `ActorRef`; public collateral, output and gas accounts as three `ActorRef` values; output, unused-input and unused-gas destinations as three `ActorRef` values; collateral funding, native-gas funding and maximum native charge as three `u128` values.

For a private route, all return destinations are private and participant-controlled. For a public route, this record describes the authorized public funding and destinations; it makes no private-return claim. Use zero absent-source or absent-gas fields only when no corresponding funding operation is required, with explicit validation; never as an unknown destination. Native gas is identified by the selected platform rather than by the collateral definition. Validate account programs, assets, source spending authority and destination control against actual route operations.

`route_commitment = H("LP/allow-route/v1\0" || route_blind[32] || encode(RoutePlanV1))`, with an independent random 32-byte blinder per attempt. The guest recomputes it and checks consistency with the purchase intent. Publish neither raw private source/destination IDs nor the route opening.

The trusted private route code must construct and enforce the funding, purchase and return calls from this plan. Do not accept a caller-supplied plan hash without enforcing its opening. Return actual output and unused input, and return unused gas after actual charges. Do not confuse maximum input or maximum charge with amounts actually spent. A required late return failure must reject the purchase's application changes. Pool admission checks do not alone establish that these later returns occurred.

This record specifies authorized endpoints and funding bounds. It is not a claim that current token or wallet APIs can execute them. The concrete token/gas call graph, its account-authority grants, actual-balance accounting and whole-route proof remain implementation gates. No separate-transaction return fallback is allowed.

## Authenticated pool entry

The eligibility guest must construct a typed synchronous buy from the checked intent. It must not return a transferable approval object for some later transaction. Every code path that can issue an allowlisted buy must perform the membership, account-control and attempt checks. Private-route code must additionally enforce its committed plan.

At every pool buy entry, including a direct factory-bypassing call:

1. Read the actual enabled policy and root from pool state. Open-policy buys keep their ordinary admission rules.
2. Require an actual native sender and the configured eligibility program in `ReceiveInput.from.program_account_id`, backed by the deployed immutable image. Reject root calls, forged caller fields and other programs.
3. Compare the policy ID, root, pool actor, engine, definitions, operation, accounts and amount bounds with the actual typed buy. Recompute the purchase hash; do not treat an arbitrary hash as checked purchase data.
4. Obtain genuine token-debit authority for the explicit payer/trader. Then apply normal pricing, custody, fees and lifecycle checks before effects commit.

The runtime supplies `ReceiveInput.from`. `ReceiveInput.is_authorized` describes the receiver, not its caller. Private inherited account authorizations are withheld from public handlers; eligibility does not replace token authority. The native proof and validator must bind private program claims and the presented sender to deployed code. A public digest, boolean or caller-selected program address is insufficient.

Current curve and fixed/weighted handlers infer payer and recipient from their sender. They cannot accept a blinded checking sender as the trader without a new checked delegated-buy boundary. That boundary and its real spending grants are required work, not existing functionality. The policy refactor must apply this admission rule to all three engines without changing their accounting. Existing permitted curve sells remain outside this allowlist gate.

## Repeated buys, replay and disclosure

A participant can reuse the same membership path with a new intent and fresh checking account. There is no collateral cap, purchase counter, consumed eligibility allowance or sale/account nullifier. Independent attempts may succeed when funding, state and normal trade checks permit. Shared funding state can still conflict, and current pool state can make a proof stale. There is no promise that transactions prepared against the same snapshot both execute.

Copied membership data fails without account control. Changed pool, root, amounts, limits, accounts or route commitment require new proof and approval. A copied public-account signature cannot authorize another checking actor. A completed checking account cannot emit another buy. Exact transaction replay must fail native replay checks. These are required behaviors, not tested results.

Public observers see the root/policy, eligibility program, fresh presented sender, purchase terms, public trade effects, outer proof and required native commitments/nullifiers. They must not see the approved private account, public-account control evidence, membership path/blinder, route opening, authorization secret or inner execution receipts/journals. Native outputs still need a confidentiality review; a literal scan is insufficient. Inner receipts and journals can contain the witness and must never be published.

Proving remains participant-local. Keys, control witnesses and protected recovery material stay outside the app and shared core. Keep the existing minimal progress journal and unknown-outcome rules: no automatic replacement while the original attempt is unresolved. New proof material does not turn recovery into a new authorized purchase.

Approval proves account control, not human uniqueness. Full-key sharing can transfer authority. Public funding/trading accounts, amounts and timing can expose or link participation. No stronger privacy claim is made.

## Evidence and release gates

The historical native prototype at `87cf9a509101bf1fdb5347adb85fc77e6321e51d` checked account control and membership with a no-funds mock purchase on the earlier v03 platform. Single outer-proof observations were about 9 minutes for a private approved account and 24 minutes for a public approved account. Its cap, allowance marker, account format and purchase commitment do not define this protocol. That evidence does not establish current performance, source-to-binary provenance, funded execution or confidentiality.

The [Semaphore experiment](../../packages/eligibility-proof-lite/README.md) generated a small proof quickly, but its prepared-key verifier exceeded the 10,000,000-cycle public action gate. It did not prove approved LEZ account control. This native candidate has not been measured against the current public-action, private-execution or full-route limits; do not treat a larger fallback budget or provisional verification fee as proof of admission.

Before selection or release, establish:

- Independent review of canonical encodings, domain separation, account-control checks, fresh-checking-account replay and deployed-code authentication. Produce independent literal encoding/hash vectors before implementation tests.
- Wallet support for protected local witnesses, public signatures, fresh checking accounts, approval, native proof construction and safe recovery. Review the source and binary provenance of the native outer circuit.
- Direct-call bypass rejection and actual delegated token authority on all engines, both exact-input/output operations and both funding routes.
- Repeated and independent concurrent buys for both approved-account kinds, wrong-account/root/pool/amount/limit/destination rejection, copied-approval rejection and exact transaction replay rejection.
- Compatible private holdings and a complete funded transaction with output, unused-input and unused-gas returns. Force a late return failure and inspect application rollback separately from actual fees and native replay effects.
- Current measurements for 100,000 members: enrollment, witness preparation, guest execution, local outer proving, peak memory, proof/transaction bytes and complete admission cost with trade headroom. Confidentiality review must include outputs, logs, local artifacts and recovery records.

Current compatibility tokens have public holdings only, and private helpers return unsupported. A Merkle proof does not resolve that blocker. No new proofs, benchmarks or executable tests were run to write this specification. Future test boundaries require agreement before tests are added.

## Pinned source basis

LEZ revision: `578ce58a71b79c49d48e97e51c520bf71f0252d9`.

- [Private regular-account and nullifier-key derivations](https://github.com/logos-blockchain/logos-execution-zone/blob/578ce58a71b79c49d48e97e51c520bf71f0252d9/lee/state_machine/core/src/nullifier.rs#L11-L102).
- [Public account mapping and x-only key validation](https://github.com/logos-blockchain/logos-execution-zone/blob/578ce58a71b79c49d48e97e51c520bf71f0252d9/lee/state_machine/src/signature/public_key.rs#L45-L85).
- [Actor messages, sender and receiver authorization](https://github.com/logos-blockchain/logos-execution-zone/blob/578ce58a71b79c49d48e97e51c520bf71f0252d9/lee/state_machine/core/src/program/mod.rs#L300-L394).
- [Private/public execution boundary and sender presentation](https://github.com/logos-blockchain/logos-execution-zone/blob/578ce58a71b79c49d48e97e51c520bf71f0252d9/lee/state_machine/core/src/execution_state.rs#L1154-L1359).
- [Program-image claims and native receipt validation](https://github.com/logos-blockchain/logos-execution-zone/blob/578ce58a71b79c49d48e97e51c520bf71f0252d9/lee/state_machine/src/validated_state_diff/mod.rs#L510-L543).
- [Private witness actions, including unchanged account state](https://github.com/logos-blockchain/logos-execution-zone/blob/578ce58a71b79c49d48e97e51c520bf71f0252d9/lee/privacy_preserving_circuit/src/output.rs#L134-L175).
