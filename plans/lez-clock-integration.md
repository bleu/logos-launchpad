# Integrate LEZ timestamp reads

Status: Implemented on the branch with current main merged. PR #35 is part of main. Its poke and checkpoint paths use the upstream adapter. The combined full local gate passed.

Date: 2026-11-09

## Goal and source

Replace the temporary weighted-sale clock from [PR #33](https://github.com/bleu/logos-launchpad/pull/33) with the upstream clock. Include weighted buys and existing setup. Main now includes the poke and checkpoint feature from PR #35. Retain it and adapt its clock calls. PR #33 is merged. The [earlier plan](weighted-clock-read.md) records the mock implementation.

Use LEZ revision `f94ca8a7550fa0012222b9fb5990dfe44c54a3dc`. This is the checked head of `schouhy/single-clock-actor`. [Upstream PR #988](https://github.com/logos-blockchain/logos-execution-zone/pull/988) is open. Pin the commit, not the branch name. Check the official release later under [ADR 0034](../docs/adr/0034-lez-v04-migration.md).

The current [clock API](https://github.com/logos-blockchain/logos-execution-zone/blob/f94ca8a7550fa0012222b9fb5990dfe44c54a3dc/lez/programs/clock/core/src/lib.rs) provides `ReadTimestamp` and `Reply::Timestamp`. Values are Unix milliseconds. A tick occurs at the end of each block. Programs therefore read saved previous-block time. The branch changes the clock account and removes `AssertTimestamp`.

## Implemented behavior

- Request `ReadTimestamp { granularity: 1 }`. Keep full millisecond precision at the platform boundary. Convert to seconds only for weighted pricing and checkpoints.
- Read once per weighted buy. Poke also reads once per call. Accept only a timestamp reply from `clock_core::clock_actor()` during the expected pending phase.
- Retain participant price limits, recorded reserves, creator-borne fees, math limits, freeze checks and checkpoint rules.
- Retain the sale's start-inclusive, end-exclusive inclusion bounds. Saved clock time cannot replace them.
- Retain the current setup checks against saved time. Replace assertions with authenticated reads and local comparisons. Keep their inclusive or exclusive bounds exactly as implemented.
- Keep weighted math independent of the clock API. Use normal builds for real-clock execution.

These choices apply to this implementation; poke uses the same adapter. They do not change the accepted previous-block pricing policy in [ADR 0026](../docs/adr/0026-lbp-clock-and-poke.md).

## 1. Update the platform pin

Update all LEZ dependencies together. The six direct manifests are root `Cargo.toml`, both application method manifests, `packages/lez-compat/Cargo.toml`, its guest manifest and its execution manifest. Update `verify/actor-tools.env` and the pin-check fixtures.

Regenerate each affected root and nested lockfile. Include the compatibility methods lockfile. Search all remaining lockfiles for the old LEZ source. Do not force unrelated math dependencies to change. Check host Rust, guest Rust, RISC0 and zkOS requirements before retaining their current tool pins.

Review changes between the old pin and this revision. The update includes changes outside the clock, such as private fees and wallet interfaces. Fix affected existing callers and fixtures. Keep each required adaptation distinct from sale policy.

Verify the selected artifacts before using them:

| Artifact | SHA-256 at the selected revision |
| --- | --- |
| `artifacts/lez/programs/clock.bin` | `0375703a2405cc0403a5cf181777444b6432ffc8bcb028eeea2817d65540bd43` |
| `artifacts/lee/privacy_preserving_circuit/privacy_preserving_circuit.bin` | `741b7fdf815a660c39895719f019f2965a167fae761931fba60de16a95528881` |

Run a small timestamp-read test with the pinned clock artifact. A rebuilt copy of the source alone is not this check. Rebuild application and compatibility guests together after the pin change. Use disposable local state.

## 2. Replace the weighted pricing provider

Keep `crates/lbp-core/src/pricing_clock.rs` as a narrow adapter to the upstream API. Remove its test actor addresses, provisional request and reply types, availability switch and fake handler.

Decode upstream replies with fallible Borsh decoding. Check both sender addresses first. Reject malformed bytes and `Reply::BlockId`. Convert the accepted timestamp to seconds with integer division. Do not use the host clock, client time or cached pool time.

Update `trading.rs` to send the upstream request. The merged poke feature uses that adapter in `weighted_lifecycle.rs`. Retain their pending-phase checks. Remove `require_available()` and `ClockUnavailable`. Remove the error variant from maintained schemas if the generator exposes it. No old wire decoder is required for this undeployed application.

Keep the raw actor entry point. Upstream clock replies must reach the pending handler before the sale's own command decoder. Verify this for buys and poke.

## 3. Adapt setup and actor declarations

Update SDK declarations and fixtures to use the new canonical clock actor. Search all references to the three old clock accounts. Check factory creation, continuation, pool activation and guest setup fixtures.

Replace each `AssertTimestamp` call in factory setup and pool activation with an explicit pending read and comparison. Authenticate its reply and resume only the operation that requested it. Do not decode a clock reply as a token or namespace `StateReply`.

Preserve four-stage setup, creator authorization, start checks, setup deadlines and inclusion bounds. A failed clock reply must roll back earlier token, authority and activation effects in that transaction. Check both allocation release choices and setup retries. Check any fixed-price path reached through shared setup helpers.

Remove `curve_core::clock_assertion` after its last caller is replaced. Keep timestamp conversion and inclusion-bound helpers where their behavior still matches. Share platform decoding only where behavior is identical. Keep weighted setup and pricing policy in their own modules.

## 4. Audit all changes from PR #33

Use the merged PR diff as the removal checklist. Search the current tree too. Later changes added poke and checkpoint callers.

| Files changed by PR #33 | Planned treatment |
| --- | --- |
| `crates/lbp-core/build.rs` | Delete the mock-only build script. |
| `crates/lbp-core/src/pricing_clock.rs` | Replace the temporary protocol with the upstream adapter. |
| `crates/lbp-core/src/lib.rs`, `trading.rs` | Remove mock gating. Keep the buy interface and pricing flow. Adapt activation clock checks. |
| `crates/lbp-core/tests/clock_availability.rs` | Replace the opt-in check with a normal-build upstream request and reply check. |
| `crates/lbp-core/tests/weighted_trading.rs` | Remove the file-level mock cfg. Use upstream reply bytes and sender. Retain all pricing and lifecycle cases. |
| `methods/build.rs` | Remove the mock environment trigger. |
| `methods/guest/src/bin/mock_clock.rs` | Delete the fake guest. |
| `methods/tests/actor_sale.rs` | Use the real clock guest and `ClockAccountData`. Remove duplicate fake-clock accounts and guest registration. |
| `verify/check-dev.sh`, `verify/tests/check-dev.sh` | Run weighted host cases in the normal test pass. Remove the extra mock pass and its shell expectations. |
| `verify/check-weighted-clock.sh` | Keep a focused real-clock command. Remove the mock export and update its output. |
| `crates/launchpad-client/tests/idl_wire.rs`, `idl/launchpad.json` | Keep amount-only buys and rejection of obsolete pricing fields. Regenerate and check the IDL. Preserve checkpoint messages and events. |
| `CONTEXT.md`, ADR 0026, ADR index, workspace guide, sale overview | Update current behavior and evidence after tests pass. Retain historical mock evidence as history. |
| `plans/weighted-clock-read.md` | Mark the replacement when implemented. Link to the new evidence. Retain its historical purpose. |

Also update `weighted_lifecycle.rs`, factory setup tests, SDK actor builders, role-authority guest fixtures, pin tests and the development-base guide. These are outside the original mock diff.

Search for `LAUNCHPAD_MOCK_CLOCK`, `local_mock_clock`, `ClockUnavailable`, `mock_clock`, `MOCK_CLOCK_ELF`, provisional actor IDs and obsolete clock messages. No active runtime, build or test path may require them. Keep historical text clearly marked. Exclude local scratch probes and build output from this audit.

## 5. Verify behavior

Use the existing host and node-free guest suites. Host fixtures may supply upstream reply bytes. Guest integration tests must execute the selected upstream clock.

| Area | Required checks |
| --- | --- |
| Clock protocol | Correct actor; wrong account; wrong program; malformed reply; block-ID reply; unsolicited, duplicate and out-of-phase replies. |
| Pricing | Both buy forms; rising, falling and constant schedules; fractional weights; millisecond conversion; current reserves; price limits; fees and math limits. |
| Clock advances | Execute real `Tick` messages through the clock guest between transactions. Check that later buys and poke use the new time. Keep direct fixture seeding only for explicit stale-state cases. |
| Sale boundaries | Separate saved time from inclusion time. Test before start, at start, just before end and at end. Hold saved time before end while inclusion advances to end. Also reject invalid pricing time when inclusion is otherwise valid. |
| Setup | All four stages; start and end bounds; stale saved time; activation; both allocation choices; authorization and failed retries. |
| Checkpoints | Same-second reads emit no duplicate checkpoint. A later pricing second updates the checkpoint. Failed buys and poke leave it unchanged. |
| Rollback | Clock failure, price-limit failure and final token-credit failure preserve complete state and signer nonce. Include prior collateral and allocation transfers. |
| Other sales | Existing fixed-price and bonding-curve checks still pass on the new revision. |

A fixture that runs `Tick` through `V03State` checks guest state updates. It does not prove sequencer ordering or admission guards. Keep that distinction in the evidence record. Use the upstream end-of-block implementation as the source for ordering. Run a node-free sequencer fixture if needed to resolve an ordering concern; do not add a network service requirement to the default checks.

Run the existing pin and shell checks, host development gate, focused clock gate, IDL and wire checks, formatting, and `bash verify/check-builds.sh --execution`. Include all affected nested workspaces. Update locks once, then require locked verification. Record any blocked check and its reason.

## 6. Record private execution limits

The local native-token probe passed five tests on this revision. A changed public clock reply succeeds when the private return is unchanged. A return that changes from 100 to 99 fails with `CrossMessageMismatch`, although the minimum is 98. A fresh proof succeeds. Rejections preserve all fixture state.

Retain this result and its development-receipt limits in the verification record. Recheck it if the selected revision changes. The fixture uses native tokens and a small payout program. It does not prove the full launchpad private route.

The clock integration must not claim that slippage alone solves private return changes. Complete private routes, wallet recovery and custom-token private support remain separate work under [ADR 0018](../docs/adr/0018-private-participation.md). Public weighted buys and poke can use the real clock without completing that work.

## 7. Update design and evidence

Amend ADR 0034 for the new development pin. Amend ADR 0026 for the verified upstream API, granularity, units, previous-block semantics and private execution limit. Update ADR 0031 if its setup clock flow changes. Keep accepted policy distinct from tested behavior.

Update the short current-state claims in `CONTEXT.md`, the ADR index, sale overview, workspace guide and development-base guide. Replace current mock-only commands. Update artifact hashes and pin references where they describe the active development base. Preserve historical evidence and the later official-release check.

## Completion

The integration is complete when normal builds execute weighted buys with the pinned upstream clock, setup has no removed assertion calls, the mock runtime and build paths are gone, and the required local checks pass. Record remaining sequencer, private-route and real-proof evidence gaps explicitly. Do not describe a source check or development receipt as deployment readiness.

## Local result

The full local gate passed with unchanged lockfiles on 9 November 2026. The host development gate, IDL and wire checks, shell regressions, strict host lints and formatting also passed. See the [workspace evidence](../docs/architecture/workspace-guide.md#weighted-pricing-clock). The initial run preceded the merge of PR #35. The combined gate then passed all 28 sale guest tests and 31 curve, role-authority and settlement guest tests with unchanged lockfiles. See the workspace evidence for scope and limits.
