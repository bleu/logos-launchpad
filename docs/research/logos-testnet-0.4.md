# LEZ testnet 0.4 delivery target

Researched: 2026-10-13. Source review only; no builds or live transactions. Open proposals describe intended changes, not released APIs.

## Public facts

The official launch article is dated 30 September 2026 and states that testnet 0.3 was released that day. It describes incremental account diffs as a breaking change, plus public transaction fees, asynchronous cross-zone messages, events, program upgrades and deployment sharding. Its wording is "smaller breaking changes possible in Testnet v0.4"; it gives no 0.4 release date. The LEZ repository has a 0.3.0 release, published on 30 September, and a 0.3.1 release, published on 11 October. [Article][article], [releases][releases].

RFP PR #214 was opened on 12 October 2026. It is OPEN, not a draft, and not merged at review time. For RFP-015 and RFP-016 it proposes one testnet 0.4 deployment milestone followed by mainnet, removes the separate 0.3 update requirement, and requires CU measurements on 0.4. Its exact delivery clause is "it is deployed and tested on LEZ testnet 0.4". The mainnet requirement stays. The current default-branch RFPs still require 0.2 deployment, 0.3 verification and mainnet; the PR does not establish a signed contract change. [PR #214][rfp-pr], [RFP-015][curve-rfp], [RFP-016][lbp-rfp].

The plan/apply model is already public in merged LEZ PR #912: planning schedules effects and child calls without reading account contents; application evaluates effects against current staged shard data. Open PR #935 proposes replacing it with actors, each identified by an account and program, and messages. It proposes atomic, depth-first Calls and Casts saved for later consumption, across public and private execution. This is a proposed removal of the separate plan step, not a released 0.4 interface. [PR #912][plan-apply], [PR #935][actors].

For LEZ Core, open PR #930 proposes shared wallet/sequencer FFI primitives and replacing their separate headers with `lez_core_ffi.h`. Open PR #948 depends on it and adds a read-only sequencer to the wallet; its integration-test work is incomplete. These support a pending API change, but do not define the final LEZ Core Module API or its release date. [PR #930][core-ffi], [PR #948][core-wallet].

## User-provided guidance

The supplied builder announcement calls 0.3 experimental, says 0.2 is being rebooted for existing programs, and makes 0.4 the target with its date "to be announced". It says 0.3 programs and the LEZ Core Module API will break, open lambda-Prizes target 0.4, SPEL focuses on 0.4 with best-effort 0.3 support, scaffold is under review, and the DEX will run on rebooted 0.2 before redeployment on 0.4. The direct RFP message asks this team not to target 0.3, confirms the intended milestone replacement, and promises dependency dates before a revised plan. These are user-provided team messages with no supplied public URL, sender identity or send date. Treat them as updated planning guidance, separate from the older article and from merged specifications; the public sources reviewed do not establish those release commitments.

## Impact on logos-launchpad

Confirm this dependency set with Logos: SPEL and scaffold; RFP-001 role authority for both launchpads; RFP-002 freeze authority for LBP; token and ATA programs; the on-chain clock for LBP; and atomic cross-program execution. RFP-015 makes DEX auto-graduation a soft requirement, dependent on RFP-004 and LP-0015. A graduation hook alone does not establish that those DEX dependencies are ready. [RFP-001][admin-rfp], [RFP-002][freeze-rfp], [RFP-015][curve-rfp], [RFP-016][lbp-rfp], [settlement decision](../adr/0020-settlement-and-graduation.md).

Existing tooling is not evidence of 0.4 compatibility. Official authority guides label their pins unreleased and show LEZ 0.2.4 dependencies. SPEL's reviewed framework manifest pins LEZ 0.2.5-rc2; `lez-programs` pins 0.2.4 and lists token and AMM deployments on that version. Its LP-0013 and LP-0014 prize pages are CLOSED, although the launchpad RFPs still call LP-0013 open. Scaffold's own package version 0.4.0 is a tool version, not proof of testnet 0.4 support. [Authority guides][admin-docs], [freeze guide][freeze-docs], [SPEL manifest][spel], [program manifest][programs], [deployments][deployments], [LP-0013][token-prize], [LP-0014][ata-prize], [scaffold manifest][scaffold].

Before fixing delivery dates, require a versioned compatible dependency set and tests for atomic buys, token custody, role authorization and the complete single-transaction private route. Atomic Calls in the actor proposal do not make deferred Casts one atomic transaction; open issue #974 says Casts do not currently propagate program authority. LBP also needs trusted, monotonic chain time: open issue #863 reports missing timestamp bounds on its cited September revisions. Neither issue establishes the final 0.4 behavior. Final APIs, migration steps, transaction limits, clock guarantees, dependency deployment dates and the 0.4 release date remain unconfirmed by the reviewed sources. [Actor proposal][actors], [PDA authorization issue][pda], [clock issue][clock], [LBP requirements][lbp-rfp], [private participation contract](../architecture/private-participation.md).

[article]: https://logos.co/media/article/logos-testnet-v03-live
[releases]: https://github.com/logos-blockchain/logos-execution-zone/releases
[rfp-pr]: https://github.com/logos-co/rfp/pull/214
[curve-rfp]: https://github.com/logos-co/rfp/blob/4b02880f023911439cdc6b2494c182500b5e40c3/RFPs/RFP-015-bonding-curve-launchpad.md
[lbp-rfp]: https://github.com/logos-co/rfp/blob/4b02880f023911439cdc6b2494c182500b5e40c3/RFPs/RFP-016-lbp-launchpad.md
[plan-apply]: https://github.com/logos-blockchain/logos-execution-zone/pull/912
[actors]: https://github.com/logos-blockchain/logos-execution-zone/pull/935
[core-ffi]: https://github.com/logos-blockchain/logos-execution-zone/pull/930
[core-wallet]: https://github.com/logos-blockchain/logos-execution-zone/pull/948
[admin-rfp]: https://github.com/logos-co/rfp/blob/4b02880f023911439cdc6b2494c182500b5e40c3/RFPs/RFP-001-admin-authority-lib.md
[freeze-rfp]: https://github.com/logos-co/rfp/blob/4b02880f023911439cdc6b2494c182500b5e40c3/RFPs/RFP-002-freeze-authority-lib.md
[admin-docs]: https://github.com/logos-co/logos-docs/blob/976a28bd2036578f13b628634eb04c035e86d1ed/docs/lez/extensions/admin-authority.md
[freeze-docs]: https://github.com/logos-co/logos-docs/blob/976a28bd2036578f13b628634eb04c035e86d1ed/docs/lez/extensions/freeze-authority.md
[spel]: https://github.com/logos-co/spel/blob/c6f73ce21e4805f8430f03afdbdedf0b7e479a5e/spel-framework/Cargo.toml
[programs]: https://github.com/logos-blockchain/lez-programs/blob/64c01210adecd6b557dbd021a3cc94553f243fd8/Cargo.toml
[deployments]: https://github.com/logos-blockchain/lez-programs/blob/64c01210adecd6b557dbd021a3cc94553f243fd8/DEPLOYMENTS.md
[token-prize]: https://github.com/logos-co/lambda-prize/blob/ea8cf61e77fa13ac52ce83b9a6751bed7e1d44e0/prizes/LP-0013.md
[ata-prize]: https://github.com/logos-co/lambda-prize/blob/ea8cf61e77fa13ac52ce83b9a6751bed7e1d44e0/prizes/LP-0014.md
[scaffold]: https://github.com/logos-co/scaffold/blob/d4b15ff7ad4e97fd987b9d1216f6e80cb750106e/Cargo.toml
[pda]: https://github.com/logos-blockchain/logos-execution-zone/issues/974
[clock]: https://github.com/logos-blockchain/logos-execution-zone/issues/863
