# Public-account membership experiment

This package tests a SHA-256 Merkle path for the [approved public trading-account model](../../docs/architecture/allowlist-proof.md). The local signed checks passed on 2026-11-10 at the unchanged 10,000,000-cycle LEZ public-action limit. Public pools in all three engines use the maintained `crates/eligibility-policy` module, and this experiment's Rust adapter now calls that verifier. Native private proving and complete private purchases were not tested.

The old Semaphore experiment and its failed verifier gates are preserved under [legacy/semaphore](legacy/semaphore/HISTORY.md). Its dependencies, results and commands are historical. The default package commands now select only the Merkle experiment.

## Measured results

One fresh Node child ran per roster size, followed by the same signed local guest test. These are single measurements, not latency guarantees. List construction uses fixed depth 17 at both sizes. Account preparation derives synthetic public accounts; it is separate from list construction and participant path preparation. Existing tools, dependencies and OS caches were used.

| Measurement | 3 addresses | 100,000 addresses |
| --- | ---: | ---: |
| Synthetic account preparation, seconds | 0.001648625 | 17.339410958 |
| List construction, seconds | 0.218361 | 0.650329334 |
| Path preparation and encoding, seconds | 0.000108625 | 0.000139958 |
| Node verification, seconds | 0.000075208 | 0.000098375 |
| Node child work, seconds | 0.220297292 | 17.990178708 |
| Node process wall time, seconds | 0.325504625 | 18.145896958 |
| Child peak RSS, bytes | 270696448 | 484950016 |
| Rust host verification, seconds | 0.000161667 | 0.000162083 |
| Signed guest execution wall time, seconds | 0.040279250 | 0.040868542 |
| Charged transaction action cycles | 260364 | 260365 |
| Public membership witness, bytes | 580 | 580 |
| Trusted policy, bytes | 97 | 97 |

The table preserves the original experiment version. After the Rust adapter switched to the maintained verifier, the isolated signed gate passed again at 271,708 cycles for three addresses and 271,709 cycles for 100,000 addresses. Node construction and path timings were not rerun for that adapter change.

The witness is a reusable public path, not a zero-knowledge proof. It can be prepared at enrollment. These timings do not measure or reduce native private-transaction proving. Guest time excludes compilation. The guest test uses synthetic signed self-execution and is fee-exempt; it does not establish production caller authorization, sale execution, full-buy cycle headroom, fee settlement, block admission or deployment readiness. Expected guest panics in negative cases are checked rejections, not successful purchases.

## Format

Capacity is 131,072 approved addresses. Accounts, salts and hash values are 32 bytes. Reject duplicate accounts. Use independent cryptographically random salts and randomly selected positions across the full tree. Never use deterministic test randomness for real enrollment.

| Value | Encoding |
| --- | --- |
| Context | Version `1:u8`, namespace ID `[32]`, independent sale-policy ID `[32]`; 65 bytes |
| Policy | Context followed by expected root `[32]`; 97 bytes |
| Witness | Index `u32` little-endian, salt `[32]`, exactly 17 sibling hashes; 580 bytes |

The actual trader is an input from trusted execution state, not an identity claimed in the witness. Policy must also come from trusted state. Integrated pools bind the stored policy to the actual namespace and the canonical pool `AccountId` bytes as the sale-policy ID. The pool address does not depend on the root and is computed before constructing the tree.

Hash concatenated raw bytes with SHA-256. Domain tags below end with a single zero byte. Branch ordering follows the low bit of the current index, then shifts the index right after each step.

```text
leaf = SHA256("LP/allowlist/leaf/v1\0" || context || account || salt)
padding = SHA256("LP/allowlist/pad/v1\0" || context || index:u32 little-endian)
node = SHA256("LP/allowlist/node/v1\0" || context || left || right)
```

The builder is `src/merkle.ts`; the Rust boundary is `verify_for(policy, trader, proof)`. Both match the independent Python/hashlib [fixed vector](fixtures/merkle-vector.json). Exact lengths, version and index bounds are checked. The signed guest's Borsh vector adds a four-byte length prefix, for a 584-byte message.

A used witness reveals its index, salt and sibling hashes. Public paths can reveal structure and correlations; roster size and metadata secrecy are not guaranteed. Unused addresses require protected roster handling and secret independent salts. Do not publish the roster, unused salts, enrollment bundles or account keys. Membership is reusable; native authorization and nonces must bind each new transaction. There is no allowance or consumed-membership state.

## Checks and reproduction

Run from the repository root with the existing cached tools and dependencies. No automatic tool or artifact installation is part of these commands. LEZ remains pinned to `578ce58a71b79c49d48e97e51c520bf71f0252d9`, RISC Zero to 3.0.5 and the host build wrapper to `risc0-zkos-v1compat = 2.2.2`. The guest compiler is RISC Zero Rust 1.94.1; host commands use Rust 1.98.1. Scoped lockfiles preserve the dependency selections.

```sh
npm test --prefix packages/eligibility-proof-lite
cargo +1.98.1 test --manifest-path packages/eligibility-proof-lite/rust/Cargo.toml --locked --offline
cargo +1.98.1 clippy --manifest-path packages/eligibility-proof-lite/rust/Cargo.toml --all-targets --locked --offline -- -D warnings
PATH="$HOME/.cargo/bin:$PATH" CARGO_NET_OFFLINE=true RISC0_BUILD_LOCKED=1 cargo +1.98.1 test --manifest-path packages/eligibility-proof-lite/rust/execution/Cargo.toml --locked --offline -- --nocapture
```

The signed test defaults to the checked-in three-address fixture. It checks signed membership, copied paths under another account, altered messages with old signatures, unsigned execution, changed trusted context/root, malformed paths, native replay and fresh authorized reuse. Successful calls advance the native nonce without changing policy. Host and Node tests cover malformed encodings, duplicate enrollment and changed paths.

The completed measurement sequence was:

```sh
npm run benchmark --prefix packages/eligibility-proof-lite -- --members 3 --fixtures
# Run the default signed gate above; proceed only after it passes.
npm run benchmark --prefix packages/eligibility-proof-lite -- --members 100000 --fixtures
PATH="$HOME/.cargo/bin:$PATH" ELIGIBILITY_MEMBERS=100000 CARGO_NET_OFFLINE=true RISC0_BUILD_LOCKED=1 cargo +1.98.1 test --manifest-path packages/eligibility-proof-lite/rust/execution/Cargo.toml --locked --offline -- --nocapture
```

`--fixtures` replaces the selected synthetic public sample. It does not save the full roster or keys. Measurements go into ignored `results/merkle-*-node.json` and `results/merkle-*-lez.json`; the table above preserves this run. Each Node child has a 120-second timeout. Do not raise the gate or automatically optimize after a failure.

Five Node tests, three Rust verifier tests and the signed test at both sizes passed. Rust formatting and verifier strict lint passed. Npm warns about the existing `NPM_TOKEN` configuration name. The native dependency `block 0.1.6` reports future Rust incompatibility. Old installed Node modules may remain locally; removing the old declared dependencies is not a new audit result or production dependency approval.

See the [program integration plan](../../plans/allowlist-program-integration.md#public-execution-evidence) for separate signed public-buy evidence, including native `Execute` authority, replay rejection, immutable staged policy and complete-buy cycle measurements. Weighted execution uses a mock clock; normal weighted buys remain unsupported without a trusted timestamp provider. Wallet preparation, complete private routes, real proofs and deployment readiness remain open.
