# Curve contract

Status: Accepted contract. Immutable trade modes and the public post-close adapter hook are implemented. Pricing and accounting keep the reviewed baseline. See [acceptance evidence](#acceptance-evidence) for passed checks and their limits. These checks do not establish production readiness.

Use the terms in [CONTEXT.md](../../CONTEXT.md). The [shared namespace, custody, and fee contract](namespace-fee-contract.md) defines namespace authority and live settings. [ADR 0009](../adr/0009-curve-arithmetic.md) defines the pricing decision. The amended [ADR 0020](../adr/0020-settlement-and-graduation.md) replaces the inactive-hook decision.

## Ownership and immutable inputs

The neutral curve pool owns pricing, real reserves, fee records and pool lifecycle. The curve factory owns sale allocations, creator authority, sale trade mode and post-close settlement. Direct neutral pools are internal primitives, not another sale product. Their owner-close operation does not give a creator an early-close operation on a factory sale.

A factory sale uses the launch token as token0 and collateral as token1. Creation fixes its namespace, launch identity, token definitions, curve program, allocations, initial virtual reserves, optional end timestamp, trade mode and optional adapter settings. A resumed creation must use the saved inputs; it cannot change them.

| Input | Contract |
| --- | --- |
| `D` | Nonzero purchasable allocation; initially the pool's real token0 reserve |
| `R` | Reserved launch tokens held outside trading reserves until post-close settlement |
| Creator allocation | Separate from `D` and `R`; its creator-authorized post-close claim is recorded once |
| Total supply | Checked `D + R + creator_allocation`; no wrap or extra minting |
| Initial virtual reserves | Each strictly between zero and `2^64`; virtual token0 reserve strictly greater than `D` |
| End timestamp | Optional; when present, strictly later than trusted chain time at creation |
| `TradeMode` | Required immutable `BuyOnly` or `BuyAndSell`; no program default or unknown wire value |
| Adapter settings | Optional saved program identity and opaque configuration; configuration without a program is invalid; no setter |

Initial real collateral is zero. Creation funds only `D` into the pool. `R` stays in factory custody and the creator allocation stays separate. Creation is staged through `Minted`, `Allocated`, `PoolPrepared` and `Active`. A prepared but unfunded pool must not trade. An action and its child calls complete in one transaction, or that stage does not complete.

`BuyOnly` permits collateral-input buys through both existing buy instructions and rejects launch-token-input sells. `BuyAndSell` also permits exact-input sells. There is no exact-output sell. The neutral account layer checks which input token is allowed. Direct pools without this restriction keep their existing behavior. The pure pricing state does not store factory trade modes. SDK quotes must apply the same restriction; a client check does not replace the program check.

Namespace protocol fees and treasury remain live. Creation and fee updates accept integer basis points from `0` through `9_999`, with denominator `10_000`. The pure pool's legacy handling of `10_000` is not permission to configure that rate in a namespace. A rate change affects later trades, not already recorded fees. A treasury change affects later collection.

## Pricing and fees

All amounts are raw integer token units. Let `V0` and `V1` be current virtual reserves, `r0` and `r1` be current real reserves, `F` be outstanding collateral fees and `H` be total collected fees. Creation fixes `k = V0_initial * V1_initial`. Every quote uses that saved `k`, not a new product of current reserves. Retained pool fees are unsupported; the pool fee input is zero.

For exact input, let `Vi` be the virtual input reserve, `Vo` the virtual output reserve and `e` the pricing input. The raw output is `q = Vo - ceil(k / (Vi + e))`. The input reserve increases by `e` and the output reserve decreases by `q`, for both virtual and real reserves. The participant's output can differ from `q` only because of a sell fee.

| Operation | Calculation | Payment and records |
| --- | --- | --- |
| Exact-input buy, gross collateral `G` | `f = ceil(G * b / 10_000)`, `e = G - f`, `q = V0 - ceil(k / (V1 + e))` | Debit `G`; pay `q` launch tokens; add `e` to real collateral and `f` to `F` |
| Exact-input sell, launch tokens `T` | `q = V1 - ceil(k / (V0 + T))`, `f = ceil(q * b / 10_000)`, `n = q - f` | Debit `T`; pay net collateral `n`; subtract full `q` from real collateral and add `f` to `F` |
| Exact-output buy, launch tokens `Q` | `E = ceil(k / (V0 - Q)) - V1`, `G = ceil(E * 10_000 / (10_000 - b))`, `f = ceil(G * b / 10_000)`, `e = G - f` | Debit fee-inclusive `G`; pay exactly `Q`; add actual `e` to virtual and real collateral and `f` to `F` |

Here `b` is the execution-time namespace fee rate. Exact output requires `Q < V0`, `Q <= r0` and positive required pricing input `E`. A rounded zero-cost inverse quote is rejected. The gross-up is the smallest integer gross input whose input after fees reaches `E`: `G - ceil(G*b/10_000) = floor(G*(10_000-b)/10_000)`. A forward quote at this gross input can exceed `Q`; exact-output execution still transfers only `Q`.

The formula notation does not require a potentially overflowing direct multiplication. The existing fee and gross-up helpers split `value` into `a = value / divisor` and `r = value % divisor`. They compute `a * multiplier + floor(r * multiplier / divisor)`, then add one if `(r * multiplier) % divisor` is nonzero. Each multiply and addition is checked. Division ceiling for pricing also uses quotient and remainder, rather than unchecked `numerator + divisor - 1`.

Exact-input limits compare the participant's output with `min_amount_out`. For a sell this is the output after fees. Exact-output buys compare the full collateral debit, including the fee, with `max_amount_in`. Equality passes. Fresh SDK quotes use current reserves, mode and fee settings, but cannot reserve them for execution. Concurrent trades or fee updates can cause a later transaction to fail its limit.

## Checked arithmetic envelope

Amounts and counters use `u128`, from zero through `2^128 - 1`. Time uses the trusted chain clock. The creation bounds ensure only the creation-time multiplication for `k` fits; they do not prove later trades fit. Runtime virtual reserves can exceed their creation bounds. The mathematical current product can exceed `u128`; it need not be multiplied to quote a trade.

Every accepted operation must pass all of these checks before its changes can complete:

- Creation checks nonzero `D`, the virtual-reserve bounds, `V0_initial > D`, checked total supply and a future optional end timestamp.
- Trades check positive input or requested output, positive pricing input, positive participant output and a nonzero pricing divisor. An input consumed by rounded fees, zero-output dust, zero-cost exact output or subtraction below zero fails.
- All virtual and real reserve additions and subtractions are checked. The full raw output, before sell fees, must fit the matching real output reserve. No trade spends pricing-only assets or accrued fees.
- Fee calculations, gross-up and `F + new_fee` are checked. Both `H + new_outstanding_fees` and `new_real_collateral + new_outstanding_fees` must fit. Collection checks `H + F` before clearing `F`.
- Physical token debits and credits are checked, including participant, pool, treasury, factory and adapter custody balances. The collateral vault must cover checked `r1 + F`; the launch-token vault must cover `r0`. Wrong addresses, owners, token definitions, layouts, permissions or namespace bindings fail.
- Withdrawal, recorded collateral due, token burns and each settlement transfer retain checked balance arithmetic. Adapter ATA snapshots supplied to the callback must contain the checked post-transfer balances.

There is no arbitrary trade minimum, percentage cap or new amount ceiling. Dust, overflow, output-reserve, physical-custody, mode, lifecycle and slippage checks define the accepted runtime envelope. A quote does not prove that all transaction accounts satisfy those checks.

## Solvency argument

This argument applies to a funded pool and any accepted sequence of buys, sells and fee collections before reserve withdrawal. It assumes canonical token accounts and transaction rollback when a child transfer fails. It proves custody and reserve safety, not a profitable sale or successful DEX integration.

At creation, `V0 * V1 = k`, launch custody covers `r0` and collateral custody covers `r1 + F`. For an exact-input trade, the new virtual output reserve is `ceil(k / (Vi + e))`. Thus the mathematical product of the new virtual reserves is at least `k`. For an exact-output buy, `V1 + e >= ceil(k / (V0 - Q))`, so the same inequality holds. Division and positive-amount guards exclude invalid transitions.

This is `current_virtual_product >= k`, not a claim that the current product never decreases. Integer rounding can leave a surplus over `k`, and a later quote still uses `k`. For example, with initial reserves `1_000 / 1_000`, zero fees and `k = 1_000_000`, two 100-unit token0 inputs leave virtual reserves `1_100 / 910`, then `1_200 / 834`. Their products are `1_001_000` and `1_000_800`. The second product decreases but remains above `k`.

Each trade changes its virtual and real input reserves by the same pricing input, and its virtual and real output reserves by the same raw output. The offsets `V0 - r0` and `V1 - r1` therefore remain fixed during trading, interpreted as mathematical integer differences. For a factory sale they start at `V0_initial - D > 0` and `V1_initial > 0`. Collection changes neither offset. The output guard independently requires `q <= real_output`; the virtual product alone cannot authorize a payout.

Let collateral custody be `B`. On a buy, `B` increases by `G = e + f`; the liability `r1 + F` increases by the same amount. On a sell, `B` decreases by `n = q - f`; the liability changes by `-q + f = -n`. On collection to a separate treasury ATA, both `B` and `F` decrease by the collected amount. If the current treasury is the pool, the fees are already in its collateral vault: collection clears `F` and increases `H` without a transfer to itself, so `B` stays unchanged. This increases custody above the remaining reserve liability. Induction gives `B >= r1 + F` for every accepted interleaving. Launch-token custody moves by the same amounts as `r0`. Checked additions exclude unrepresentable liabilities; physical credits and debits exclude unrepresentable or unfunded transfers. Rejected transactions preserve the prior state and balances.

Extra deposits increase custody without increasing reserves or fees. They remain surplus. Terminal withdrawal collects outstanding fees for the current treasury and moves only recorded real reserves. If the pool is the treasury, the collected fees remain in its collateral vault after reserve withdrawal. Clients omit a trailing treasury ATA when it duplicates a fixed account; programs reconstruct it from validated current namespace settings and canonical ATA derivation. Withdrawal retires the pool, so further swaps cannot spend those withdrawn reserves. Factory settlement then burns recorded unsold `D` and pays only the permitted recorded amounts once.

## Worked integer cases

These are independent arithmetic examples, not test-run results. They use a treasury ATA separate from the pool vault. Unless stated otherwise, start a factory sale with `D = 800`, `R = 100`, creator allocation `100`, `V0 = 1_000`, `V1 = 100`, `k = 100_000`, real collateral `0` and fee rate `1_000` basis points.

| Case | Literal result |
| --- | --- |
| Exact-input buy of 25 collateral | Fee `ceil(25*1_000/10_000) = 3`; pricing input `22`; output `1_000 - ceil(100_000/122) = 180`. New virtual reserves `820 / 122`, real reserves `620 / 22`, fees `3`, collateral custody `25`. `min_amount_out = 180` passes; `181` fails. |
| Sell 100 launch tokens after that buy | Raw collateral `122 - ceil(100_000/920) = 13`; fee `2`; participant gets `11`. New virtual reserves `920 / 109`, real reserves `720 / 9`, fees `5`, collateral custody `14`. `min_amount_out = 11` passes; `12` fails. |
| Collect after those two trades | Pay all `5` fees to the current treasury; `F = 0`, `H = 5`, real collateral and custody both `9`. Repeating collection pays zero. |
| Exact-output buy of 200 from the initial state | Required pricing input `ceil(100_000/800) - 100 = 25`; gross input `ceil(250_000/9_000) = 28`; fee `3`; actual pricing input `25`. Pay exactly `200`. `max_amount_in = 28` passes; `27` fails. A forward buy with gross `27` uses `24` and returns only `193`. |
| Zero-fee rounding from the initial state | Input `24` returns `193`, because `ceil(100_000/124) = 807`. Buying exactly `100` requires pricing input `12`, because `ceil(100_000/900) - 100 = 12`. |
| Maximum permitted fee | With `b = 9_999`, exact output `100` requires pricing input `12` and gross `120_000`; fee `119_988`. Gross `119_999` leaves only `11` after fees. A one-unit exact-input buy is consumed by its rounded one-unit fee and fails. |
| Real-output guard | From the initial state, a zero-fee buy of `500` quotes `833` launch tokens, more than `D = 800`; reject it rather than spend `R` or cap the output silently. A sell before any buy cannot pay collateral because real collateral is zero. |
| Creation boundaries | `V0 = D` fails; either virtual reserve equal to zero or `2^64` fails. `D = 1`, `R = 2^128 - 1`, creator allocation `0` fails the supply addition. An end timestamp equal to creation time fails. |
| Runtime overflow | A reserve at `2^128 - 1` cannot accept an additional unit. With `H = 2^128 - 1`, a trade that adds one fee unit fails even if its price and physical input otherwise fit. Physical destination credit overflow also fails the transaction. |

A dust example uses `V0 = V1 = 1_000`, zero fees and input `1`: output is `1_000 - ceil(1_000_000/1_001) = 0`, so reject it. These failures must leave records and balances unchanged. Exact-output sell requests are rejected regardless of fee or liquidity.

## Closing and settlement

A factory sale trades only after creation is active and pool funding is complete. It closes immediately when its real token0 sale reserve reaches zero, or when trusted `now >= end_timestamp` if expiry is configured. Selling can restore sale inventory only while the sale remains open; it cannot reopen a depleted sale. No collateral-raised target, creator early close or separate close transaction is added. Expiry is effective even if stored pool lifecycle is still `Open`.

The contract has no minimum duration for optional expiry. This remains an accepted internal departure from RFP-015 with external status `To be reviewed by Logos team`. Frontend duration policy remains undecided. See [ADR 0025](../adr/0025-sale-closing.md).

Closure permits settlement; it does not execute the adapter. A client must submit a separate post-close hook transaction and continue any required stages. Anyone may submit or retry a configured hook. The operational submitter and fee payer are deferred. No bot, UI, new administrator role or live DEX is part of this contract.

| Stage or route | Required effect |
| --- | --- |
| Shared custody preparation | Validate the saved factory, pool, namespace, tokens, trusted clock and canonical ATAs; prepare factory collateral custody |
| Shared reserve withdrawal | Collect all remaining fees to the current treasury in the same transaction as withdrawal; record exact unsold `D` and real collateral due; withdraw recorded reserves and retire the pool |
| Shared burn | Burn recorded unsold `D` after expiry once; zero unsold inventory requires no burn; keep `R` and creator allocation separate |
| Manual payout | Current creator authority holder receives full `R`, then full recorded collateral due through recorded, repeat-safe stages |
| Adapter-owned setup prerequisite | The saved adapter initializes and claims its canonical custody PDA under its own program before factory destination-ATA preparation |
| Adapter custody preparation | Check saved-adapter ownership of custody, then prepare its two canonical ATAs in a separate idempotent stage; this does not select or lock the payout route |
| Final hook payout | Transfer full `R` and full recorded collateral due to saved adapter custody, then call the saved adapter; record terminal `Graduated` in the same transaction |
| Creator-allocation claim | Separate creator-authorized claim; does not select either settlement route or enter hook amounts |

The saved shared stages are `Unstarted`, `Ready`, `Withdrawn { unsold, collateral_due }` and `Burned { collateral_due }`. The hook adds `HookReady { collateral_due }`, `Graduated` and `NoPayout` to the existing manual `TokenPaid { collateral_due }` and `Complete` states. These states record progress so settlement can resume. Preparation does not select a payout route.

The first actual manual settlement payout blocks hook payout. `TokenPaid` and a completed paying manual route cannot race with the hook. `Graduated` blocks manual payout and hook replay. Route checks must also block duplicate payout or reentry during the final call. Shared preparation, reserve withdrawal, burn and adapter-ATA preparation do not select a route.

Zero amounts require distinct handling. When `R = 0` and collateral due is positive, manual settlement skips the empty token-payment marker and performs the collateral payout directly. When both amounts are zero, manual bookkeeping records `NoPayout`; an uncalled configured hook may still execute once with zero amounts. Empty transfers and the separate creator-allocation claim must not block that hook. A successful zero-amount hook still records `Graduated` and cannot run twice.

Each transaction completes in full or leaves its state and balances unchanged. If the adapter fails during the final stage, both final transfers and the `Graduated` state change roll back. Earlier successful custody preparation, fee collection, reserve withdrawal and burn transactions stay complete. The unchanged final-stage state permits retry with the saved adapter settings or creator-authorized manual recovery. The adapter can change its own readiness state; the sale cannot replace its adapter or configuration.

For the buy/sell example above, with a treasury ATA separate from the pool vault, expiry leaves `720` unsold `D`, `R = 100`, creator allocation `100`, real collateral `9` and outstanding fees `5`. If the pool collateral vault also contains a surplus deposit of `7`, its physical balance is `21`. Withdrawal pays `5` to treasury and moves `9` to factory custody; the surplus `7` remains outside settlement. Burn the unsold `720` once. The final hook receives exactly `100` launch tokens and `9` collateral, not the surplus, fees or creator allocation. A failing callback restores these two final debits and credits; it does not restore the burned inventory or reverse the earlier treasury payment. Manual recovery pays the same `100` and `9` to the creator instead.

## General adapter hook

The factory saves an optional adapter program and opaque configuration at creation. An adapter can interpret that configuration as its own namespace or settings. The factory does not interpret DEX fields or call a specific DEX. The adapter owns checks for its result. Factory success proves only that the saved callback and required transfers completed; `Graduated` is a terminal factory route marker, not proof of a real DEX pool or external liquidity migration.

The typed settings are `GraduationHook { program_id: ProgramId, configuration: Vec<u8> }`, saved as `FactoryState.graduation_hook: Option<GraduationHook>`. Creation wire fields are `graduation_hook_program_id: Option<ProgramId>` and `graduation_hook_configuration: Vec<u8>`. The public `ExecuteGraduationHook { remaining_accounts: u32 }` instruction can identify the remaining-account count; it cannot override saved settings or amounts.

The fixed serialized `GraduationRequest` envelope has these fields in order:

| Field | Trusted source |
| --- | --- |
| `factory_program_id` | Executing factory program |
| `namespace` | Saved sale namespace |
| `launch_salt` | Saved sale identity |
| `token_definition_id` | Saved launch-token definition |
| `collateral_definition_id` | Saved collateral definition |
| `dex_seed_reserve` | Full saved `R` |
| `collateral_amount` | Full recorded real collateral due |
| `configuration` | Saved opaque adapter configuration |

The callback account prefix is the factory snapshot, adapter custody PDA, launch-token definition, collateral definition, credited adapter launch-token ATA and credited adapter collateral ATA. Ordered remaining accounts follow that prefix for adapter-specific state. Handler tests verify the envelope and prefix; focused guest tests verify executed transfers and callback receipts. Public generated-IDL parity passed in the full IDL lane. Clients must use the released generated interface rather than infer byte encoding from this document.

Adapter custody is a PDA derived under the saved adapter program from the saved factory identity, using `compute_graduation_seed(factory)` and `compute_graduation_pda(adapter, factory)`. Both destination ATAs are canonical for that custody and the saved token definitions. Neither custody nor ATAs are caller-selected payout addresses. Callback snapshots contain balances after the exact transfers. A caller cannot override the program, configuration, amounts, destination, sale identity or token definitions at execution time.

Before the factory prepares destination ATAs, the saved adapter must initialize and claim its own canonical custody PDA. The factory must verify that custody is owned by the saved adapter program; a correct PDA address alone is insufficient. The pinned [ATA `Create` implementation](https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lez/programs/associated_token_account/src/create.rs) claims a default owner with `Claim::Authorized`, even when the ATA already exists. An untouched default custody account can therefore fail with `ClaimedUnauthorizedAccount`. The factory does not fix this by authorizing adapter custody or sharing factory seeds. Adapter-owned setup is separate from the factory hook; its precise interface belongs to the adapter.

Missing or wrongly owned custody must reject factory destination-ATA preparation without payout or route selection. That failed transaction leaves its state and balances unchanged. Earlier completed withdrawal, fee collection and burn stages remain complete. Once the saved adapter completes setup, anyone can retry the hook with the same saved settings. Creator-authorized manual recovery remains available until an actual payout selects a route. The setup guard has handler evidence and successful adapter-owned setup has guest evidence; a failed-setup recovery transaction is not a recorded focused guest case.

The initial hook is public only. The root public hook requires an unauthorized factory snapshot and unauthorized callback-prefix accounts. Remaining accounts must also be unauthorized and unique, must not alias protected prefix or settlement accounts, and must not include additional factory-owned accounts. Supplied order is preserved. Runtime authorization flags must match validated caller authorization; clearing flags after validation is not an authority boundary.

Only exact withdrawal, burn and transfer children receive the factory PDA seeds they require. The adapter callback receives no factory seeds or factory signing rights. The adapter verifies the runtime caller program and authorizes only its own PDAs. Unauthorized input metadata does not prohibit updates to adapter-owned state; normal LEZ program-ownership checks still apply. Private-wrapped hook execution is explicitly rejected. Existing private manual settlement is retained; private child wrapping applies only to actual curve-program children, never to an arbitrary adapter decoded as a curve instruction.

The pinned runtime permits [10 chained calls per transaction](https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lee/state_machine/core/src/program.rs) and limits [each public execution to `32 * 1024 * 1024` cycles](https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lee/state_machine/src/program.rs). A final hook with both payout amounts nonzero uses five chained calls before adapter descendants. Those descendants must fit the remaining call budget and applicable cycle limits. Opaque configuration and remaining accounts do not imply arbitrary-size execution support; no general size or cycle envelope has been proved.

## Release compatibility

The release changes factory state and creation instruction encoding, and adds the neutral account restriction. The new state and instruction encoding are incompatible with the old versions. Factory public instruction tags `0` through `3` stay fixed; `ExecuteGraduationHook` is inserted at `4` before `Private`, whose factory tag shifts to `5`. Rebuild guests and client artifacts together; their program identities can change. Use fresh local state. Do not silently decode old state, infer missing trade modes or claim a deployed-state migration. The pure `pool::Pool` pricing layout remains unchanged.

The project `launchpad create-sale` command and SDK can create a sale without a hook or with empty hook configuration; the project CLI accepts hexadecimal configuration bytes. The pinned `lgs` SPEL parser cannot encode an empty `Vec<u8>` from either `''` or `'[]'`. Nonempty decimal bytes such as `'7,11'` work. Use the SDK or project CLI for empty-configuration creation. This upstream parser limit does not change the wire contract or prevent independent IDL generation and parity checks.

## Acceptance evidence

These are actual test names inspected in the repository. Existing baseline tests identify coverage. Verified host suites passed with 53 curve-core tests, 45 factory-core tests, 37 SDK tests and 24 CLI tests. The full workspace suite and strict all-targets/all-features Clippy passed. The guest build and full guest suites passed with 3 private-authority tests and 9 settlement tests. Their lane uses `RISC0_DEV_MODE=1`: it executes guests and runtime checks, but does not produce secure production proofs. Handler tests prove their stated validation and emitted-call behavior; they do not alone prove child-call rollback. Runtime evidence is recorded separately. Generated-IDL comparisons and all 5 pinned-CLI wire tests passed. Formatting, dependency pins and diff whitespace checks also passed. See [ADR 0022](../adr/0022-verification-boundaries.md).

| Acceptance case | Existing test location and name | Evidence limit or remaining check |
| --- | --- | --- |
| Creation envelope and allocation sum | `crates/factory-core/src/lib.rs`: `launch_requires_virtual_token_reserve_above_tradeable_supply`, `launch_rejects_virtual_reserves_outside_the_curve_arithmetic_bound`, `overflow_cannot_create_an_unbacked_supply`, `fixed_supply_is_the_exact_three_way_split` | Baseline factory validation |
| Pricing rounding and inverse | `crates/curve-math/src/lib.rs`: `exact_input_rounds_output_down`, `exact_output_rounds_input_up`, `exact_output_input_is_the_smallest_integer_that_reaches_the_requested_output` | Pure math, not account custody |
| Fee-inclusive pricing and input cap | `crates/pool/src/lib.rs`: `protocol_fee_is_collateral_input_on_buy_and_collateral_output_on_sell`, `exact_output_swaps_token1_for_token0_with_the_fee_inside_max_input`, `exact_output_uses_the_smallest_fee_inclusive_gross_input` | State-machine coverage |
| Fixed creation-time invariant | `crates/pool/src/lib.rs`: `later_quotes_continue_to_use_the_creation_time_k` | No monotonic-current-product assertion |
| Dust, real-output and zero-cost guards | `crates/pool/src/lib.rs`: `exact_input_rejects_a_nonzero_trade_that_rounds_to_zero_output`, `a_swap_cannot_exceed_the_selected_real_output_reserve`, `exact_output_rejects_a_zero_cost_quote_without_mutating_the_pool` | Rejection preserves pure pool state |
| Counter overflow and interleavings | `crates/pool/tests/proptest_invariants.rs`: `fee_counter_overflow_rejects_both_buy_forms_without_mutation`, `randomized_interleavings_preserve_solvency_conservation_and_atomicity` | Randomized pure state transitions, not full guest settlement |
| Namespace fee range and execution slippage | `crates/curve-core/src/tests.rs`: `namespace_fee_rate_is_below_one_hundred_percent_on_creation_and_update`, `execution_fee_update_respects_buy_and_sell_slippage` | Program-handler coverage |
| Current treasury, surplus and liabilities | `crates/curve-core/src/tests.rs`: `permissionless_collection_uses_live_treasury_and_preserves_principal`, `accrued_fees_cannot_back_a_sell_or_withdrawal` | Handler account checks; guest lane is separate |
| Depletion and expiry | `crates/pool/src/lib.rs`: `final_exact_input_swap_closes_a_pool_configured_to_close_on_token0_depletion`, `final_exact_output_swap_closes_a_pool_configured_to_close_on_token0_depletion`, `swaps_are_rejected_at_the_close_timestamp_without_mutating_reserves` | Neutral pool lifecycle |
| Factory expiry settlement and allocation separation | `crates/factory-core/src/lib.rs`: `withdrawal_prepares_collateral_custody_before_moving_reserves`, `expiry_settlement_burns_unsold_d_once_and_keeps_r_and_creator_allocation` | Handler stages and emitted children; not final callback rollback |
| Existing real guest settlement | `crates/launchpad-client/tests/settlement.rs`: `fee_accrual_collection_and_withdrawal_preserve_vault_liabilities`, `private_sale_resumes_creation_and_settlement_with_fresh_payout_holders` | Both passed in the full guest suite, including private staged creation/manual settlement with fresh payout holders and a stop at the saved terminal stage |
| Existing quote builders | `crates/launchpad-client/src/tests.rs`: `quote_builders_match_pool_snapshot_rules_and_map_errors` | Does not yet establish the new mode restriction |
| Saved adapter settings through creation and SDK resume | `crates/launchpad-client/tests/settlement.rs`: `public_creation_saves_immutable_hook_configuration` | Passed focused real guest test: program and configuration persist through all creation stages; SDK resume rejects changes to either |
| Required mode wire values | `crates/factory-core/src/hook_tests.rs`: `creation_wire_rejects_invalid_or_missing_trade_mode`; `crates/launchpad-client/src/tests.rs`: `creation_request_encodes_the_required_typed_mode` | Passed host tests: reject unknown or missing mode encoding and map both typed modes to their wire values |
| Neutral restriction and both public buy forms | `crates/curve-core/src/tests.rs`: `pool_creation_validates_and_saves_the_neutral_input_restriction`, `restricted_pool_rejects_selling_through_public_swap`, `neutral_input_restrictions_apply_to_both_public_buy_forms` | Passed handler tests: validate the saved restriction, reject restricted selling and permit both collateral-input buy forms; unrestricted pools retain both buy forms |
| SDK mode immutability and quotes | `crates/launchpad-client/src/tests.rs`: `resumed_creation_cannot_change_trade_mode_at_any_stage`, `buy_only_quotes_reject_selling_and_keep_both_buy_forms`, `token0_only_pool_quotes_reject_both_collateral_buy_forms` | Passed host tests: all creation stages reject mode changes; BuyOnly quotes reject sells and retain both buys; neutral token0-only pools reject both buy quotes |
| Hook settings validation and continuation | `crates/factory-core/src/hook_tests.rs`: `creation_saves_hook_settings_and_continuation_preserves_them`, `creation_rejects_configuration_without_a_hook_program` | Passed handler tests; focused guest creation evidence above also verifies persistence and SDK retry rejection |
| Permissionless configured-hook stages | `crates/factory-core/src/hook_tests.rs`: `noncreator_can_prepare_factory_collateral_for_configured_hook`, `hook_withdrawal_records_reserves_without_fees_and_burns_only_unsold_d` | Passed handler tests: no creator authorization on hook preparation; recorded collateral excludes fees and only unsold `D` enters the burn call. Focused guest payout below verifies the executed stages |
| Adapter setup prerequisite | `crates/factory-core/src/hook_tests.rs`: `adapter_must_claim_its_custody_before_factory_prepares_atas`, `hook_prepares_both_adapter_atas_without_a_payout` | Passed handler tests reject default custody and prepare ATAs without payout or adapter authority seeds. Focused guest cases use adapter-owned setup. A failed-setup recovery transaction is not a recorded focused guest case |
| Exact payout, callback receipt and allocation separation | `crates/launchpad-client/tests/settlement.rs`: `permissionless_hook_transfers_only_reserved_tokens_and_recorded_collateral` | Passed focused real guest test: adapter receives `R = 100`, recorded collateral `12` and saved configuration; treasury receives fees `2`; unsolicited factory token/collateral surplus `19 / 23` and creator allocation `100` stay separate |
| Callback envelope, credited snapshots and ordered accounts | `crates/factory-core/src/hook_tests.rs`: `final_hook_transfers_recorded_amounts_and_supplies_credited_custody`, `hook_preserves_remaining_account_order_and_root_post_order`, `hook_validates_remaining_count_before_account_split` | Passed handler tests: verify the fixed envelope, exact transfer children, credited snapshots, unauthorized callback metadata, no callback factory seeds, preserved account order and rejected invalid counts. Executed payout and callback receipts are separately proved by the focused guest case above |
| Failed callback and readiness retry | `crates/launchpad-client/tests/settlement.rs`: `failed_hook_rolls_back_final_payout_and_can_retry_after_adapter_readiness_changes` | Passed focused real guest test: final callback failure leaves all V03State accounts unchanged at `HookReady`, with custody uncredited; earlier burn and treasury fees remain complete; adapter-owned readiness change permits retry to `Graduated` |
| Failed callback and creator manual recovery | `crates/launchpad-client/tests/settlement.rs`: `failed_hook_keeps_creator_manual_recovery_available_with_or_without_reserved_tokens` | Passed focused real guest test for `R = 100` and `R = 0`, each with collateral `12`: manual recovery reaches `Complete`; later hook is refused without changing factory state or creator payouts |
| SDK terminal-stage helper | `crates/launchpad-client/src/tests.rs`: `saved_settlement_stages_identify_completed_manual_work` | Passed host test: `Complete`, `NoPayout` and `Graduated` end the manual loop; preparation and intermediate stages do not. This does not prove a zero-amount guest payout |
| Manual route selection and terminal replay guards | `crates/factory-core/src/hook_tests.rs`: `creator_can_choose_manual_payout_after_hook_ata_preparation`, `manual_token_payout_blocks_hook_execution`, `successful_hook_blocks_replay_and_manual_payout` | Passed handler tests: preparation does not lock a route; manual token payout blocks hook execution; `Graduated` rejects both routes. Executed manual recovery and zero-payout replay cases are mapped separately |
| Zero-amount manual transitions | `crates/factory-core/src/hook_tests.rs`: `zero_r_manual_settlement_pays_collateral_without_an_empty_token_marker`, `zero_manual_payout_keeps_an_uncalled_zero_hook_available` | Passed handler tests: skip empty token payout and preserve an uncalled hook after `NoPayout` |
| Hook with either or both amounts zero | `crates/launchpad-client/tests/settlement.rs`: `hook_runs_once_when_either_or_both_payout_amounts_are_zero` | Passed focused real guest test for token/collateral amounts `0 / 12`, `100 / 0` and `0 / 0`: exact callback receipt, `Graduated`, and rejection of later hook and manual calls with all accounts unchanged |
| `NoPayout` and separate creator claim | `crates/launchpad-client/tests/settlement.rs`: `no_payout_manual_bookkeeping_and_creator_claim_do_not_block_a_zero_amount_hook` | Passed focused real guest test: creator allocation `100` is claimed separately; manual bookkeeping reaches `NoPayout`; one zero-amount hook reaches `Graduated`; replay fails and the creator claim stays unchanged |
| Adapter reentry | `crates/factory-core/src/hook_tests.rs`: `successful_hook_blocks_replay_and_manual_payout` | Handler evidence rejects the callback's terminal factory snapshot on a new settlement call. No adapter-initiated reentry guest run is recorded |
| Account and authorization boundary | `crates/factory-core/src/hook_tests.rs`: `hook_rejects_accounts_that_expose_factory_authority_or_protected_state`, `settlement_rejects_noncanonical_pool_and_token_custody_before_children`, `private_wrapped_hook_is_rejected_at_the_public_boundary`, `private_factory_wraps_only_children_for_the_saved_curve_program` | Passed handler tests: reject authorized callback inputs, protected aliases, duplicate or factory-owned remaining accounts, wrong custody and private hook wrapping; preserve non-curve child encoding. These are handler rejection tests, not separate runtime transactions for every malformed account |
| Checked settlement custody and credits | `crates/factory-core/src/hook_tests.rs`: `settlement_checks_custody_liabilities_and_credits_before_children`, `manual_payout_checks_source_balance_and_recipient_credit_before_children` | Passed handler tests: reject reserve/fee deficits, insufficient burn custody or supply, and destination credit overflow before emitting children |
| Pool treasury and unique collection accounts | `crates/curve-core/src/tests.rs`: `withdrawal_accepts_an_omitted_treasury_that_is_the_pool_collateral_vault`, `withdrawal_leaves_collected_fees_in_the_pool_when_it_is_the_treasury`, `collection_accepts_unique_accounts_when_fees_are_already_at_the_treasury`; `crates/launchpad-client/src/tests.rs`: `collection_builder_omits_the_treasury_when_it_is_the_pool_vault` | Passed host tests: reconstruct the canonical treasury, collect without self-transfer, preserve physical fees during reserve withdrawal and omit duplicate collection accounts |
| Treasury alias and remaining-account order | `crates/factory-core/src/hook_tests.rs`: `omitted_treasury_is_reconstructed_from_fixed_accounts_without_reordering_remaining`, `hook_withdrawal_accepts_full_collateral_vault_when_it_is_the_treasury`; `crates/launchpad-client/tests/settlement.rs`: `hook_preserves_recorded_payouts_when_current_treasury_aliases_settlement_custody` | Passed host reconstruction and maximum-balance guard tests. Passed real guest test with current treasury at factory custody, adapter custody or pool: recorded hook collateral stays `12`; fees `2` stay at the selected treasury; remaining-account order is preserved |
| SDK graduation builder | `crates/launchpad-client/src/tests.rs`: `graduation_builder_is_unsigned_and_preserves_remaining_account_order` | Passed host test: unsigned builder preserves order and serializes hook tag `4` with the remaining-account count |
| Configured creation parity and upstream parser diagnostic | `crates/launchpad-client/tests/idl_wire.rs`: `factory_public_instruction_wire_matches_the_pinned_cli`, `pinned_cli_reports_its_empty_hook_configuration_limit` | Passed in the full IDL lane: nonempty configured-hook creation matches pinned CLI serialization, while empty configuration is rejected by that parser |
| CLI, state and generated wire checks | `cli/src/main.rs`: `create_sale_requires_trade_mode_with_only_the_two_named_values`, `graduate_accepts_ordered_public_adapter_accounts_without_creator_authority`, `saved_hook_and_settlement_json_values_are_explicit`; `crates/launchpad-client/tests/idl_wire.rs`: `curve_public_instruction_wire_matches_the_pinned_cli`, `graduation_instruction_wire_matches_the_pinned_cli` | Passed all 24 CLI tests, generated-IDL comparisons and all 5 wire tests, including collection with or without a trailing treasury account. No live wallet command run is recorded |

Passing checks do not establish production readiness. The missing runtime cases and proof limits remain stated in the table and adapter section. Live DEX integration, external readiness policy, operational submitter and payer selection, frontend defaults and Logos approval of the minimum-duration departure remain outside this verification claim.
