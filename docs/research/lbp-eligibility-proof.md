# LBP eligibility-proof investigation

Status: Research candidate. The allowlist proof method is not selected. Final user review, privacy review and platform integration remain open.

Updated: 2026-10-05. Scope: A bounded design investigation. The accepted account and allowance rules are in [ADR 0021](../adr/0021-modes-and-open-limits.md). This report does not change them or claim production enforcement.

## Proposed direction

Continue with SHA-256 for the application-proof candidate. In this RISC0 implementation, it costs less than Poseidon and has real local proofs for a private account in a 100,000-account list. Both public and private account-control paths passed guest execution. A completed public-account proof is not yet established. These results do not select a production proof method or approve its local cost.

Native LEZ reuse remains preferable where it meets the requirements. Direct native public-account authorization publishes the approved account ID, so it cannot meet the accepted private-route requirement for that account type unchanged. The next investigation must settle the public-account proving cost, receipt confidentiality and current-platform verifier route before final selection. These prototype results do not complete proof selection.

## What the evidence proves

| Layer | Evidence | Limit |
| --- | --- | --- |
| Membership and account checker | Public/private control, SHA/Poseidon paths, cap and purchase binding tests pass. Independent cryptographic literals are used. | Synthetic regular wallet accounts; unaudited construction. |
| Shared purchase allowance | Host tests reject route switching, credential duplication and equivalent BIP340 keys after success. Rejected simulated trades preserve the allowance. Separate sales and private identifiers remain distinct. | Host simulation, not chain rollback, concurrency or durable state. |
| Guest execution | Depth-17 trees with 100,000 approved accounts pass for both account types and hashes. | Execution is not a proof. |
| Real local proof | Private account, SHA-256, depth 17: Composite and Succinct receipts verify against the embedded image and bound public purchase. | Not a complete private route or confidentiality validation. |
| Normal public guest checker | Compiles with SDK 3.0.5. Rejects Fake receipts and malformed Groth16 receipts, including incorrect verifier parameters. A real Composite receipt is also rejected. | No successful Groth16 verification; no LEZ transaction; root SDK 3.0.6 compatibility unverified. |
| Wallet, chain and full route | Not implemented or tested here. | The [single-transaction private route](../architecture/private-participation.md) still needs its own evidence. |

The prototype is in [packages/eligibility-proof](../../packages/eligibility-proof/README.md). It is isolated from production handlers, IDLs, SDK key custody, root dependencies and existing locks. The source and reference-vector record is [SOURCES.md](../../packages/eligibility-proof/SOURCES.md).

## Native and reference checks

The pinned LEZ revision is `a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a`. Regular private account IDs stay stable while their account state changes. Native state nullifiers prevent state reuse; they do not supply a one-buy-per-sale allowance. Native commitment paths and account control can support further inline private-program research. No verified generic native application allowlist registry was found. A sale/account marker PDA remains an untested candidate.

Public IDs use BIP340 x-only secp256k1 keys. The checker canonicalizes equivalent secret scalars before deriving the sale marker. Private IDs use the native nullifier public key and account identifier; two private accounts with one wallet key retain separate allowances. These are control checks for regular accounts, not proof of human uniqueness or arbitrary program-owned account authority.

DistributionX at `c629c6c7bbd4b3aa56d4357f60817d49438e044b` is a reference only. Its LEZ pin is older (`47eba256479f6f785acbd138834340703cd03401`). Its Ed25519 account binding does not match the selected native public identity. Its salt-based marker does not preserve one allowance after credential redelivery unchanged. Its client journal-as-receipt path is a stub. Its direct Groth16 checker is a useful integration pattern, but no source or audit assurance was imported. Upstream proof timings are not estimates for this candidate.

Standalone application receipts cannot simply be added as LEZ program assumptions through the examined host interface. The research public guest instead calls SDK receipt verification directly. It requires Groth16, verifier-owned default parameters, the pinned eligibility image, a successful claim, the authenticated journal, matching policy/purchase and canonical journal encoding. Receipt metadata does not select trusted behavior. The guest's boolean result is test evidence, not a sale handler or transferable purchase permit. A real handler must derive policy and purchase from trusted sale/trade state and commit the verified marker with the trade.

Local Groth16 generation is not established. Docker CLI 24.0.5 exists, but its daemon did not answer. The pinned Docker proving route requires x86; this host is Darwin arm64. The image is `risczero/risc0-groth16-prover:v2025-04-03.1`. Positive public-guest verification must be tested with a genuine proof on supported local hardware. Compilation and invalid-seal rejection do not establish it.

## Candidate and participant setup

The creator needs known approved account IDs, a sale context and a private blinder for each delivered credential. Tree preparation does not require a buyer's account key or prior proof setup. The research tree accepts at most depth 17 and pads unused positions with a separate padding domain and index. It does not implement enrollment, authenticated delivery, a file format or immutable on-chain policy storage.

After initial delivery, the buyer needs the account-control material and local membership path. The checker has no creator-service dependency. The approved account is separate from the public trading account. Production delivery, storage protection, account recovery and buying without the creator's service still need wallet/chain tests. Predictable benchmark blinding must never become a production credential generator.

The private witness contains account-control material, the credential blinder, leaf index and siblings. The public permit contains sale and root commitments, hash choice, common cap, purchase commitment and sale allowance marker. It omits the approved account ID, key, blinder and path. Rejection commits no permit. A `None` journal cannot authorize a purchase.

The purchase commitment binds deployment/factory/namespace/sale, root, hash, depth, cap, trading account, recipient and full input/output amounts. The sale marker binds sale, approved account and canonical control key. It excludes blinding, route, hash choice and trading account. Duplicate or redelivered credentials therefore cannot create another allowance. Account replacement, one-person-one-account checks and key-sharing prevention are not added.

SHA-256 uses fixed `logos/lbp/eligibility/v1/` domains for sale, leaf, node, padding, purchase and nullifier. Poseidon is canonical Circom BN254 Poseidon, not Poseidon2. It uses separate leaf/node/padding tags and splits each digest into two little-endian 128-bit limbs. Arbitrary 256-bit inputs are not truncated or reduced into the field. Its output is a canonical 32-byte big-endian field value. Sale, purchase and marker hashing remain SHA-256 for both tree choices. The exact encoding and independent vectors are in code and tests.

## Local measurements

These are single release-build observations on an Apple M3 Pro, 11 logical CPUs, 18 GiB RAM, macOS Darwin 23.4.0. Caches were warm; other system activity was not controlled. They are not capacity, wallet latency or minimum-memory guarantees. Creator ID collection and buyer account creation are excluded. The synthetic list has one public account and 99,999 regular private IDs from one published wallet key, with different identifiers. Exactly two paths are retrieved per tree; no batch of 100,000 proofs is generated.

Host Rust is 1.94.0. Guest Rust is 1.94.1. RISC0 tools/build/SDK are 3.0.5; C++ is 2024.1.5. The root lock remains at SDK 3.0.6. The committed core, zkvm host and guest locks define the other versions. Local IPC `r0vm` is used; development mode is disabled for proving and verification. The final eligibility image is `f6ed577574da9b5fc452b627c0d472862626f1921b7e896318d9aceefa2a3c36`.

| Tree hash | Synthetic ID preparation | Tree construction | Two membership paths | Process max RSS |
| --- | --- | --- | --- | --- |
| SHA-256 | 0.056918 s | 0.113861 s | 0.000001 s | 76,726,272 bytes |
| Poseidon | 0.030537 s | 19.595826 s | 0.000007 s | 80,871,424 bytes |

Both trees contain 100,000 approved leaves and 31,072 padding leaves. Capacity is 131,072. Memory values are macOS `/usr/bin/time -l` maximum resident set size for the whole command and children, not separate tree memory or a sum of simultaneous process memory.

| Hash / account | Host checker | Guest execution wall time | User cycles | Padded cycles | Segments |
| --- | --- | --- | --- | --- | --- |
| SHA-256 / public | 0.000115 s | 0.251243 s | 7,622,300 | 8,388,608 | 8 |
| SHA-256 / private | 0.000024 s | 0.178577 s | 204,732 | 262,144 | 1 |
| Poseidon / public | 0.001194 s | 1.158264 s | 106,586,406 | 113,246,208 | 108 |
| Poseidon / private | 0.001305 s | 1.128579 s | 99,168,483 | 105,381,888 | 101 |

Execution wall time includes local IPC startup. Segment size is capped at `2^20`. The initial 64-million-cycle session bound rejected the depth-17 Poseidon run. Raising the operational session bound to 256 million cycles allowed all four execution cases to pass. This does not change a product limit. These numbers compare this guest implementation, not hashes in a dedicated proof circuit.

| Real proof | Prove time | Verify time | Max RSS | Result |
| --- | --- | --- | --- | --- |
| SHA-256 / private / Composite / depth 17 | 23.750422 s | 0.017036 s | 2,314,436,608 bytes | Verified |
| SHA-256 / private / Succinct / depth 17 | 33.524393 s | 0.014561 s | 2,267,086,848 bytes | Verified |
| SHA-256 / public / Composite / depth 17 | No completed receipt within 600 s | Not established | Not established | Operational limit reached |
| Poseidon real proofs | Not attempted | Not established | Not established | Not a finalist for this guest |
| Groth16 | Unavailable here | Not established | Not established | Local platform/tooling gate |

Both completed receipts have 204,732 user cycles, 262,144 total padded cycles, one segment and a 536-byte journal. Prove time includes guest execution. Host verification rejects a changed image, altered journal or different purchase. No receipt or witness artifact is saved by the runner. Build time is excluded from the proof operational limit; each expensive attempt is bounded separately at 600 seconds. There is no product proof-time limit.

An earlier public SHA-256 depth-0 attempt on the preliminary image `3954c429e474921be574bc57c1aa23713472d3426f55e8fd79d103a81f09590f` reached the 600-second kill limit, exit 137, without a completed receipt. The final-image depth-17 public Composite attempt also produced no receipt within the limit. Its timer reported 599.99 seconds, abnormal termination and `time: signal: Invalid argument`, exit 1. It produced no proof or verifier metrics; the timer's small memory figure does not establish prover memory use. An earlier interrupted attempt is not proof evidence. These results are not estimates of successful proving time.

## Privacy and completion gates

A successful receipt verifies the statement. The absence of witness fields from the journal does not by itself establish witness confidentiality. SDK 3.0.5 Composite receipts expose segment claims and attached assumptions; Succinct is a recursive STARK and Groth16 adds a SNARK wrapper. Receipt kind is not a separate confidentiality flag. Journals and claim commitments remain public. SDK source has randomized padding for ordinary proving; debug circuit/witness settings can change it. This source check and the real proofs do not complete a confidentiality review. Do not publish witness data, debug traces, segment files or receipt internals on that basis.

Public sale state, cap, purchase effects, amounts, timing and a same-sale allowance marker remain observable. This candidate must not publish a persistent cross-sale participant ID. The creator already knows the approved accounts; hiding the identity from the creator is not required. No buyer-tracking feature is added. Local proving tools must protect the long-term account keys; this package is not production key storage and does not implement key erasure or side-channel protection.

Before final proof selection, complete these investigation checks or split them into explicit follow-up research:

- Finish the public-account real-proof and local-cost review. Do not infer it from the cheaper private account path.
- Validate confidentiality for the selected receipt mode and its public disclosures, artifacts, logs and debug settings.
- Verify a genuine Groth16 proof in the normal public guest, then establish compatibility with the actual root SDK 3.0.6 and pinned LEZ transaction interface. Compare inline private-program checking and native marker-account reuse without changing LEZ in this task.
- Review the results with the user. Record a selected approach only after its evidence and limits are accepted. Selection remains open if a follow-up is needed to select and test the approach.

Production work is separate from this bounded investigation. Before production use:

- Implement trusted immutable sale policy and shared on-chain marker storage. Test copied/wrong-account/wrong-sale/wrong-purchase proofs, both routes, duplicate credentials, concurrent attempts, above-cap buys and failed-trade rollback.
- Test the complete single-transaction private route, source authority, return accounting and total call budget. Custody and platform compatibility remain unverified by this prototype.
- Validate protected wallet proving, authenticated initial delivery, local membership storage and original-account recovery. Keep keys outside the app, creator service and planned shared core.

## Verification record

The 16 core tests and nine regular host/guest tests pass. The large-list guest test and real private receipt-binding test passed when invoked explicitly. The public real-proof test remains opt-in and has no completed passing run. Core and host lint checks pass with warnings denied. Guest source lint passes on the host target; actual RISC0 guest builds and execution are checked separately. All three format checks pass. The root `cargo test --workspace --locked --offline` passed with 196 tests; existing root and weighted-math locks were not changed.

`verify/check-pins.sh` passed. `verify/check-pins.sh --tools` failed on the local scaffold installation metadata for expected revision `f919fd03b064a91a4f261093cf2b0b4f8150e27c`. No tool installation or global configuration was changed. RISC0's active Rust, C++, executor and cargo tools were checked separately and match the recorded pins. No funded transaction, deployment, live wallet key or remote proving was used. See the [reproduction commands](../../packages/eligibility-proof/README.md).
