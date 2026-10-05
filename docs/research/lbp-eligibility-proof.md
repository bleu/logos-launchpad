# LBP eligibility-proof investigation

Status: Research candidate. The allowlist proof method is not selected. Final user review, privacy review and platform integration remain open.

Updated: 2026-10-05. Scope: A bounded design investigation. The accepted account and allowance rules are in [ADR 0021](../adr/0021-modes-and-open-limits.md). This report does not change them or claim production enforcement.

## Proposed direction

SHA-256 remains the cheaper application-proof candidate in this RISC0 implementation. Real local public Composite and private Composite/Succinct receipts now verify for a 100,000-account list. The public Composite trial took about 14 minutes and reached about 7.1 GiB of sampled process-group memory. That cost still needs user review. No account-check optimization was needed.

Direct Succinct receipt verification in the normal public guest exceeded the pinned LEZ limit of 33,554,432 cycles. This path is not viable as tested. The existing local Docker setup lacks the required Groth16 image, and ARM-host emulation remains untested. No image or tool was installed. A separate native private-program path now has genuine proofs for both approved account types accepted by the pinned local LEZ state validator. Its private-account proof took about 9 minutes; its public-account proof took about 24 minutes. Final cost, confidentiality and cached outer-circuit provenance review remain open.

The native path checks membership and account control inside a private program. LEZ's existing private transaction circuit authenticates its chained mock purchase. This avoids full receipt verification inside an ordinary public guest. The approved identity stays out of public pre-states and signers. This is a no-funds local fixture, not a deployed LBP or complete private route. Do not build production handlers to bypass the remaining gates.

## What the evidence proves

| Layer | Evidence | Limit |
| --- | --- | --- |
| Membership and account checker | Public/private control, SHA/Poseidon paths, cap and purchase binding tests pass. Independent cryptographic literals are used. | Synthetic regular wallet accounts; unaudited construction. |
| Shared purchase allowance | Host tests reject route switching, credential duplication and equivalent BIP340 keys after success. Rejected simulated trades preserve the allowance. Separate sales and private identifiers remain distinct. | Host simulation, not chain rollback, concurrency or durable state. |
| Guest execution | Depth-17 trees with 100,000 approved accounts pass for both account types and hashes. | Execution is not a proof. |
| Real local proof | SHA-256, depth 17: public Composite and private Composite/Succinct receipts verify against the embedded image and bound public purchase. Public receipt serialization/reverification passed. | Public Succinct not attempted after the private guest-verifier budget failure; cost and privacy still need review. |
| Normal public guest checker | Builds with SDK 3.0.6. Rejects Fake, Composite and incorrect Groth16 parameters. Malformed Groth16 with trusted parameters and a genuine private Succinct receipt reach the 33,554,432-cycle guest cap. | No positive in-budget verification; no genuine Groth16 receipt or LEZ transaction. |
| Native private-program path | Both account types execute the depth-17 SHA fixture. Genuine outer proofs for both pass strict verification and the pinned local LEZ transaction validator. Marker, mock purchase and private scratch effects commit together. | Fresh state per account type. No funded transaction, sequencer block or full route. Cached outer-circuit provenance and confidentiality remain open. |
| Wallet and full route | Not implemented or tested here. | The [single-transaction private route](../architecture/private-participation.md) still needs its own evidence. |

The prototype is in [packages/eligibility-proof](../../packages/eligibility-proof/README.md). It is isolated from production handlers, IDLs, SDK key custody, root dependencies and existing locks. The source and reference-vector record is [SOURCES.md](../../packages/eligibility-proof/SOURCES.md).

## Native and reference checks

The pinned LEZ revision is `a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a`. Regular private account IDs stay stable while their account state changes. Native state nullifiers prevent state reuse; they do not supply a one-buy-per-sale allowance. No verified generic native application allowlist registry was found. The native fixture uses a separate sale/account marker PDA. Its successful transitions for both approved account types and replay rejection now have local native-validator evidence.

Public IDs use BIP340 x-only secp256k1 keys. The checker canonicalizes equivalent secret scalars before deriving the sale marker. Private IDs use the native nullifier public key and account identifier; two private accounts with one wallet key retain separate allowances. These are control checks for regular accounts, not proof of human uniqueness or arbitrary program-owned account authority.

DistributionX at `c629c6c7bbd4b3aa56d4357f60817d49438e044b` is a reference only. Its LEZ pin is older (`47eba256479f6f785acbd138834340703cd03401`). Its Ed25519 account binding does not match the selected native public identity. Its salt-based marker does not preserve one allowance after credential redelivery unchanged. Its client journal-as-receipt path is a stub. Its direct Groth16 checker is a useful integration pattern, but no source or audit assurance was imported. Upstream proof timings are not estimates for this candidate.

Standalone application receipts cannot simply be added as LEZ program assumptions through the examined host interface. The research public guest calls full SDK receipt verification directly. It requires an explicitly selected Succinct or Groth16 receipt, verifier-owned default parameters, the pinned eligibility image, a successful claim, the authenticated journal, matching policy/purchase and canonical journal encoding. Succinct mode also supplies trusted default hash suites. Fake, Composite and mode/kind mismatches are rejected. Receipt metadata does not select trusted behavior. The guest's boolean result is test evidence, not a sale handler or transferable purchase permit. A real handler must derive policy and purchase from trusted sale/trade state and commit the verified marker with the trade.

The public verifier includes input decoding and journal work under the native `1024 * 1024 * 32 = 33,554,432`-cycle session cap. The genuine private SHA/depth-17 Succinct experiment failed with `Session limit exceeded: 33554432 >= 33554432`. Host verification and receipt-restoration checks had passed first. Complete verifier cycles and remaining sale-handler budget are unknown; the 256-million-cycle research checker bound was not used to hide this failure. The positive opt-in test stays as a visibly failing research gate.

Local Groth16 generation is not established. Docker Desktop 4.22.1 with Engine 24.0.5 now answers on Linux arm64. A bounded local image inspection returned `No such image: risczero/risc0-groth16-prover:v2025-04-03.1`. SDK documentation requires x86 for Docker proving; source has no hard host-architecture rejection and invokes Docker without `--platform`. ARM-host amd64 emulation was not tested. The bounded no-install fallback stopped here. No image download or Groth16 proof was attempted. A direct public-guest integration needs a genuine eligibility proof on a supported local setup. Compilation and invalid-seal rejection do not establish that result. The native experiment below uses a different path and does not resolve the direct public-guest budget failure.

## Candidate and participant setup

The creator needs known approved account IDs, a sale context and a private blinder for each delivered credential. Tree preparation does not require a buyer's account key or prior proof setup. The research tree accepts at most depth 17 and pads unused positions with a separate padding domain and index. It does not implement enrollment, authenticated delivery, a file format or immutable on-chain policy storage.

After initial delivery, the buyer needs the account-control material and local membership path. The checker has no creator-service dependency. The approved account is separate from the public trading account. Production delivery, storage protection, account recovery and buying without the creator's service still need wallet/chain tests. Predictable benchmark blinding must never become a production credential generator.

The private witness contains account-control material, the credential blinder, leaf index and siblings. The public permit contains sale and root commitments, hash choice, common cap, purchase commitment and sale allowance marker. It omits the approved account ID, key, blinder and path. Rejection commits no permit. A `None` journal cannot authorize a purchase.

The purchase commitment binds deployment/factory/namespace/sale, root, hash, depth, cap, trading account, recipient and full input/output amounts. The sale marker binds sale, approved account and canonical control key. It excludes blinding, route, hash choice and trading account. Duplicate or redelivered credentials therefore cannot create another allowance. Account replacement, one-person-one-account checks and key-sharing prevention are not added.

SHA-256 uses fixed `logos/lbp/eligibility/v1/` domains for sale, leaf, node, padding, purchase and nullifier. Poseidon is canonical Circom BN254 Poseidon, not Poseidon2. It uses separate leaf/node/padding tags and splits each digest into two little-endian 128-bit limbs. Arbitrary 256-bit inputs are not truncated or reduced into the field. Its output is a canonical 32-byte big-endian field value. Sale, purchase and marker hashing remain SHA-256 for both tree choices. The exact encoding and independent vectors are in code and tests.

## Proposed membership-data format v1

This is a delivery-format proposal, not an implemented file loader. Use a versioned JSON object with `version: 1`; the four `sale` context digests; `hash`, `depth`, `root` and `collateral_limit`; and `membership` with `index`, `blinder` and `siblings`. Digests are exactly 64 lowercase hexadecimal characters in byte order. `hash` is `sha256` or `circom-bn254-poseidon`. Depth and index are unsigned JSON integers; depth is at most 17 and index is below tree capacity. The collateral cap is a canonical decimal string that fits `u128`, so JSON number precision cannot change it. Reject unknown or missing fields rather than save unrelated wallet material.

The path contains exactly `depth` sibling digests, ordered from leaf to root. Bit zero of the leaf index selects the first branch; later bits select later branches. Version 1 fixes the code's domain tags, indexed padding and hash byte encodings; it does not let a credential choose new domains. SHA integer inputs use the code's fixed little-endian widths. The buyer must compare sale context, root, hash, depth and cap with trusted sale policy before local proving. A root supplied only by the delivery file is insufficient.

Deliver the blinder and path over an authenticated, protected initial channel. Keep the roster private. The buyer retains the credential locally so later proving does not need the creator. This file contains no public secret scalar, private NSK or other account-control secret. The protected local tool obtains control material separately and checks the leaf against the chosen approved account. Redelivery does not change the sale/account marker or add an allowance. Production delivery, storage and recovery remain separate work.

## Local measurements

The standalone measurements below use release builds on an Apple M3 Pro, 11 logical CPUs, 18 GiB RAM, macOS Darwin 23.4.0. The native experiment's profile is stated separately. Caches were warm; other system activity was not controlled. They are not capacity, wallet latency or minimum-memory guarantees. Creator ID collection and buyer account creation are excluded. The synthetic list has one public account and 99,999 regular private IDs from one published wallet key, with different identifiers. Exactly two paths are retrieved per tree; no batch of 100,000 proofs is generated.

Host Rust is 1.94.0. Guest Rust is 1.94.1. Installed RISC0 tools/executor remain 3.0.5; C++ is 2024.1.5. The initial measurements below used SDK/build library 3.0.5 and image `f6ed577574da9b5fc452b627c0d472862626f1921b7e896318d9aceefa2a3c36`. The continuation aligns only the isolated SDK/build library and three locks with root SDK 3.0.6. The earlier continuation measurements used eligibility image `961317040c198b1e0d7c49fef2aade777078a3d9f0a0211c805ef4afea824b75`. Adding the native instruction types changed the current standalone checker image to `04eea04e61e414e3b50741c3081cefba13203a2757f4fad682a632927960dc85`; the earlier proof costs were not remeasured for this image. Local IPC `r0vm` is used; development mode is disabled for proving and verification. Global tool pins and root/weighted-math locks remain unchanged.

### Initial SDK 3.0.5 observations

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

The two initial private receipts have 204,732 user cycles, 262,144 total padded cycles, one segment and a 536-byte journal. Prove time includes guest execution. Host verification rejects a changed image, altered journal or different purchase. The runner did not deliberately export receipts or witnesses, but SDK proving can write witness-bearing segment files. Their paths, permissions and cleanup were not inventoried in this first pass. Build time was excluded from its separate 600-second proof bound. There is no product proof-time limit.

An earlier public SHA-256 depth-0 attempt on the preliminary image `3954c429e474921be574bc57c1aa23713472d3426f55e8fd79d103a81f09590f` reached the 600-second kill limit, exit 137, without a completed receipt. The final-image depth-17 public Composite attempt also produced no receipt within the limit. Its timer reported 599.99 seconds, abnormal termination and `time: signal: Invalid argument`, exit 1. It produced no proof or verifier metrics; the timer's small memory figure does not establish prover memory use. An earlier interrupted attempt is not proof evidence. These results are not estimates of successful proving time.

### SDK 3.0.6 continuation

The unchanged SHA/depth-17 public-account checker completed a genuine Composite proof. The test restored the serialized receipt, verified it on the host, checked the independent sale/account marker and rejected a different image, root, purchase or altered journal.

| Public Composite proof / 100,000 accounts | Observation |
| --- | --- |
| Local prove time | 829.176223 s |
| Host verify/check/cleanup time | 0.159337 s |
| Test wall time | 830.23 s |
| Supervisor wall time | 831.658 s |
| User / padded cycles | 7,622,300 / 8,388,608 |
| Segments | 8 |
| Journal / serialized receipt | 536 / 2,254,884 bytes |
| Protected segment files / bytes | 8 / 577,108 |
| Normal cleanup | Directory removed |
| Sampled peak process-group RSS | 7,583,416,320 bytes |

The Python supervisor used a 1,800-second wall bound and a 12 GiB process-group RSS bound, with two-second sampling. Neither bound was reached. RSS sums the sampled process group without shared-memory deduplication; it is not the true peak or macOS `time` maximum RSS. The supervisor SHA-256 was `6adc612b0c6d47d29aa882069f654b8d7fd6d0ad31b01a4ceaee144a487e0703`. This run's host verification timer included artifact inventory and deletion; the runner now stops that timer before cleanup. No public-control optimization was made.

A separate SHA execution sample prepared IDs in 0.031546 s, built the tree in 0.115550 s and retrieved two paths in 0.000001 s. Public execution took 0.295756 s and private execution took 0.181758 s. Their cycles and segment counts matched the initial SHA observations. Ordinary local IPC execution uses inline output, so both protected-directory inventories had zero files and were removed normally. Genuine private Composite proving wrote one segment file; public Composite proving wrote eight.

A bounded private Succinct rerun completed proving in 32.593172 s and host verification/checks in 0.014705 s, excluding cleanup. It had 204,732 user cycles, 262,144 padded cycles, one segment, a 536-byte journal and a 227,496-byte serialized receipt. Its one protected segment file used 127,133 bytes and was removed. The public verifier input was 228,452 bytes. Host verification, receipt restoration and the corrected private-ID literal scans passed before the guest reached the exact cycle cap. The full experiment failed with exit 101 after 33.28 s; its sampled process-group RSS peak was 2,514,321,408 bytes. The supervisor completed because it explicitly checked this known failure, not because the positive guest test passed.

The malformed Groth16 test with trusted default parameters also reaches that cap, rather than returning `false`. SDK verifier-key pairing preparation occurs before parsing its invalid seal. Incorrect Groth16 parameters reject earlier and return `false`. Neither result supplies genuine Groth16 verification evidence. Full verification cost and an in-budget positive target path remain unmeasured.

### Native private-program experiment

The two application guests use eligibility image `4b2a931ef536ab00b8ce0a6a013444c6ef977550f62e9dc7341f6e82700e6451` and receiver image `6188e4cc274a8c4dd9861f2fe0f6cbefaee2e1b0b263d3f42d3b7dc5c883468c`. A fixed receiver-owned configuration account holds the trusted eligibility image and sale policy. The receiver rejects direct or foreign callers, changed policy and changed purchases. These negative caller and deliberately failed-purchase cases were guest execution tests, not proven native rollback tests.

| SHA-256 / depth 17 / 100,000 accounts | Two-guest user cycles | Execution wall time | Program runs |
| --- | --- | --- | --- |
| Private approved account | 1,428,039 | 0.227441 s | 2 |
| Public approved account | 8,844,533 | 0.283137 s | 2 |

Each application guest ran under the exact 33,554,432-cycle execution cap. These cycles exclude the outer privacy circuit and recursive proving. Execution wrote no segment files. The proof experiment used debug host tests, release guests and the pinned installed local prover. Fixture preparation, compilation and transaction validation are excluded from the prover timer.

These measurements use the same Apple M3 Pro, 11 logical CPUs, 18 GiB RAM and macOS Darwin 23.4.0. The host tests use the debug profile; the application guests use real release builds. The trusted cached outer privacy-circuit image is `ab86d257c5e0b8a8f6390b35500c37ca24fa57aa7a06bda330753cf36ad6a249`. Its relation to pinned source was not independently verified; see [SOURCES.md](../../packages/eligibility-proof/SOURCES.md#local-prover-provenance).

| Native outer proof / approved account | Private | Public |
| --- | --- | --- |
| Local complete driver prove time | 529.035436 s | 1,435.908950 s |
| Protected worker wall time, through verification and cleanup | 538.182004 s | 1,446.259825 s |
| Test wall time | 548.80 s | 1,457.18 s |
| Sampled peak child-process-group RSS | 6,677,807,104 bytes | 7,258,226,688 bytes |
| Borsh outer proof / complete transaction | 234,815 / 237,230 bytes | 234,815 / 237,230 bytes |
| Outer journal | 11,676 bytes | 11,676 bytes |
| New commitments / native nullifiers | 1 / 1 | 1 / 1 |
| Sampled peak artifact files / aggregate bytes | 3 / 695,059 | 9 / 781,049 |
| Normal cleanup | Protected subtree removed | Protected subtree removed |

One proof was attempted per account type, private first and public second. Neither reached the 1,800-second worker wall bound or 12 GiB sampled child-process-group RSS bound. The wall timer starts before child launch and excludes parent preflight, fixture preparation, compilation and transaction application. RSS samples sum the child process group without shared-memory deduplication and can miss short peaks. Parent crash cleanup and forced termination were not tested through the full native prover.

Strict host verification requires a Succinct outer receipt for the pinned native privacy circuit, trusted parameters and hash suites, real proving and canonical output. The pinned `V03State` validator then accepts the native message and proof. One transition uses the sale/account marker, increments the mock purchase and initializes a separate zero-balance private scratch account. That private effect is required by the native transaction format; a marker-only private transaction is invalid. Each account-type trial starts from fresh fixture state. This does not test both purchases against one live state or a complete public/private route.

Both genuine artifacts were reused without another proof. Corrupt seal, changed journal, changed purchase output, stale purchase nonce, changed trusted policy and replay all reject. Rejection preserves the observed marker, mock purchase and commitment root. The marker remains unused for the rejected pre-success cases. Successful-buy/replay checks use the actual native validator; deliberate chained purchase failure and credential redelivery remain execution-only evidence.

The complete serialized transaction is below 1 MiB. Its size includes the native message and witness set, including the outer proof and encrypted scratch output. It excludes block headers, other transactions and sequencer overhead. No block was built or submitted, so this is not block-acceptance evidence. The 10-program-run and 100 KiB account-data limits were not increased.

Both final outer proofs, journals, serialized transactions and captured proof-command outputs passed full raw, RISC0-word and hexadecimal scans for the selected control key, both approved fixture IDs, blinder and every sibling. This detects specified literal leaks only. Public configuration, the sale/account marker, mock purchase account and amounts remain visible. The scratch commitment, nullifier and encrypted output are also public. Native inner journals contain the instruction witness; they were not exported, logged or used as public transactions.

## Privacy and completion gates

A successful receipt verifies the statement. The absence of witness fields from the journal does not by itself establish witness confidentiality. SDK 3.0.6 describes ordinary Composite, Succinct and Groth16 receipts as zero knowledge; that claim depends on the prover and its build/settings. Composite exposes segment claims and attached assumptions. Receipt kind is not a separate confidentiality flag. Journals and claim commitments remain public. SDK source has randomized padding for ordinary proving; debug circuit/witness settings can change it. The inspected local binary matches the official archive and its published recipe excludes debug features, but an independent effective external-server compiler/feature record was not verified. See [prover provenance](../../packages/eligibility-proof/SOURCES.md#local-prover-provenance).

The host refuses set witness-debug, witness-dump, profiling and logging variables before fixture preparation. SDK proving uses explicit owner-only `0700` segment directories and reports only aggregate counts and sizes. Success checks removal; errors use normal temporary-directory cleanup. Signals or crashes can leave protected files. Deletion is not secure erasure, and directory permissions do not protect against a process with the same user authority. Both the public secret scalar and private `nullifier_key` (the long-term NSK) require protected local custody.

The genuine private Succinct receipt scan checks full raw and RISC0-word encodings of the synthetic NSK, approved private account ID, public fixture account ID, blinder and every sibling. CLI checks cover selected synthetic key/account encodings and private field names in successful and failed output. These checks detect specified literal leaks only. They do not prove mathematical zero knowledge, rule out partial or transformed leaks, or establish side-channel safety. A serialized public Composite receipt passed restoration and binding checks; an equivalent public-secret/path literal scan was not run. Do not publish witness data, debug traces or segment files based on these checks.

Public sale state, cap, purchase effects, amounts, timing and a same-sale allowance marker remain observable. This candidate must not publish a persistent cross-sale participant ID. The creator already knows the approved accounts; hiding the identity from the creator is not required. No buyer-tracking feature is added. Local proving tools must protect the long-term account keys; this package is not production key storage and does not implement key erasure or side-channel protection.

Before final proof selection, complete these investigation checks or split them into explicit follow-up research:

- Review local proof costs. Native proofs pass for both approved account types: about 9 minutes for private and 24 minutes for public on this host. These are single observations, not product latency guarantees.
- Validate confidentiality for the selected path, including both account types, disclosures, artifacts, logs and effective prover build/settings. Literal scans are insufficient. Independently check the cached outer-circuit artifact against the pinned source or trusted release evidence.
- Review native target-format evidence and its limits. Full block overhead, sequencer acceptance and supervised failure cleanup remain untested. Keep the failed direct public-guest verifier visible; a supported local Groth16 route remains untested.
- Review the results with the user. Record a selected approach only after its evidence and limits are accepted. Selection remains open if a follow-up is needed to select and test the approach.

Production work is separate from this bounded investigation. Before production use:

- Implement trusted immutable sale policy and shared on-chain marker storage. Test copied/wrong-account/wrong-sale/wrong-purchase proofs, both routes, duplicate credentials, concurrent attempts, above-cap buys and failed-trade rollback.
- Test the complete single-transaction private route, source authority, return accounting and total call budget. Custody and platform compatibility remain unverified by this prototype.
- Validate protected wallet proving, authenticated initial delivery, local membership storage and original-account recovery. Keep keys outside the app, creator service and planned shared core.

## Verification record

The current isolated package passes 16 core tests and 31 regular host tests: 16 standalone, 11 native execution and four native validator/startup tests. Nine host tests are opt-in. Current large-list standalone and native execution checks pass. One genuine native proof per approved account type passes, followed by artifact-reuse literal scans and corrupt-proof/output, stale-state and replay checks for both. Earlier standalone proof successes and the positive Succinct public-guest failure are recorded above with their historical images; those expensive proofs were not regenerated only to change the image label. The regular Groth16 budget test still requires the exact limit error.

Core and host lint checks pass with warnings denied. Guest source lint passes on the host target; actual locked RISC0 guest builds, regular execution and the large-list test were run again after host lint's skipped build. All three format checks pass. Root `cargo test --workspace --locked --offline` passed with 196 tests. Root and all four weighted-math locks remain unchanged.

`verify/check-pins.sh` passed. `verify/check-pins.sh --tools` failed on the local scaffold installation metadata for expected revision `f919fd03b064a91a4f261093cf2b0b4f8150e27c`. No tool installation or global configuration was changed. RISC0's active Rust, C++, executor and cargo tools were checked separately and match the recorded pins. No funded transaction, deployment, live wallet key or remote proving was used. See the [reproduction commands](../../packages/eligibility-proof/README.md).
