# Namespace, custody, and fee contract

Status: M0 specification draft. Accepted design, PoC behavior, and platform evidence are separate below. Final design review and platform verification remain open.

This contract uses [CONTEXT.md](../../CONTEXT.md). It applies to curve and LBP sales without giving them one shared pricing model or lifecycle. The [ADRs](../adr/README.md) record decisions and their reasons. This document defines the shared rules, required checks, balance examples, and acceptance cases.

## Decisions and implementation status

| Area | Required behavior | Current evidence and gap |
| --- | --- | --- |
| Namespace | Permissionless creation, stable identity, immutable sale binding, and isolated accounts, listings, and history | Curve PoC account derivation and sale binding exist. Shared physical namespace schema and cross-factory account ownership are not selected. |
| Administration | One namespace `AccountId` administrator; transfer needs current-admin authorization and successor acceptance; renunciation is permanent | [ADR 0006](../adr/0006-namespaces-and-authority.md) selects the administration module. Its current-admin getter and manual-dispatch integration remain unverified. The PoC still uses NFT administration and direct admin replacement. |
| Setting changes | Fee-only and treasury-only updates, separate from admin transfer | Accepted design. The PoC's combined `UpdateConfig` operation still replaces the whole configuration. Separate production operations are not built. |
| Namespace fee range | Integer rates from 0 through 9,999 basis points at creation and fee update | Curve public-handler conformance tests cover creation and update. Targeted host tests passed for this correction; this is not guest or production administration evidence. |
| Curve accounting | Live collateral fees, separate fee counters, current-treasury collection, and atomic collection with reserve withdrawal | Pool math and curve handlers exist, with host tests. Pinned-platform custody and settlement evidence is still required. |
| LBP accounting | Creator-borne collateral fee; full buy input enters pricing; live rate and anytime collection | [ADR 0016](../adr/0016-lbp-live-fees.md) accepts this internally. External status is `To be reviewed by Logos team`. No LBP factory or pool program exists. |
| Events and errors | Compatible events, stable semantic error categories, and no persistent private participant identifiers | Fields below are specification requirements. The PoC does not publish these required events or provide the listed semantic error mapping. Platform encoding and client mapping remain unverified. |

## Namespace lifecycle and isolation

Any caller can create a new namespace when its initialization authorizations and account checks are valid. No global administrator approves namespace creation. Creation must establish the namespace and its initial administration state atomically. It must not leave an empty administration state that another caller can claim.

A namespace has one logical identity, current administrator, fee rate, and treasury. Each factory binds a sale to that namespace at creation. The binding never changes, including after administrator or treasury changes. A sale must not read the rate from one namespace and the treasury from another.

Namespace-scoped accounts must be derived and checked against the sale's stored namespace. Clients and backend reads must also scope listings, history, and monitoring by namespace. A UI filter does not replace a program account check. This contract does not select a registry, physical schema, cross-factory owner, or PDA layout for the future shared namespace state.

Namespace creation supplies the initial fee and treasury explicitly. One basis point is 0.01%. The accepted fee range is 0 through 9,999 basis points, or 0% through 99.99%. Reject 10,000 and larger values. Keep 10,000 as the arithmetic denominator. Reject a default treasury identity; supported treasury account forms still need platform verification.

### Administration operations

| Operation | Required authority | Fields it can change | Fields it must preserve |
| --- | --- | --- | --- |
| Change fee rate | Current namespace administrator | Fee rate | Namespace identity, treasury, administrator, sale bindings, and existing fee counters |
| Change treasury | Current namespace administrator | Treasury | Namespace identity, fee rate, administrator, sale bindings, and existing fee counters |
| Transfer administration | Current administrator plus successor acceptance under the selected module's rules | Current administrator | Namespace identity, fee rate, treasury, sale bindings, creator authority, and custody |
| Renounce administration | Current namespace administrator | Remove administrator power permanently | Namespace identity, last fee rate and treasury, sale bindings, creator authority, and custody |

Transfer must use the selected administration lifecycle. Do not add a separate pending-transfer model, decode private module storage, or keep a second administrator field. A supported current-administrator read is required for `namespace-info` and operation checks.

After renunciation, no caller can change administrative settings or restore administration. Renunciation does not itself disable trading, fee collection, or permitted settlement. These actions still apply their own account and lifecycle checks.

### Sale inputs and live settings

Sale-to-namespace binding, token roles, typed sale creation inputs, and the stable creator-role identity are immutable. Current creator NFT ownership can change without changing those identities. A curve factory owns its curve allocation and settlement policy; an independent LBP factory owns LBP creation and sale policy. Their full input sets belong to their mechanism specifications.

The namespace fee rate and treasury are live settings until administration is renounced. Execution reads authoritative current state. Indexed state and displayed quotes can lag. A changed rate affects later trades, not the fee amount already accrued. A changed treasury affects later collection, including collection of previously accrued fees.

## Authority and custody

| Action | Who can request it | Required checks |
| --- | --- | --- |
| Create a namespace | Any caller | Valid initialization authorization; unused namespace state; atomic admin initialization; valid fee and treasury |
| Change settings or renounce | Current namespace administrator | Supported admin check, correct namespace state, and operation-specific inputs |
| Transfer administration | Current administrator and accepting successor | Selected module's transfer rules; unchanged namespace and custody |
| Create or settle a sale | Creator authority holder | Factory checks, immutable sale binding, current creator authority, and permitted lifecycle stage |
| Trade | Participant | Valid participant authorization, token accounts, namespace binding, current rate, quote limits, and mechanism state |
| Collect fees | Any caller | Valid sale and namespace state, fee-covering custody, and current-treasury collateral ATA; no administrator or creator approval |
| Pause or freeze an LBP | Authority remains open | Compatible platform controls and reviewed instruction gates; namespace administration alone grants no such right |

An Authority NFT can grant a creator or direct pool-owner role. Its definition is the stable role identity; the current holder proves control. It does not administer a namespace under the selected production design. Creator rights do not grant namespace administration. Namespace administration does not grant creator withdrawals or token custody.

Use the platform token and custody mechanisms described in [ADR 0028](../adr/0028-token-custody-and-freeze.md). Keep token/mint authority, ATA ownership, namespace administration, and pause/freeze rights separate. Curve factory-created pools remain under factory custody. Do not copy that custody or staged creation model into an LBP without its own evidence.

### Logical accounts and access checks

These are logical account roles, not a selected physical layout.

| Account role | Required access and validation |
| --- | --- |
| Namespace state | Read the authoritative administrator, rate, and treasury. Write only the field permitted by the authorized administration operation. Validate its identity and supported program owner. |
| Sale or pool state | Read the immutable namespace and token roles. Validate its derivation and program owner. Write trading, fee, or settlement state only through its mechanism operations. |
| Creator authority | Validate the current authority holder for factory creation or settlement. A namespace administrator cannot replace this check. |
| Participant accounts | Validate authorization and the input/output token accounts required by the trade. The SDK constructs private routes; this contract does not prove wallet support. |
| Collateral vault | Validate the expected owner, collateral token definition, ATA derivation, token-program data, and available balance. Only the supported program custody path can move its tokens. |
| Current treasury collateral ATA | Derive from the current treasury and sale collateral definition. Validate owner, token definition, address, and token-program data. The caller cannot provide an alternative destination. |
| Allocation and proceeds accounts | Validate the mechanism's claims and payout destinations. Keep curve `D`, reserved `R`, and separate creator allocations distinct. |
| Clock | Use the supported trusted chain clock wherever timing is required. Fee collection does not acquire an LBP end-time gate from a creator withdrawal check. |

The client can prepare a missing treasury ATA through the supported ATA creation path. The program must reject a missing or mismatched required account; it must not redirect fees. If treasury changes after preparation, the caller must refresh current state and prepare the new destination before retrying. Account aliases, such as a treasury that is also the creator, must follow the platform's unique-account rules without counting a fee or payout twice.

Custody must cover all remaining claims, counted once. For a curve collateral vault, `physical collateral >= real collateral reserve + outstanding fees`. Surplus deposits do not automatically become protocol fees or creator proceeds. Fees cannot back swap output or reserve withdrawal.

For LBP, physical collateral must cover outstanding fees and all remaining permitted collateral payouts. Pricing state is not proof that tokens exist in the vault. Input bounds, initial collateral ownership, and the pricing/custody model after early fee collection require the independent LBP specification and platform evidence.

Pause and freeze integration must preserve required settlement and recovery paths unless a reviewed policy explicitly gates them. No automatic global freeze policy is selected. See [ADR 0028](../adr/0028-token-custody-and-freeze.md).

## Fee accounting

Amounts are integer base units of each token. A base unit is its smallest unit. Protocol fees are always in collateral: token1 for a curve factory sale. Track outstanding fees and cumulative collected fees independently from reserves and creator claims. Do not infer fees from the entire vault balance.

For a collateral amount `amount` and rate `r`, the fee is `ceil(amount * r / 10_000)`, with checked arithmetic. The rate is read at execution. Collection does not recompute an accrued fee at a newer rate.

| Mechanism | Fee base | Effect on trade pricing and payout |
| --- | --- | --- |
| Curve buy | Gross collateral input | Deduct the fee before pricing. Only effective input increases the real collateral reserve; the fee increases the outstanding counter. |
| Curve sell | Gross collateral output | Quote gross collateral first. Deduct the fee from the trader's output. Decrease the real reserve by gross output and accrue the withheld fee. |
| LBP buy | Full collateral input | Use full input for buyer pricing. Accrue a creator-borne claim on collateral proceeds. Do not use the curve's net-input deduction. |

The LBP proposal specifies buys. This contract does not add LBP sells or sell-fee accounting. Weighted and fixed-price output rules, validated input bounds, and the treatment of initial collateral belong to its mechanism specification.

A valid rate does not guarantee a valid trade. For example, a 1-unit curve buy at 1 basis point has a 1-unit rounded fee and no effective input. Curve input/output and slippage checks must still reject invalid trades. The LBP must prove its own rounding and collateral coverage without underflowing creator claims.

The LBP RFP baseline snapshots the fee at creation and waits until the scheduled end for collection. The accepted internal design uses the live rate and permits collection at any time. Both departures remain `To be reviewed by Logos team`. Creator withdrawal still waits for the original end timestamp. Pause does not move that timestamp, and no early closure can bypass it. See [ADR 0016](../adr/0016-lbp-live-fees.md), [ADR 0025](../adr/0025-sale-closing.md), and [ADR 0026](../adr/0026-lbp-clock-and-poke.md).

## Collection and settlement

Let `A` be outstanding fees and `C` cumulative collected fees. A successful single-sale collection transfers exactly `A` collateral to the current treasury, sets `A` to zero, and increases `C` by the transferred amount. It does not change the original fee rate used, charge another protocol fee, or transfer the trading reserve as fees.

If `A` is zero, collection transfers nothing and leaves `C` unchanged. Repeated collection cannot pay the same fees twice. Required account checks still apply. New trades can create new fees between retries; collecting those is not a second collection of the old amount.

Curve collection is available during and after trading. Internally accepted LBP collection is also available at any time, including during pause and before the scheduled end. This does not permit early creator withdrawal. Collection never requires a current administrator signature, including after renunciation.

Terminal settlement collects remaining fees atomically with the relevant reserve or proceeds transfer. If an account check, counter update, or token call in that stage fails, no part of that stage commits. Earlier completed factory stages remain completed. The whole resumable factory workflow is not one transaction. Payout retries must not repeat a completed payout, burn, or fee transfer. See [ADR 0020](../adr/0020-settlement-and-graduation.md).

### Namespace-batched client behavior

A batch selects sales within one namespace and checks each sale's program owner, binding, and collateral definition. It resolves current treasury state for each sale. A client submits one independently atomic collection transaction per sale in sequence. A failure does not undo earlier confirmed transactions.

The existing [`collect_pool_fees`](../../crates/launchpad-client/src/lib.rs) helper follows that sequential curve-pool model. It stops at the first error and returns hashes only if the call completes successfully. It does not return a complete per-sale report after partial failure. A production caller must retain submission/confirmation progress, report the failed sale, and offer a safe retry. That reporting interface remains client work.

A retry must refresh account state and treasury. It can retry an uncertain transaction after checking its receipt, or repeat collection safely. Previously collected fees remain collected. An operation receipt or indexed history can confirm earlier work; indexed state never authorizes a new transfer.

## Worked balances

These examples use literal amounts from [pool accounting tests](../../crates/pool/src/lib.rs), named `protocol_fee_is_collateral_input_on_buy_and_collateral_output_on_sell`. They show accounting, not platform settlement proof. The buy and sell start from separate pools: launch reserve 800, real collateral 100, virtual launch reserve 1,000, virtual collateral 100, creation invariant `k = 100,000`, and no fees.

### Curve buy

A participant buys with 25 collateral at 1,000 basis points. The fee is 3, effective input is 22, and token output is 180. The invariant remains the creation invariant; the fee does not enter pricing reserves.

| Step | Real launch reserve | Real collateral reserve | Physical collateral | Outstanding fees | Collected fees |
| --- | ---: | ---: | ---: | ---: | ---: |
| Before buy | 800 | 100 | 100 | 0 | 0 |
| After buy | 620 | 122 | 125 | 3 | 0 |
| After collection | 620 | 122 | 122 | 0 | 3 |
| After repeated collection | 620 | 122 | 122 | 0 | 3 |

The treasury receives 3. The remaining 122 collateral backs the reserve. A separate surplus donation would not increase either fee counter.

### Curve sell

From the separate initial pool, a participant sells 250 launch tokens at 1,000 basis points. Gross collateral output is 20, the fee is 2, and the participant receives 18.

| Step | Real launch reserve | Real collateral reserve | Physical collateral | Outstanding fees | Collected fees |
| --- | ---: | ---: | ---: | ---: | ---: |
| Before sell | 800 | 100 | 100 | 0 | 0 |
| After sell | 1,050 | 80 | 82 | 2 | 0 |
| After collection | 1,050 | 80 | 80 | 0 | 2 |

The real collateral reserve falls by the full 20. Only 18 leaves the vault for the participant; 2 stays as a fee claim until collection. Settlement pays the permitted remaining reserve after collecting outstanding fees; it does not apply a new trade fee to that payout.

### LBP buys with early collection

This is an illustrative full-input accounting model. Let `P` be the full-input pricing collateral balance and `V` physical collateral. Both start at 100. The initial collateral's ownership and payout are not assigned here. Token outputs are premises supplied by a valid quote; this table does not qualify weighted math or select its production reserve representation.

| Step | Rate used | Buyer collateral input | P | V | Outstanding fees | Collected fees | Treasury receipt |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Initial state | - | 0 | 100 | 100 | 0 | 0 | - |
| Buy | 1,000 bps | 25 | 125 | 125 | 3 | 0 | - |
| Change treasury A to B | - | 0 | 125 | 125 | 3 | 0 | - |
| Collect | - | 0 | 125 | 122 | 0 | 3 | B receives 3 |
| Change fee to 2,000 bps | - | 0 | 125 | 122 | 0 | 3 | - |
| Buy | 2,000 bps | 40 | 165 | 162 | 8 | 3 | - |
| Terminal collection | - | 0 | 165 | 154 | 0 | 11 | B receives 8 |
| Repeated collection | - | 0 | 165 | 154 | 0 | 11 | No transfer |

The 65 new collateral units enter buyer pricing in full. Fees total 11, leaving a net addition of 54 after collection. The remaining physical 154 includes the starting 100. It must not all be labelled creator proceeds without the initial-collateral and payout policy. Creator settlement must prove the permitted payout and remaining coverage. It must not deduct the same 11 again.

## Event requirements

Use supported LP-0012 events and the existing indexer and explorer tools. The following fields are semantic requirements. Their encoding, supported types, event locations, and compatibility remain open. Events report committed effects; clients must not treat a failed transaction as a committed event.

| Event purpose | Minimum information |
| --- | --- |
| Namespace creation | Namespace identity, initial administrator, initial rate, treasury, and chain event location |
| Fee update | Namespace identity, previous and new rate, and chain event location |
| Treasury update | Namespace identity, previous and new treasury, and chain event location |
| Admin transfer or renunciation | Namespace identity, supported lifecycle action, affected administrator identities, and chain event location |
| Trade | Namespace, sale, mechanism, token roles, actual input/output, fee base, rate used, fee amount, relevant pricing/net amount, fee counters, and chain event location |
| Collection | Namespace, sale, collateral definition, actual collected amount, actual current treasury, resulting fee counters, and chain event location |
| Settlement stage | Namespace, sale, mechanism, completed stage, collected remainder, permitted payout or burn amounts, resulting state, and chain event location |

A zero collection result must report no transfer and no increase in collected fees. Event mapping must preserve that distinction. No trade event contains a persistent private participant identifier. Public pool state, amounts, and trade effects remain visible. These requirements do not establish a stronger privacy claim or a successful combined private transaction.

Listings and history use namespace-scoped backend module reads. Indexed results can lag. Use current chain state for quotes, authority checks, and transfers. See [ADR 0027](../adr/0027-events-and-history.md).

## Stable error categories

These names identify semantic categories for program/client agreement. They are not implemented numeric platform codes or a generated IDL enum. The supported transport mapping must be checked before final interface acceptance. Clients must give an actionable cause without exposing persistent private participant identifiers.

| Category | Cause and required action |
| --- | --- |
| `Unauthorized` | Required authorization or authority-holder proof is missing or wrong. Use the correct current authority. |
| `AdministrationRenounced` | A caller attempts administration after permanent renunciation. Do not offer recovery through a new admin claim. |
| `NamespaceMismatch` | Sale binding or supplied namespace state does not match. Reload the sale's immutable namespace. |
| `AccountMismatch` | Wrong PDA, owner, token definition, token-program data, account alias layout, or trusted clock. Supply the validated accounts. |
| `InvalidFeeRate` | Fee rate is outside 0 through 9,999 basis points or cannot be represented. Correct the setting. |
| `InvalidTreasury` | Default or unsupported treasury identity, or mismatched treasury collateral ATA. Refresh the current destination and prepare its ATA. |
| `InvalidSaleState` | Funding, trading, closing, or settlement stage does not permit the requested action. Resume only a permitted stage. |
| `InvalidTradeAmount` | Input/output bounds or rounding make the trade invalid. Obtain a new valid quote. |
| `SlippageExceeded` | Execution violates the participant's explicit quote limit. Review a fresh quote before resubmitting. |
| `ArithmeticFailure` | Checked arithmetic, representation, or counter accumulation cannot safely complete. Reject without partial state changes. |
| `InsufficientCustody` | Actual tokens cannot cover fees, output, or remaining permitted claims. Do not spend a fee claim as liquidity. |

Zero accrued fees are a successful no-transfer result, not an error. More than one check can fail for an invalid request; callers must not depend on the order of validation. No caller can turn a failure into permission to choose another treasury.

## Acceptance scenarios and evidence

These cases are required for both mechanisms where applicable. Existing tests establish only the behavior and execution lane they actually exercise. Test names below are source evidence, not a claim that every optional lane passed in this change.

| Case | Expected result | Current evidence or required work |
| --- | --- | --- |
| Create two namespaces | Distinct state, custody selection, settings, listings, and history | Curve host test `permissionless_namespaces_have_independent_addresses_fees_and_updates` in [curve tests](../../crates/curve-core/src/tests.rs). LBP and indexed reads remain required. |
| Use a foreign namespace | Reject sale/config mismatch and cross-namespace admin access | Curve tests `swap_rejects_another_namespaces_config` and `namespace_admin_cannot_modify_another_namespace`. |
| Create or update a fee | Accept 0 and 9,999; reject 10,000 and higher without committing a setting | Public host test `namespace_fee_rate_is_below_one_hundred_percent_on_creation_and_update`; targeted host verification passed. |
| Lower a legacy fee | A valid legacy 10,000-bps state fixture can be lowered to a valid rate | Public host test `namespace_fee_can_be_lowered_from_a_legacy_hundred_percent_rate`; this is not live upgrade evidence. |
| Change one setting | Fee change preserves treasury/admin; treasury change preserves fee/admin | Required production-operation tests. The current combined PoC operation does not prove this. |
| Transfer administration | Successor acceptance is required; old admin loses power; namespace, creator authority, and custody stay unchanged | Selected module integration is unverified. PoC direct-admin rotation tests are not acceptance evidence for the selected lifecycle. |
| Renounce | Admin changes remain forbidden; permitted trading, collection, and settlement continue | Curve test `transfer_then_renounce_preserves_trading_but_permanently_removes_admin_power` covers the PoC authority model. Production module and LBP verification remain required. |
| Change rate between quote and execution | Curve uses the execution rate and enforces limits; LBP uses full input and records the actual creator-borne fee | Curve test `execution_fee_update_respects_buy_and_sell_slippage`; LBP cases remain required. |
| Change treasury with fees pending | All pending fees go to the current treasury; no caller-selected destination | Curve test `permissionless_collection_uses_live_treasury_and_preserves_principal`; LBP cases remain required. |
| Collect zero, then repeat | No transfer and no counter increase; new fees can later be collected once | Existing pool/curve collection tests; LBP cases remain required. |
| Donate extra collateral | Fee counters do not increase from a raw vault surplus | Must verify explicit fee accrual and the mechanism's surplus policy. A surplus is not permission to collect it as a fee. |
| Underfund custody or supply a wrong ATA | Reject before committing a transfer or fee-counter change | Curve test `accrued_fees_cannot_back_a_sell_or_withdrawal`; pinned token/ATA and atomic failure evidence remain required. |
| Settle a closed sale | Collect only the remainder with the relevant payout, with no second fee or repeated payout | Curve handler and guest-lane test `fee_accrual_collection_and_withdrawal_preserve_vault_liabilities` in [settlement tests](../../crates/launchpad-client/tests/settlement.rs). Guest-lane execution must be recorded separately. |
| Fail a terminal token call | The entire relevant stage rolls back; earlier committed stages remain committed | Required pinned-platform custody and settlement test. Host poststate checks alone do not prove chain rollback. |
| Fail the second sale in a client batch | First confirmed collection remains committed; failed and uncertain sales can be refreshed and retried safely | Existing sequential helper defines stop-on-error behavior. Per-sale progress/reporting and integration tests remain client work. |
| Pause an LBP before its end | Clock and original end stay unchanged; internal-policy collection remains available; creator withdrawal stays gated | LBP controls, compatible clock reads, and authority gates remain unimplemented/unverified. |
| Index a private-route trade | Namespace history works without a persistent private participant identifier; public effects remain visible | Event encoding, wallet/private-route integration, and privacy review remain required. |

## Open evidence and final review

The specification can be drafted while platform tests run. Final acceptance requires executable evidence on the pinned target release:

- Token authority, custody ownership, ATA derivation, supported settlement tail calls, rejection of unauthorized or mismatched accounts, and atomic failure behavior.
- Compatible clock reads and event emission/decoding, with reviewed field types and privacy behavior.
- Supported current-administrator access, atomic administration initialization, manual-dispatch integration, transfer/renunciation, canonical namespace checks, and isolation. The reviewed administration module lacks the required public getter; do not treat the stopped compatibility probe as passing.
- Explicit pause/freeze instruction gates. Administration support alone does not prove custody or freeze support.
- Independent LBP state, pricing/input bounds, rounding, seed ownership, and collateral coverage during early fee collection. Weighted math is unaudited and is not an LBP program.
- Logos review of the live LBP rate and anytime-collection departures, plus final product/design review that checks this contract against the mechanism specifications, client contracts, and platform evidence.

Record versions, commands, results, failures, and affected features with each executable test. A passing host suite does not prove guest execution, full private-route success, production proving, or platform compatibility. Keep these gaps visible until the required evidence and review exist.

## Sources

- [Namespace and authority decision](../adr/0006-namespaces-and-authority.md)
- [Fee accounting decision](../adr/0015-fee-accounting.md)
- [LBP live-fee decision and RFP departures](../adr/0016-lbp-live-fees.md)
- [Settlement and graduation decision](../adr/0020-settlement-and-graduation.md)
- [Events and history decision](../adr/0027-events-and-history.md)
- [Token custody and freeze decision](../adr/0028-token-custody-and-freeze.md)
- [Workspace checks and execution limits](../architecture/workspace-guide.md)
- [RFP-015](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-015-bonding-curve-launchpad.md) and [curve proposal](https://github.com/logos-co/rfp/issues/118)
- [RFP-016](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md) and [LBP proposal](https://github.com/logos-co/rfp/issues/119)
