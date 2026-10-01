# Namespace, custody, and fee contract

M0 design contract for curve and LBP sales. Use the terms in [CONTEXT.md](../../CONTEXT.md).

## Namespace and authority

Namespace creation is permissionless. It sets the initial administrator, fee, and treasury in one transaction. Each sale keeps its creation namespace. Sale inputs and token roles stay fixed after creation. Accounts, listings, and history are scoped to that namespace.

| Operation | Authority | State change |
| --- | --- | --- |
| Change fee | Current administrator | Fee only |
| Change treasury | Current administrator | Treasury only |
| Transfer administration | Current administrator and accepting successor | Administrator only |
| Renounce administration | Current administrator | Permanently remove administration; retain fee and treasury |
| Create or settle a sale | Current creator authority holder | Factory actions allowed in the current sale stage |
| Collect fees | Any caller | All outstanding fees move to the current treasury |

Administration uses one state and the [selected module's lifecycle](../adr/0006-namespaces-and-authority.md). Creator authority and custody remain separate from administration. Creator Authority NFT ownership can change while its role identity stays fixed.

Fee rates are integers from 0 through 9,999 basis points: 0% through 99.99%. Creation and updates enforce this range and require a valid treasury identity. Trades use the current rate. Collection uses the current treasury, including for fees from earlier trades. Renunciation retains these settings while trading, collection, and settlement continue under their sale rules.

## Custody

Sale and namespace accounts are checked by identity, derivation, and program owner. Token accounts are checked by owner, token definition, ATA address, token-program data, and authorization. Timed actions use the trusted chain clock.

The treasury collateral ATA is derived from the current treasury and the sale's collateral definition. Clients prepare a missing ATA and refresh its destination after a treasury change. When one account has several roles, each payment occurs once.

Curve custody covers `real collateral reserve + outstanding fees`. LBP custody covers outstanding fees and remaining permitted payouts. Fee counters record trade fees; extra deposits remain surplus. Mint authority, custody, administration, and pause/freeze authority have separate checks. See [ADR 0028](../adr/0028-token-custody-and-freeze.md).

## Fees and collection

Fees use collateral: token1 for curve factory sales. Calculate `ceil(amount * rate / 10_000)` with checked integer arithmetic. Track outstanding and total collected fees separately from reserves and creator claims.

| Trade | Fee base | Pricing and payout |
| --- | --- | --- |
| Curve buy | Gross collateral input | Price the input after the fee; add that net input to the real reserve |
| Curve sell | Gross collateral output | Pay the participant after the fee; remove gross output from the real reserve |
| LBP buy | Full collateral input | Price the full input; record the fee as a creator-borne claim on proceeds |

Execution checks amounts, rounding, slippage, and custody coverage. A rate change affects later trades; accrued amounts stay fixed.

Collection is available anytime. It transfers all outstanding fees, clears that counter, and adds the transferred amount to total collected fees. A zero balance returns success with unchanged counters. Repeating collection has the same result until a trade adds new fees.

Terminal settlement collects the remaining fees in the same transaction as the related reserve or proceeds payout. The payout uses the balance after collection. If that stage fails, its state and balances stay unchanged. Earlier stages remain complete. The factory workflow can span transactions, with completed payouts and burns recorded for safe retries.

LBP creator withdrawal waits for the original end timestamp. Pause retains the schedule and weight progression. See [ADR 0015](../adr/0015-fee-accounting.md), [ADR 0016](../adr/0016-lbp-live-fees.md), and [ADR 0020](../adr/0020-settlement-and-graduation.md).

The [client batch helper](../../crates/launchpad-client/src/lib.rs) collects from curve sales in sequence, with one transaction per sale. Each call checks the namespace and current treasury. It stops at the first error; earlier confirmed transactions stay complete. It returns hashes when all calls succeed. Retries use current state and transaction receipts.

## Worked balances

### Curve

Each example starts from a separate pool: launch reserve 800, real collateral 100, virtual reserves 1,000 and 100. The fixed invariant is `k = 100,000`; the rate is 1,000 basis points. These values come from the [pool tests](../../crates/pool/src/lib.rs).

| Action | Participant result | Real launch | Real collateral | Physical collateral | Outstanding fees | Collected fees |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Buy with 25 collateral | Fee 3; price 22; receive 180 tokens | 620 | 122 | 125 | 3 | 0 |
| Collect after buy | Treasury receives 3 | 620 | 122 | 122 | 0 | 3 |
| Sell 250 tokens | Gross output 20; fee 2; receive 18 | 1,050 | 80 | 82 | 2 | 0 |
| Collect after sell | Treasury receives 2 | 1,050 | 80 | 80 | 0 | 2 |

Repeated collection leaves the final balances unchanged.

### LBP

This accounting example starts with pricing collateral `P = 100` and physical collateral `V = 100`. Token output is supplied by a valid quote.

| Action | P | V | Outstanding fees | Collected fees |
| --- | ---: | ---: | ---: | ---: |
| Buy 25 at 1,000 bps | 125 | 125 | 3 | 0 |
| Change treasury to B; collect 3 to B | 125 | 122 | 0 | 3 |
| Change fee to 2,000 bps; buy 40 | 165 | 162 | 8 | 3 |
| Collect the final 8 to B | 165 | 154 | 0 | 11 |

New input totals 65. Fees total 11. Net added collateral is 54. Repeated collection retains these balances.

## Events and errors

Events identify the namespace and chain location and record committed effects. History uses the existing indexer and namespace-scoped reads.

| Event | Required fields |
| --- | --- |
| Namespace creation | Initial administrator, fee, treasury |
| Fee or treasury update | Setting, old value, new value |
| Admin transfer or renunciation | Lifecycle action, affected administrator identities |
| Trade | Sale, mechanism, token roles, input/output, fee base, rate, fee, pricing/net amount, fee counters |
| Collection | Sale, collateral definition, amount, treasury, resulting fee counters |
| Settlement stage | Sale, mechanism, completed stage, fees collected, payout/burn amounts, resulting state |

Program and client errors use these stable cause categories:

| Category | Cause |
| --- | --- |
| `Unauthorized` | Missing or wrong authorization or authority proof |
| `AdministrationRenounced` | Administrative action after renunciation |
| `NamespaceMismatch` | Sale binding or namespace state mismatch |
| `AccountMismatch` | Wrong derivation, owner, token data, account layout, or clock |
| `InvalidFeeRate` | Rate outside 0 through 9,999 or not representable |
| `InvalidTreasury` | Default or unsupported treasury; wrong treasury ATA |
| `InvalidSaleState` | Action outside its permitted stage |
| `InvalidTradeAmount` | Invalid amount or rounding result |
| `SlippageExceeded` | Execution exceeds the quote limit |
| `ArithmeticFailure` | Unsafe arithmetic or counter update |
| `InsufficientCustody` | Balance below fees, output, or remaining claims |

## Open points and verification

- Shared namespace schema and cross-factory account ownership need a design choice.
- Separate setting operations and the selected administration lifecycle need implementation. The PoC uses NFT administration and a combined update. The selected module needs a supported current-administrator getter and dispatcher integration checks.
- Live LBP fees and anytime collection remain `To be reviewed by Logos team`. They differ from the RFP's creation-time rate and end-time collection.
- LBP pricing state, rounding, initial collateral ownership, payouts, and coverage after early collection need proof. The balance example defines accounting only; the remaining 154 includes the initial 100.
- Platform tests must verify custody, ATAs, failed-stage rollback, clock reads, and pause/freeze authority and instruction gates.
- Event emission, field types, encoding, and error mapping need implementation and platform checks. Privacy review must check that records exclude persistent private participant identifiers. Public sale effects remain visible. See [ADR 0027](../adr/0027-events-and-history.md).
- Batch progress and partial-failure reporting need a client interface.

Acceptance checks cover namespace isolation, fee limits, setting changes, transfer/renunciation, live settings, repeated collection, surplus, failed settlement, and batch retries. The [curve host tests](../../crates/curve-core/src/tests.rs) cover PoC behavior, including fee limits and lowering a legacy 100% fixture. Production administration and LBP need their own checks.

Record target versions, commands, and results for guest and platform tests. See the [workspace guide](../architecture/workspace-guide.md) for execution checks. Final review must compare this contract with mechanism rules, client behavior, and platform evidence.
