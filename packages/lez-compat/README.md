# LEZ actor development base

This temporary development base targets LEZ actor revision `55c0bf37184728556e158a9ffa987e3e9b41732b`. It supplies token, ATA and role-authority handlers, thin guests and repository tools. It has no sale rules. The root application is migrated to the same revision. There is no second supported platform implementation. Nothing is deployed to testnet or production.

## Build requirements

| Requirement | Selected version |
| --- | --- |
| Host Rust and Cargo | `1.98.1` |
| RISC0 zkVM library, `cargo-risczero` and `r0vm` | `3.1.0` |
| RISC0 build library | `3.0.7` |
| RISC0 guest Rust | `1.94.1` |
| RISC0 C++ | `2024.1.5` |
| `rzup` command | `0.5.0` |
| zkOS wrapper | `2.2.3`, matching the LEZ program loader |

Install host Rust with `rustup toolchain install 1.98.1 --profile minimal --component rustfmt --component clippy`. Install and select the listed RISC0 components with `rzup`. Run the preflight below to check them. `RISC0_SERVER_PATH` can select an executor. Scaffold, SPEL CLI, a node, a sequencer and wallet state are not required. The shell checks use Python 3 and standard command-line tools.

The root, `methods/`, `methods/guest/` and `execution/` directories in this package have separate lockfiles. The core supports guest Rust 1.94; the host tools use Rust 1.98.1. Root launchpad and weighted-math lockfiles are outside this foundation check.

Host execution uses the selected LEZ revision's checked-in `artifacts/lee/privacy_preserving_circuit/privacy_preserving_circuit.bin`. Its SHA-256 is `b7e824e042a13ce2b8bb510eeee025089d0f7efb4467b58b81a858a7ab539b39`. The selected upstream `artifacts/lez/programs/clock.bin` has SHA-256 `631d75de50601d722eb13bb21fb97cf9fbf770849a6a36293789fbe60dca082a`. This lane does not build circuits or generate proofs. The upstream wallet's wider build uses the [upstream circuit set](https://github.com/logos-blockchain/logos-execution-zone/blob/55c0bf37184728556e158a9ffa987e3e9b41732b/flake.nix), including Logos blockchain circuits revision `2846ee7a4cfa24458bb8063412ab2e753b344d2f`; that wallet build is outside this package's evidence.

## Commands

Run from the repository root:

```sh
bash verify/check-builds.sh --actor --preflight
bash verify/check-pins.sh --actor
cargo +1.98.1 test --manifest-path packages/lez-compat/Cargo.toml --workspace --locked
RISC0_BUILD_LOCKED=1 cargo +1.98.1 build --manifest-path packages/lez-compat/methods/Cargo.toml --release --locked
bash verify/generate-idl.sh --actor
bash verify/check-idl.sh --actor
bash verify/check-builds.sh --actor
```

The final command selects repository-wide checks, including the root application and weighted math. `--actor` is an alias for the same platform. See the [workspace guide](../../docs/architecture/workspace-guide.md) for current check scope and evidence limits. Add `--execution` to include the optional token and seeded curve fixtures. The token fixture's separate command is:

```sh
RISC0_BUILD_LOCKED=1 cargo +1.98.1 test --manifest-path packages/lez-compat/execution/Cargo.toml --locked
```

The executor needs permission to start a local process and use local IPC. A filesystem sandbox can reject it with `Operation not permitted`. This is not a passing execution result. No network service or real-proof mode is needed. The fixture uses upstream fee-exempt test transactions; it does not establish wallet submission or network-fee behavior.

Foundation evidence below is separate from application results in the [workspace guide](../../docs/architecture/workspace-guide.md#execution-and-evidence-limits).

## Interfaces and addresses

The [actor interface](https://github.com/logos-blockchain/logos-execution-zone/blob/55c0bf37184728556e158a9ffa987e3e9b41732b/lee/state_machine/core/src/program/mod.rs) uses `ReceiveInput`, `Response` and Borsh messages. Both handlers accept the runtime's input and return `Result<Response, Error>`. The guest entry points turn an error into a transaction failure. Do not construct authorization or sender evidence from client message fields.

A program account address identifies a deployment. An image ID identifies its binary. Pass the program account address to PDA derivation and callbacks. The execution fixture alone uses `AccountId::from_builtin_program` to register a disposable test program. The generated `COMPAT_TOKEN_ID` and `COMPAT_AUTHORITY_ID` constants are image IDs, not deployment addresses. Guest binaries and IDs can depend on the build path; this foundation does not establish release identity across paths or operating systems.

`token::holding_address(program, owner, definition)` and `holding_seed` define canonical associated holdings. Each holding stores its definition, owner, token kind and balance. These holdings are public only; private purchase and authority execution is explicitly unsupported in the application helpers. A definition stores its name, kind, supply and optional metadata. Metadata is part of the definition actor state; there is no separate metadata actor in this temporary interface. Fungible amounts use checked `u128` arithmetic. Legacy role NFTs have one master unit; printing and general NFT products are outside this package.

Call `Create`, `Mint` or `Initialize` at the definition actor. `Create` and `Mint` require definition authorization. Call `Transfer`, `Burn` or `AssertNft` at the owner's actor for this token program. That actor requires the runtime's signer or PDA authorization, then calls the canonical holding. Credits and burns use synchronous messages. Another program can request `ReadState`; `read_reply` checks the expected token actor before decoding the upstream `StateReply`. `verify_nft_holder` checks a reply's token program, canonical holding, definition and owner. Token NFT support does not grant launchpad role rights.

Transactions that first reach an absent public holding need upstream `PublicAccountEvidence::Pda` with the token program address and `holding_seed`. Admission does not grant authorization. The execution fixture shows both admission evidence and signatures.

Call `authority::Bootstrap` at the authorized role account for a role that can renounce. Use `BootstrapTransferOnly` for curve creator and direct pool-owner roles, which cannot renounce. This policy is fixed at creation and survives transfer. The initial authority must approve in the same transaction. Call `Check`, `Transfer`, `TransferPda` or `Renounce` at the current authority's actor for this authority program. A key successor co-signs. A PDA successor's owner program handles `PdaRequest` with `approve_pda(input, trusted_authority_program)`, which sends the required seed grant. This replaces the obsolete whole-account owner check with actual owner-program participation. Candidate owner programs must integrate this helper; arbitrary owner programs are not assumed to support it. `verify` checks an authenticated role reply. Role state stays private to the authority module.

Use a separate account for each namespace administrator or sale creator role. Application code must derive and pin those role addresses and the trusted token and authority program addresses. A namespace role proof grants no permission for another role. Initialization, consent and transfer completion use synchronous messages with a checked epoch. A failed consent must reject the transaction. Renunciation cannot reopen bootstrap.

The generated [IDL](idl/compat.json) describes public message/state types, including internal messages that reject direct calls. `AccountId` and `PdaSeed` are 32 bytes. `Actor` is an account ID followed by a program account ID. Standard Borsh rules apply to enum tags, lengths, options and little-endian integers. The generator reads Rust declarations; the wire fixtures check independently specified bytes. Generate it again after a wire change. The CLI can encode a transfer:

```sh
cargo +1.98.1 run --manifest-path packages/lez-compat/Cargo.toml -p lez-compat-tools --locked -- encode-transfer 0101010101010101010101010101010101010101010101010101010101010101 0202020202020202020202020202020202020202020202020202020202020202 30
```

For other messages, serialize the public Rust message type with `borsh::to_vec`. The foundation does not generate replacement application SDKs or CLI commands.

## Evidence and limits

Local checks on 20 October 2026 used macOS on Apple silicon, the selected host toolchain and `r0vm` 3.0.5. Host tests cover creation, metadata, exact supply, minting, transfer, burn, canonical holdings, legacy NFT holder changes, role consent, transfer, renunciation, PDA approval and role/program isolation. Both guest entry points passed a locked release build.

The node-free test uses the upstream `V03State` executor. It creates 100 tokens, transfers 30, rejects a transfer of 71 from the remaining 70, and checks unchanged balances and nonce. It also rejects a transfer after the source debit when the destination is absent from the declared actors; the debit and nonce are rolled back. This is execution evidence for that disposable token fixture. It is not evidence for sale flows, authority guest execution, wallet submission or proofs.

With existing Cargo source and RISC0 tool caches, the first guest-workspace build took 43.9 seconds. The first execution-workspace compilation took 55.7 seconds. A warm execution test took about 0.7 seconds. These are local observations, not performance limits. The small executor setup was practical, so it is retained as an optional lane. Broader actor execution, service integration, wallet/module builds, private-route recovery and real proofs remain outside this foundation's evidence.

## Sources and replacement

The token and authority handlers are repository code that adapts reviewed behavior. They do not claim upstream wire or implementation equivalence. No third-party source files were copied into this package. Dependencies retain their licenses and notices. Reviewed references are the [legacy token types](https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lez/programs/token/core/src/lib.rs), [admin-authority lifecycle](https://github.com/mmlado/spel-admin-authority/tree/479adf86f85b664073f359cfd2fd0087489f63a0), actor [wrapped-token authorization](https://github.com/logos-blockchain/logos-execution-zone/blob/f94ca8a7550fa0012222b9fb5990dfe44c54a3dc/lez/programs/wrapped_token/src/main.rs) and [flash-swap execution tests](https://github.com/logos-blockchain/logos-execution-zone/blob/f94ca8a7550fa0012222b9fb5990dfe44c54a3dc/lee/state_machine/src/state/tests/flash_swap.rs).

Use the selected upstream [wallet](https://github.com/logos-blockchain/logos-execution-zone/blob/55c0bf37184728556e158a9ffa987e3e9b41732b/lez/wallet/src/lib.rs), [native fees](https://github.com/logos-blockchain/logos-execution-zone/blob/55c0bf37184728556e158a9ffa987e3e9b41732b/lez/programs/fee/core/src/lib.rs), [clock](https://github.com/logos-blockchain/logos-execution-zone/blob/55c0bf37184728556e158a9ffa987e3e9b41732b/lez/programs/clock/core/src/lib.rs) and [event](https://github.com/logos-blockchain/logos-execution-zone/blob/55c0bf37184728556e158a9ffa987e3e9b41732b/lee/state_machine/core/src/program/mod.rs) interfaces for application integration. This package does not replace them or supply an execution engine.

The workspace guide tracks replacement of [token/ATA support](../../docs/architecture/workspace-guide.md#upstream-token-ata) and [role authority](../../docs/architecture/workspace-guide.md#upstream-authority), plus the [official-release compatibility check](../../docs/architecture/workspace-guide.md#release-check). Keep repository build, pin and IDL tools where useful. See [ADR 0034](../../docs/adr/0034-lez-v04-migration.md) for the decision.

## Clock helpers

`lez_compat_core::clock` uses the selected upstream clock. It authenticates timestamp replies and can defer a response until an inclusive millisecond interval passes. The response must set its final state. Factory and pool handlers resume this pending state before they decode domain messages. Sale rules and inclusion-time bounds remain in those handlers. This is a wire adapter, not a replacement clock.
