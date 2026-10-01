# Namespace, custody, and fee contract

Status: M0 draft. Final acceptance needs platform tests and design review.

Use the terms in [CONTEXT.md](../../CONTEXT.md). These rules apply to curve and LBP sales. Their pricing and sale lifecycles remain separate.

## Namespace and authority

Any caller can create a namespace with valid authorization. Creation must set its administration state in the same transaction. It must not leave state that another caller can claim.

Each sale keeps its creation namespace. Accounts, listings, and history must stay within that namespace. Programs must check account identity; a UI filter is not sufficient. The shared physical schema and cross-factory account owner remain open.

| Action | Required authority | Allowed change |
| --- | --- | --- |
| Change fee | Current namespace administrator | Fee only; preserve treasury and administrator |
| Change treasury | Current namespace administrator | Treasury only; preserve fee and administrator |
| Transfer administration | Current administrator and successor acceptance | Administrator only, under the selected module's rules |
| Renounce administration | Current namespace administrator | Remove administration permanently; preserve the last fee and treasury |
| Create or settle a sale | Current creator authority holder | Only actions allowed by the factory and sale stage |
| Trade | Authorized participant | Only trades allowed by account, amount, and sale-state checks |
| Collect fees | Any caller | Transfer all accrued fees to the current treasury |

Namespace administration grants no creator or custody rights. Creator Authority NFT ownership can change without changing the sale or role identity. Renunciation does not stop permitted trading, collection, or settlement.

Use one administration state. Do not copy its administrator field, read private module storage, or add a separate pending-transfer model. The selected module has no supported current-administrator getter. Its dispatcher integration is also unverified. The PoC still uses NFT administration, direct admin replacement, and one combined configuration update. Separate production setting operations are not implemented. See [ADR 0006](../adr/0006-namespaces-and-authority.md).

Sale inputs, token roles, and namespace binding are fixed at creation. The namespace fee and treasury remain live until renunciation. A new fee affects later trades. A new treasury receives later collections, including fees from earlier trades.

## Custody checks

Check the sale and namespace account identities, derivations, and program owners. Check each token account's owner, token definition, ATA address, token-program data, and required authorization. Timing checks must use the trusted chain clock.

Derive the treasury collateral ATA from the current treasury and sale collateral definition. The caller cannot select another destination. A client can create a missing ATA through the supported platform operation. If the treasury changes, read its current state and prepare the new ATA before retrying. When one account has more than one role, do not transfer twice.

Curve collateral custody must cover `real collateral reserve + outstanding fees`. Fees cannot back trade output or reserve withdrawal. Extra deposits do not become protocol fees or creator proceeds by default.

LBP custody must cover outstanding fees and all remaining permitted payouts. Pricing state does not prove custody balance. Initial collateral ownership, payout rules, and custody coverage after early collection still need LBP evidence.

Keep mint authority, custody, administration, and pause/freeze rights separate. Namespace administration alone grants no pause or freeze right. The authority and instruction gates remain open. Recovery and settlement paths need review. See [ADR 0028](../adr/0028-token-custody-and-freeze.md).

## Fees and collection

Namespace rates are integers from 0 through 9,999 basis points: 0% through 99.99%. Reject 10,000 and higher at creation or update. Reject a default treasury identity. Keep 10,000 as the fee denominator.

Amounts use integer base units, the token's smallest units. Fees use collateral: token1 for curve factory sales. Calculate `ceil(amount * rate / 10_000)` with checked arithmetic. Read the rate at execution. Track outstanding fees and total collected fees separately from reserves and creator claims.

| Trade | Fee base | Accounting rule |
| --- | --- | --- |
| Curve buy | Gross collateral input | Deduct the fee before pricing; add only net input to the real reserve |
| Curve sell | Gross collateral output | Deduct the fee from the participant's payout; remove gross output from the real reserve |
| LBP buy | Full collateral input | Use full input for pricing; record a creator-borne fee claim on collateral proceeds |

LBP sells are not specified. A valid rate can still produce an invalid trade. For example, a 1-unit curve buy at 1 basis point leaves no input after fee rounding. Amount, slippage, arithmetic, and custody checks still apply.

Any caller can collect all outstanding fees. Transfer them to the current treasury. Set outstanding fees to zero and add the transfer amount to total collected fees. Do not calculate old fees again at a new rate or charge a collection fee. Zero or repeated collection makes no transfer and does not increase collected fees. Account checks still apply. Later trades can add new fees.

Curve collection is available anytime. The internal LBP design also permits anytime collection, including during pause. Live LBP rates and early collection depart from the RFP baseline. Their external status remains `To be reviewed by Logos team`. Creator withdrawal still waits for the original end timestamp. Pause does not move the schedule or stop weight progression. See [ADR 0016](../adr/0016-lbp-live-fees.md).

Terminal settlement collects remaining fees in the same transaction as the related reserve or proceeds payout. A failure must leave that stage's state and balances unchanged. Earlier completed stages remain complete. The full factory workflow can span transactions. Retries must not repeat a completed payout or burn. See [ADR 0015](../adr/0015-fee-accounting.md) and [ADR 0020](../adr/0020-settlement-and-graduation.md).

The client's [batch helper](../../crates/launchpad-client/src/lib.rs) submits one curve collection transaction per sale, in sequence. Check each sale's namespace, owner, and collateral definition. Refresh current treasury state for each sale. A failure does not undo earlier transactions. The helper stops at the first error and returns hashes only on full success. Per-sale progress and failure reporting remain client work. Check receipts and refresh state before retrying uncertain results.

## Worked balances

### Curve

These are separate examples from the [pool tests](../../crates/pool/src/lib.rs). Each starts with launch reserve 800, real collateral 100, virtual launch reserve 1,000, and virtual collateral 100. The fixed pricing invariant is `k = 100,000`. The fee rate is 1,000 basis points.

| Case | Fee | Participant result | Real launch reserve | Real collateral | Physical collateral | Outstanding fees | Total collected |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| Buy with 25 collateral | 3 | Price 22; receive 180 launch tokens | 620 | 122 | 125 | 3 | 0 |
| Collect after buy | - | Treasury receives 3 | 620 | 122 | 122 | 0 | 3 |
| Sell 250 launch tokens | 2 | Gross output 20; receive 18 collateral | 1,050 | 80 | 82 | 2 | 0 |
| Collect after sell | - | Treasury receives 2 | 1,050 | 80 | 80 | 0 | 2 |

Repeated collection leaves each final balance unchanged. Reserve payout collects only remaining fees and charges no new trade fee.

### LBP

This example shows accounting only. It does not prove weighted math or select production pricing state. Let `P` be full-input pricing collateral and `V` physical collateral. Both start at 100. Initial collateral ownership and payout remain open. Token output requires a valid quote.

| Step | P | V | Outstanding fees | Total collected |
| --- | ---: | ---: | ---: | ---: |
| Buy 25 at 1,000 bps | 125 | 125 | 3 | 0 |
| Change treasury to B; collect 3 to B | 125 | 122 | 0 | 3 |
| Change fee to 2,000 bps; buy 40 | 165 | 162 | 8 | 3 |
| Terminal collection of 8 to B | 165 | 154 | 0 | 11 |

New input totals 65; fees total 11; net added collateral is 54. The remaining 154 includes the initial 100. Do not assign all 154 to creator proceeds or deduct the same 11 again. Repeated collection changes nothing.

## Events and errors

Use supported LP-0012 events and existing indexer tools. Each event must identify its namespace and chain location. Events report committed effects only. The encoding and field types still need platform checks.

| Event | Minimum additional fields |
| --- | --- |
| Namespace creation | Initial administrator, rate, treasury |
| Fee or treasury update | Setting changed; old and new values |
| Admin transfer or renunciation | Lifecycle action; affected administrator identities |
| Trade | Sale, mechanism, token roles, actual input/output, fee base, execution rate, fee amount, pricing/net amount, fee counters |
| Collection | Sale, collateral definition, amount, current treasury, resulting fee counters |
| Settlement stage | Sale, mechanism, completed stage, remaining fees collected, payout/burn amounts, resulting state |

Zero collection must report no transfer or counter increase. Do not publish persistent private participant identifiers. Public pool state, amounts, and trade effects remain visible. Indexed reads can lag; use current chain state for authorization and transfers. See [ADR 0027](../adr/0027-events-and-history.md).

These error names group failures by cause. They are required categories, not implemented platform codes. Program and client mapping remain open.

| Category | Cause |
| --- | --- |
| `Unauthorized` | Missing or wrong authorization or authority proof |
| `AdministrationRenounced` | Administrative action after renunciation |
| `NamespaceMismatch` | Sale binding or namespace state does not match |
| `AccountMismatch` | Wrong derivation, owner, token data, alias layout, or clock |
| `InvalidFeeRate` | Rate outside 0 through 9,999 or not representable |
| `InvalidTreasury` | Default or unsupported treasury; wrong treasury ATA |
| `InvalidSaleState` | Action not permitted in the current stage |
| `InvalidTradeAmount` | Amount bounds or rounding reject the trade |
| `SlippageExceeded` | Execution exceeds the participant's quote limit |
| `ArithmeticFailure` | Arithmetic or counter update cannot complete safely |
| `InsufficientCustody` | Tokens do not cover fees, output, or remaining claims |

Clients must give a usable cause and next action without private identifiers. Validation order is not a client contract. Zero fees are a successful result.

## Acceptance checks and open work

| Check | Required result |
| --- | --- |
| Create two namespaces; supply foreign accounts | Isolated state and history; reject account or authority substitution |
| Create/update fees; lower a legacy 100% fixture | Accept 0 and 9,999; reject 10,000 and higher; allow lowering to a valid rate |
| Change one setting; transfer or renounce administration | Preserve unrelated fields and custody; require successor acceptance; renunciation is permanent |
| Change fee after a quote; change treasury with fees pending | Use execution rate and quote limits; send old fees to the current treasury |
| Collect zero; repeat collection; donate collateral | No duplicate transfer or counter increase; donations do not become fees |
| Underfund custody; fail a terminal token call | Reject without partial changes in that transaction stage |
| Fail the second sale in a batch | Keep the first confirmed result; allow safe retry with fresh state |
| Pause an LBP; settle; index a private-route trade | Keep schedule and withdrawal gate; collect remaining fees once; omit persistent private identifiers |

The [curve host tests](../../crates/curve-core/src/tests.rs) cover the fee limit, legacy fixture, namespace checks, live fees, and collection. They do not prove the selected administration lifecycle or a live upgrade. Separate setting operations and the LBP program are not implemented.

Final acceptance needs target-platform evidence for custody, ATAs, rollback, clock reads, events, administration, and freeze gates. LBP math, rounding, seed ownership, and early-collection coverage also need proof. Record versions, commands, results, and failures. Keep Logos fee-policy review open.

A passing host suite does not prove guest execution, a successful full private route, or production compatibility. See the [workspace guide](../architecture/workspace-guide.md) for separate execution checks. Final design review must compare this contract with mechanism rules, client behavior, and platform evidence.
