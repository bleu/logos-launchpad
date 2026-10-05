# Source record

New research code uses the repository's [MIT license](../../LICENSE). Dependencies keep their upstream licenses. No DistributionX source, Circomlibjs implementation or Circomlibjs constants file was copied into this package. Reference vectors are literal outputs checked against independent sources; they are not an audit of the construction. This record covers the main cryptographic sources, not a complete release license review.

| Source | Pin | Use | Upstream license |
| --- | --- | --- | --- |
| [LEZ](https://github.com/logos-blockchain/logos-execution-zone/tree/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a) | `a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a` | Private account derivation; public account byte encoding; native test vectors. | MIT or Apache-2.0 |
| [k256](https://crates.io/crates/k256/0.13.4) | `0.13.4` | BIP340 account control and canonical secret-key representation. | Apache-2.0 or MIT |
| [RISC0 SDK](https://crates.io/crates/risc0-zkvm/3.0.5) | `3.0.5` | SHA-256 accelerator, execution, local proving and receipt verification. | Apache-2.0 |
| [light-poseidon](https://crates.io/crates/light-poseidon/0.4.0) | `0.4.0` | Canonical Circom BN254 Poseidon. | Apache-2.0 |
| [ark-bn254](https://crates.io/crates/ark-bn254/0.5.0), [ark-ff](https://crates.io/crates/ark-ff/0.5.0) | `0.5.0` | Poseidon field and canonical field encoding. | MIT or Apache-2.0 |
| [Circomlibjs reference](https://github.com/iden3/circomlibjs/tree/48b3ab37013c5ed21e9ff8a80a5b010795c97094) | `48b3ab37013c5ed21e9ff8a80a5b010795c97094` | Independent reference outputs only. Not a package dependency or source import. | GPL-3.0 in package metadata |
| [DistributionX](https://github.com/Timidan/dist-x/tree/c629c6c7bbd4b3aa56d4357f60817d49438e044b) | `c629c6c7bbd4b3aa56d4357f60817d49438e044b` | Design reference only; no source import or inherited audit claim. | Not relied on for code reuse |

Public account fixtures use [BIP340 test vector 0](https://github.com/bitcoin/bips/blob/master/bip-0340/test-vectors.csv), with secret scalar 3 and its published x-only public key. Public account and SHA tree/purchase/marker literals were computed separately with Python `hashlib` using the pinned LEZ domain and the research encodings. Private account literals come from the pinned LEZ helpers and their test vectors.

Poseidon literals were computed with a Python port of the pinned [reference algorithm](https://github.com/iden3/circomlibjs/blob/48b3ab37013c5ed21e9ff8a80a5b010795c97094/src/poseidon_reference.js) and [constants](https://github.com/iden3/circomlibjs/blob/48b3ab37013c5ed21e9ff8a80a5b010795c97094/src/poseidon_constants.json), outside this package. The reference was checked against the upstream `[1,2]` and `[1,2,3,4]` examples and a published full-byte example. The constants file SHA-256 is `354f31bbed9d9a8b884d8f2181e339cb425b6e3e4cc7240bd3dd22f0070f3f2c`. Tests retain leaf, padding, parent, root and full-width blinder outputs. They do not use `eligibility-core` or `light-poseidon` to calculate the expected values.

The direct public guest checker uses SDK receipt verification with verifier-owned parameters and an image pin. It was written here from the SDK interface. DistributionX's older LEZ pin, Ed25519 account binding, salt-based marker and journal-as-receipt client stub were not adopted. The [research report](../../docs/research/lbp-eligibility-proof.md) states what was tested and what remains open.
