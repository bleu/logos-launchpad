# Platform probes

These checks test selected platform APIs. They do not implement production sale controls or a full LBP. The authority probe tests a fixed emergency authority with swaps-only freeze; this is isolated policy evidence, not root integration. The initial results below were checked on 2026-10-02; the candidate follow-up was checked on 2026-10-06.

## Versions

| Probe | Dependency | Exact revision |
| --- | --- | --- |
| Clock and root launchpad | LEZ | `a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a` |
| Authority | `spel-admin-authority` v0.1.4 | `479adf86f85b664073f359cfd2fd0087489f63a0` |
| Authority | `spel-freeze-authority` v0.1.5 | `a4743468bd3a87093931c48c965cc084aa4690c3` |
| Authority | SPEL | `d0bb659048c3fc6533f51c69e9f7f12382af6dd2` |
| Authority | LEZ v0.2.4 | `47eba256479f6f785acbd138834340703cd03401` |
| Events | Candidate LEZ | `c31a6bca60537ec08f595cf786f1ae6275ab7f43` |

These are three separate stacks, not one compatible production stack. Authority and events have isolated workspaces and lockfiles. Root platform pins are unchanged.

## Verified behavior

The [clock tests](../../crates/launchpad-client/tests/clock_probe.rs) pass five cases through real public execution. They update the actual clock program, read the canonical account in a guest, preserve it, observe later millisecond values, and reject missing, malformed or substituted input. LEZ defines `Timestamp` as Unix milliseconds. The guest reads the recorded clock value, which can differ from the transaction's supplied execution timestamp. The pinned sequencer updates `CLOCK_01_PROGRAM_ACCOUNT_ID` at block end. This does not prove live-node freshness or LBP weights and checkpoints.

The 13 [authority host tests](authority/tests/authority_policy.rs) pass and use real `AdminConfig` and `FreezeConfig` APIs behind a manual dispatcher. They check namespace addresses and owners, admin transfer and irreversible renunciation, fixed emergency control, namespace isolation, and swaps-only admission, including sale references created while frozen. Creator/custody references are preserved. These are admission checks with fixed creator fixtures, not NFT, token-transfer or sale-lifecycle tests. `#[freeze_authority(manual)]` still exposes transfer and renounce instructions, so this dispatcher uses public primitives without those emergency endpoints.

The authority guest builds and its [four guest tests](authority/tests/authority_guest.rs) pass. They inspect real `ProgramOutput` journals and call public `validate_execution`. Initialization returns a PDA claim with both components initialized; there is no separate component-claim instruction. Later calls use controlled claimed-owner fixtures and authorization flags. Transaction signatures, actual claim application and rollback through `V03State` remain unverified. This is not proof of atomic production namespace creation.

The [event tests](events/tests/event_roundtrip.rs) pass four cases. A real guest emits `Plan::event`; the public transition returns its emitter, eight-byte selector and Borsh payload. The probe decoder rejects unsupported schema versions and truncated payloads. A later chained failure discards the earlier event: direct execution returns an error, and metered validation reports execution work but returns no events. This departs from LP-0012 pre-failure retention; [ADR 0027](../../docs/adr/0027-events-and-history.md) now permits it. Signed execution also verifies nonce-only changes after metered failure and rejection of replay. The probe payload contains no private participant identifier; its version byte is a probe schema choice, not a platform wire-version guarantee.

Guest execution uses `RISC0_DEV_MODE=1`; these results are not cryptographic proof generation or deployed-node tests. The candidate probes use the licensed local admin component in `third-party/spel-admin-authority`; that dependency does not integrate namespace controls into the root programs.

## Commands

Run from the repository root. Checks used Rust 1.94.0, guest Rust 1.94.1 and the RISC0 3.0.5 tools on macOS arm64. Set `RISC0_SERVER_PATH` to the matching installed `r0vm` executable when it is absent from `PATH`.

```sh
cargo test --workspace --locked
cargo build --workspace --release --locked
RISC0_BUILD_LOCKED=1 cargo build --manifest-path methods/Cargo.toml --release --locked
RISC0_DEV_MODE=1 cargo test -p launchpad-client --release --features guest-tests --test clock_probe --test settlement --locked
cargo test --manifest-path packages/platform-probes/authority/Cargo.toml --locked --test authority_policy
RISC0_BUILD_LOCKED=1 cargo build --manifest-path packages/platform-probes/authority/methods/Cargo.toml --release --locked
RISC0_DEV_MODE=1 cargo test --manifest-path packages/platform-probes/authority/Cargo.toml --release --locked --features guest-tests --test authority_guest
RISC0_BUILD_LOCKED=1 cargo build --manifest-path packages/platform-probes/events/methods/Cargo.toml --release --locked
RISC0_BUILD_LOCKED=1 RISC0_DEV_MODE=1 cargo test --manifest-path packages/platform-probes/events/Cargo.toml --release --locked --features guest-tests
```

## Candidate compatibility follow-up

On 2026-10-06, all nine candidate tests passed with development receipts. The [NFT tests](events/tests/compatibility.rs) mint and transfer a real Authority NFT master, check the trusted token shard, and reject the former holder with full rollback. The [PDA tests](events/tests/pda_admission.rs) show that the unchanged legacy ownership check rejects a runtime-authorized PDA. The accepted candidate policy keeps address and seed checks and requires owner-program approval in the same transaction. The probe uses that evidence with the real administration library's authorized-signer branch, without inventing a legacy owner field. A direct call without owner approval is rejected and preserves state. This covers successor admission, not the full administration or emergency lifecycle.

Root adoption is blocked by client encoding. Installed scaffold `f919fd03b064a91a4f261093cf2b0b4f8150e27c` uses SPEL CLI `0cb7e0980535af619482cf1c823f4d394b3ebd61`. Its `collect-fees` dry run emits `08000000`; candidate Borsh encoding for that unit variant is `08`. Its submission path also lacks candidate shard selectors. A compatible tool revision or a separately approved client interface is needed before changing root pins. Candidate clock guards, further authority rejection cases, foreign-state checks, child-write rollback and freeze integration remain unverified.

## Open gaps

- The current root LEZ has no event field in `ProgramOutput`. The event candidate uses a different Plan/Apply interface; it cannot replace the root pin without migration work.
- The upstream admin component has no supported current-admin getter for `namespace-info`. The licensed local copy adds a getter; root integration remains separate work. The probe must not read private component bytes or store a second administrator field.
- The component's PDA-successor check requires a nondefault owner but does not require that owner to equal the candidate program. The probe adds that equality check at its dispatcher boundary. The positional initializer cannot reuse one input account for both roles because the runtime rejects duplicate input IDs; this is a probe input-layout limit, not a product rule.
- Event delivery for private transactions and deployed indexer ingestion are unverified. The candidate indexer's default capture filter retains no events; source and capture configuration are required. Its range query limit is 1,000 blocks. These probes establish no event byte/count limit or complete LP-0012 compatibility.

Platform sources: [current LEZ](https://github.com/logos-blockchain/logos-execution-zone/tree/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a), [event candidate](https://github.com/logos-blockchain/logos-execution-zone/tree/c31a6bca60537ec08f595cf786f1ae6275ab7f43), [admin component](https://github.com/mmlado/spel-admin-authority/tree/479adf86f85b664073f359cfd2fd0087489f63a0), [freeze component](https://github.com/mmlado/spel-freeze-authority/tree/a4743468bd3a87093931c48c965cc084aa4690c3).
