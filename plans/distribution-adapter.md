# Distribution adapter implementation plan

Status: Implemented on `distribution-graduation-adapter`; local verification passed. This plan records the agreed scope. [ADR 0036](../docs/adr/0036-graduation-adapter-delivery.md) owns the final contract. The workspace guide records verification limits.

## Pull request base

Create a new branch named `distribution-graduation-adapter` from the current head of `origin/yvesfracari/shared-graduation-payouts`. Open the implementation pull request against that branch, not `main`. This is a separate change from [PR 49](https://github.com/bleu/logos-launchpad/pull/49). If the parent merges first, retarget after checking its final settlement contract.

Reviewed base: `e315593ea1f07eb05229e0a1fd5a851c0604e554`, checked on 2026-11-18. Read `CONTEXT.md`, ADR 0020 and the code on the target branch before work. The earlier local baseline has different burn, reserve, seed and manual-route behavior. Do not use those earlier rules in the implementation.

## Goal and boundary

Distribute the two asset amounts in a graduation request through one adapter. Support percentage splits with an automatic final remainder. The first version supports plain transfers only. Support bonding-curve, fixed-price and weighted sales through the existing kind-tagged request.

The factory already computes each adapter share and transfers it before the adapter runs. The adapter distributes the full custody balance of each authenticated sale asset, including prior direct deposits. Creator remainders, protocol fees, separate creator allocation and surplus in sale custody remain governed by the parent branch. A configured adapter cannot be bypassed by manual settlement. A zero-share adapter must still acknowledge the request.

The first version uses the existing token program and public holdings. It does not implement DEX operations, vault callbacks, vesting, redemption, stablecoin backing, staged execution, private custody or refunds. The liquidity adapter is deferred under ADR 0036. No new upstream dependency is needed for distribution.

## Configuration contract

Use a versioned configuration with `launch_rules` and `collateral_rules`. Each ordered rule has `bps` and `destination`.

| Field | Values |
| --- | --- |
| Percentage | `bps: u16`; each asset list must total exactly 10,000 basis points |
| Destination | `AccountId` of a fixed public owner |

Each destination owner is fixed in the hook configuration at sale creation. Derive its canonical holding for the selected asset. Namespace treasury changes and creator role transfers do not change these destinations. Do not add burn rules, vault actions or asset identifiers to the configuration. Asset identities come from the authenticated request.

After the factory payout, read and save the full custody balance for each authenticated asset. Apply each list to that saved balance. Require each balance to cover its request amount before distribution. Fixed payouts and separate remainder rules are excluded. Require each asset list to total exactly 10,000 basis points. Calculate each early payout as `floor(initial_custody_balance * bps / 10_000)` with full-range checked arithmetic. The final recipient receives the full unallocated saved balance, including rounding dust. If no amount remains, skip further transfers. A zero request still requires a custody balance check. A positive custody balance is distributed even when the request amount is zero. A zero custody balance performs no recipient preparation or transfers for that asset. The adapter must still acknowledge the request so the factory can complete graduation.

The accepted percentage base is the full custody balance for that asset, saved after the factory payout. For 101 units with 50%, 30% and 20% rules, pay 50, 30 and 21 units. Only the final payout uses the running remainder. Later deposits do not change the saved payout plan.

Use a maximum of eight rules per asset and 16 rules in total. Validate basis points in `0..=10_000`, version, list bounds, destination restrictions and an exact 10,000-basis-point total for each list. Reject an owner destination equal to adapter custody. Permit repeated external destinations; aggregate their expected credits when checking balances. Zero transfers need no token mutation.

ADR 0036 records the final contract. Client validation and the on-chain handler use the same pure validator and payout planner.

## 1. Add the adapter boundary

- Add `crates/distribution-adapter/` with configuration types, a pure payout planner and the actor handler. Reuse the existing request, result and initialization wire types from `factory-core` where they fit. Do not make the factory depend on the new adapter crate.
- Add `methods/guest/src/bin/distribution_adapter.rs` as a thin entry point. Update host and guest manifests, lockfiles and guest registration through the normal repository tools.
- Keep `graduation_adapter.rs` and its receipt-only handler for existing hook tests. Do not silently change that test adapter into the distribution program.
- Inspect the factory's adapter-state checks on the target branch. Preserve the externally consumed initialized-state and receipt contract, or define the smallest shared state view needed by both adapters. Do not require factory readers to decode an adapter's private pending state.
- Both factory paths decode `AdapterState` from the `ReadState` reply: curve in `factory-core/src/lib.rs`, fixed-price and weighted in `factory-core/src/lbp_setup.rs`. Use an explicit public projection. Keep the distribution adapter's stored state separate. Its idle `ReadState` reply must contain a Borsh-compatible `AdapterState` with the bound factory, accepted kind, receipt and no pending work. Add a narrow constructor in `factory-core` because its pending field is private. Reject external state reads during initialization or execution. Test both factory readers with this projection. Do not return the distribution adapter's stored bytes through the default `StateReply::from(input)` path.
- Reuse identity and initialization checks only where behavior matches. Avoid a broad factory refactor or a configurable execution framework.

## 2. Authenticate initialization and bind configuration

- Keep one external initialization call. Use factory program, namespace, launch salt and required sale kind as the identity inputs. Derive canonical adapter custody under the adapter deployment.
- Authenticate the factory state read. Check the sale kind, saved adapter program, launch definition and collateral definition. Read the saved hook configuration, validate it and save its hash with the identities.
- Do not add mutable recipient setters, separate activation, configuration replacement or caller-supplied token program addresses. Anyone may initialize only the exact saved configuration.
- Reject forged, repeated, malformed and wrong-phase replies. Initialization must fail atomically if the saved configuration is invalid.
- Expose a typed client preflight so invalid configuration is detected before sale creation. Initialization remains the on-chain check. On-chain sale creation treats configuration as opaque; this adapter does not claim to prevent every invalid sale from being created.

## 3. Prepare destinations and execute distribution

- Verify the authenticated factory caller, canonical sale identity, kind, asset definitions and saved configuration hash for every request. Reject a completed receipt or an in-progress competing request.
- Read authenticated definitions and custody holdings through the configured token program. Check fungible kind, ownership, asset identity and coverage of both request amounts. Save both full balances as the distribution amounts. Reject any custody balance below its request amount.
- Compute the full bounded payout plan before any asset mutation. Reject invalid percentage totals before moving funds.
- Derive each recipient's canonical holding. Use the existing permissionless holding preparation if it is safe and supported. Otherwise require pre-created holdings and expose the preparation in the client. Test the final selected behavior rather than leaving it implicit.
- The current token handler supports permissionless `Initialize { owner }` on an authenticated definition. It creates or checks the canonical holding through a zero credit. Use this operation for each unique recipient and asset before reading initial balances. Skip destinations with zero planned transfers. Declare holding actors and required holding PDA seeds in the transaction builder, using the same preparation pattern as the parent. Prove this path with a fresh recipient in guest execution. Keep preparation within the graduation transaction so later failure also rolls it back.
- Preserve the complete plan, expected replies and initial balances across synchronous continuations. Grant only the adapter's own required PDA seeds. Never grant factory seeds or trust a recipient value supplied at execution.
- Transfer exactly the planned amounts, including direct deposits present in the saved balances. Limit distribution to the two authenticated asset definitions. Other asset deposits are outside this version. Later deposits of the two sale assets use the accepted post-graduation distribution call.
- Authenticate completion and check destination credits and custody debits before callback success. Aggregate repeated destinations. Then store the receipt with the request and both actual distributed amounts. Send the existing full-request hash result to the factory. The public state projection must retain the original request receipt expected by the factory.
- Token transfers end without an adapter-specific completion reply. Use the actor response ordering already used by settlement: schedule mutations before authenticated verification reads. Confirm that ordering in a focused guest test before extending the full workflow. A recipient credit overflow is a real final-credit failure case; keep separate fault fixtures for faults that the fixed token program cannot otherwise produce.
- Ensure callback failure rolls back adapter mutations and transfers in the current transaction. Successful retries must never pay twice.

## Post-graduation distribution

The accepted design includes a permissionless `DistributeRemaining` call. It is available only after successful graduation. It cannot move funds before or during graduation. Anyone may submit it; the caller cannot select recipients, assets or percentages.

Gate the call on the adapter's committed graduation receipt and idle execution state. Set that receipt only after the first distribution is verified, before returning the factory callback. The final callback and factory completion must commit atomically with the receipt. A failed callback must roll back the receipt, so it cannot unlock this call. Prove this gate with real guest execution.

For each call, read and save the current full custody balances of the two bound sale assets. Reuse the same pure planner, recipient preparation, transfer and balance checks as graduation. Each asset list still totals exactly 100%. Round early payouts down and send the final recipient the remainder. Skip zero balances and zero transfers. An empty call succeeds without token mutations.

Keep the original graduation receipt and factory request hash unchanged. Do not repeat factory payouts, invoke the graduation callback or change the factory lifecycle. Keep each call atomic. Reject re-entry while a distribution is in progress. A failed call rolls back its transfers and adapter state; it can then be retried. A later successful call uses fresh balances and cannot pay already distributed funds again.

Record each completed distribution through a versioned adapter event with the sale identity, distributed totals and whether it was the graduation distribution or a later call. Keep event delivery and indexing outside the completion claim. The exact event wire form remains an implementation choice.

Each call rounds independently. Several small deposits distributed in separate calls can give the final recipient more rounding dust than one combined distribution. Callers can select when to submit a call, but cannot change its rules. This version has no minimum distribution size or waiting period.

## 4. Expose client, CLI and generated interfaces

- Add a typed `DistributeRemaining` builder for the bound adapter. Derive all accounts from its immutable configuration and asset identities. Expose this operation in the CLI.
- Add typed configuration encoding, validation, payout preview and recipient-account derivation to `launchpad-client`. Use the adapter's pure planner. CLI and later QML callers must not duplicate it.
- Keep saved hook bytes as the protocol boundary. Provide a small CLI command to encode a distribution configuration from a file and a preview using explicit full custody balances. A live preview must include expected factory payouts and is only an estimate until execution reads the balances. Keep initialization identity arguments shared with the existing adapter builder.
- Include all destination holding actors and required token/definition actors in the graduation transaction. The factory forwards no arbitrary account list; the client must declare actors needed by adapter continuations. Derive them from saved configuration, not new execution-time destinations.
- Regenerate the maintained on-chain schema and add independent Borsh fixtures for configuration, state and supported messages. Update the schema generator and drift checks for the new crate as required.
- Do not scaffold QML or shared-core placeholders. Keep the SDK as the common transaction boundary.

## 5. Verify the behavior

| Level | Required evidence |
| --- | --- |
| Pure planner | Exact percentage totals, percentages of the saved full custody balance, floor rounding, final-remainder handling, repeated recipients, zero amounts, invalid versions, list bounds and full `u128` amounts. Prove conservation: transfers equal the saved full custody balance for each asset. |
| Adapter handler | Authenticated initialization, immutable configuration, correct and wrong sale kinds, wrong factory or deployment, wrong assets, malformed replies, custody coverage, unexpected phases, replay and zero-amount completion. |
| Client and CLI | Typed encoding and preview agree with the planner. Transaction declarations cover destination preparation and completion reads. Execution cannot substitute recipients. Generated wire fixtures and drift checks pass. |
| Guest integration | Run the real distribution guest with factory, pool, role-authority and token guests for all three engines. Check adapter shares, creator remainders, current creator changes, fees and untouched surplus. Check exact transfer balances and unchanged token supply. |
| Failure and retry | Fail destination preparation, an intermediate transfer, a final recipient credit and the callback after prior actions. Check full rollback of current-transaction state, balances, supply, receipt, progress and signer nonce. Repair the external fault and retry once. Reject duplicate completion and manual bypass. |

Include adapter custody prefunding and direct deposits to prove that the full saved balance is distributed. Check that a balance below the request amount is rejected. Include two rules with the same recipient. Cover zero request shares with both zero and positive custody balances. Verify that zero custody balances cause no recipient preparation or transfers and still permit callback completion. Verify that positive balances distribute despite zero request amounts. Reject totals below or above 100%.

Reuse the parent's host and guest test harness. Keep weighted mock-clock execution explicit and distinct from upstream-clock evidence. Run relevant crate tests, SDK/CLI tests, maintained IDL checks, dependency-pin checks, guest builds and focused guest execution. Use the repository verification scripts for exact commands. Do not report proof, service, package-load or deployment readiness from local guest checks.

Test `DistributeRemaining` before initialization, before graduation, after a failed graduation callback and after successful graduation. Check authorization of continuations, exact balances, empty calls, repeated calls, new deposits, separate-call rounding and rollback after a later transfer failure. Verify that later calls preserve the original receipt and leave factory and pool state unchanged.

## 6. Update documentation and prepare review

- Update ADR 0036 with the final distribution contract and its implemented status only after checks pass. Link to ADR 0020 for factory payout rules.
- Add brief links from `CONTEXT.md` and the product overviews. Put new definitions in the glossary and the transfer flow in a sequence diagram.
- Update the workspace guide with reproducible commands, guest identity registration and evidence limits.
- Record examples for treasury splitting and rewards custody.
- Keep liquidity marked deferred. Do not cite private tracker metadata in public documentation.
- Open the implementation PR against the parent branch. Describe the result, initialization rules, tested rollback behavior and evidence limits. Exclude liquidity and vault guarantees from its completion claim.

## Completion criteria

The adapter can allocate both request assets for all three sale types. All value is transferred according to the immutable rules. It distributes direct deposits present in the saved custody balances. It permits later permissionless distributions only after graduation completes. It cannot change destinations, bypass the required route or complete graduation twice. The required local checks pass, generated interfaces match, and the new implementation PR targets PR 49's branch.

## Review questions

The agreed choices are:

1. Accepted: plain transfers only. Burns and destination vault actions are outside this version.
2. Accepted: percentages only, exactly 100% per asset list, early payouts rounded down, final recipient receives all remaining value, and zero transfers are skipped. Percentages use the full custody balance saved after the factory payout for that asset. Only the final payout uses the running remainder. Later deposits do not change the saved payout plan.
3. Accepted: all destination owners are fixed at sale creation. A treasury destination is a saved owner, not a live namespace-treasury lookup. Creator role changes do not change adapter destinations.
4. Use a maximum of eight rules per asset. Guest execution must check both full lists with fresh recipient holdings. Staged payouts are excluded.
5. Accepted: read and distribute the full custody balance of each authenticated sale asset after the factory payout. Direct deposits increase distribution. Keep request amounts as minimum coverage checks and record actual distribution amounts. Accepted: a separate permissionless call distributes later deposits only after graduation completes, using the saved assets, recipients and percentages.

The scope, percentage-only rules, full-balance percentage base and fixed destination owners are accepted. The post-graduation distribution call is accepted. Guest execution passed for eight rules per asset with fresh recipient holdings.
