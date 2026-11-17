# LEZ 0.2 to development 0.4 migration check

Historical source check: 2026-10-20. No builds, proofs or transactions were run. Release and proposal status below is from that date, not a current platform survey. See the [workspace guide](../architecture/workspace-guide.md) for the selected revision, migration evidence and remaining checks. The earlier [delivery-target note](logos-testnet-0.4.md) records planning guidance.

## Release boundary

At review, the release list had LEZ `v0.3.1`, published on 11 October, and `v0.2.4-lez-legacy`, published on 13 October. It had no 0.4 release. The legacy release rebuilds LEZ 0.2.4 against Logos Blockchain and zone-sdk 0.3.1. Thus, a base-layer 0.3.1 version does not establish support for newer LEZ program interfaces. [Release list](https://github.com/logos-blockchain/logos-execution-zone/releases), [legacy release](https://github.com/logos-blockchain/logos-execution-zone/releases/tag/v0.2.4-lez-legacy).

## Confirmed changes

The official 0.3 announcement describes a breaking change from full account state outputs to incremental account changes. It also lists public transaction fees, events, program upgrades, deployment sharding and asynchronous messages between zones. These are changes already introduced on the path from 0.2; they must not all be described as new 0.4 features. [Official announcement](https://logos.co/media/article/logos-testnet-v03-live).

PR #912 merged on 26 September. It introduced separate planning and application steps: planning schedules effects without account contents; application uses current state. However, PR #935 merged on 14 October and replaces that model with transactional actors. Therefore, implementing the intermediate plan/apply interface would not match current development code. [PR #912](https://github.com/logos-blockchain/logos-execution-zone/pull/912), [PR #935](https://github.com/logos-blockchain/logos-execution-zone/pull/935).

An actor is state identified by an account and a program. The new handler receives its state, sender, authorization and message. It returns updated state, messages, events and validity limits. Synchronous messages run within the transaction; asynchronous messages can be consumed later. A later claim is not the same guarantee as the launchpad's selected single-transaction private route. Program calls, custody checks and authorization need new integration tests. [Merged actor interface](https://github.com/logos-blockchain/logos-execution-zone/blob/abf1350d42ff65b2b51a54cae39d486558b92e69/lee/state_machine/core/src/program/mod.rs).

The same source distinguishes a deployed program's account address from its bytecode image ID. PDA derivation uses the program account address. Deployment records, address derivation and clients therefore need explicit review; a dependency version change alone is insufficient. [Program header and PDA interfaces](https://github.com/logos-blockchain/logos-execution-zone/blob/abf1350d42ff65b2b51a54cae39d486558b92e69/lee/state_machine/core/src/program/mod.rs).

PR #930 closed without merging. Its replacement, PR #993, merged on 16 October and combines the wallet and sequencer headers into `core_ffi.h`. PR #948 also closed without merging. Open PR #996 proposes wallet startup through a `SequencerServiceFFI` pointer and still lists LEZ core and explorer module updates as future work. Module integration remains a moving dependency. [PR #993](https://github.com/logos-blockchain/logos-execution-zone/pull/993), [PR #996](https://github.com/logos-blockchain/logos-execution-zone/pull/996).

## Follow-up checks

The [platform replacement checklist](../architecture/workspace-guide.md#platform-replacement-checklist) tracks the active work. [ADR 0034](../adr/0034-lez-v04-migration.md) records the migration decision. This source survey does not establish a final 0.4 API or a compatible deployment set.

At review, private transaction fees and dry runs were proposed in PR #949. Issue #974 reported that asynchronous messages did not propagate PDA program authority; issues #956 and #957 concerned private replies. These are version-specific source leads, not findings about the selected project pin. [PR #949](https://github.com/logos-blockchain/logos-execution-zone/pull/949), [issue #974](https://github.com/logos-blockchain/logos-execution-zone/issues/974), [issue #956](https://github.com/logos-blockchain/logos-execution-zone/issues/956), [issue #957](https://github.com/logos-blockchain/logos-execution-zone/issues/957).

Issue #863 reports missing timestamp bounds on named September revisions. It does not establish later behavior. The separate [clock review](lez-clock-and-transaction-time.md) checks time APIs at named revisions. [Clock issue](https://github.com/logos-blockchain/logos-execution-zone/issues/863).
