# Namespace, custody, and fee contract

M0 design contract for fixed-price, bonding-curve and weighted sales. Use the terms in [CONTEXT.md](../../CONTEXT.md).

## Namespace rules

Anyone can create a namespace. Creation sets its administrator, fee, and treasury in one transaction. Each sale keeps its creation namespace. Sale inputs and token roles stay fixed. Accounts, listings, and history belong to that namespace.

| Action | Who can do it | What changes |
| --- | --- | --- |
| Change fee | Current administrator | Fee only |
| Change treasury | Current administrator | Treasury only |
| Change administrator | Current administrator and accepting successor | Administrator only |
| Give up administration | Current administrator | Remove administration permanently; keep fee and treasury |
| Create a sale or perform manual settlement | Current creator authority holder | Sale actions allowed at the current stage |
| Execute a configured curve adapter hook | Anyone | Post-close settlement under [ADR 0020](../adr/0020-settlement-and-graduation.md) |
| Collect fees | Anyone | Collect all outstanding fees for the current treasury; no self-transfer if already in its vault |

Each namespace has one administration record. The [administration module](../adr/0006-namespaces-and-authority.md) defines its setup and transfer rules. Namespace administration does not grant creator rights.

## Funds

Sale and namespace accounts must have the correct addresses and program owners. Token account checks cover the owner, token definition, expected address, token-program data, and permission to use the account.

Curve accounts must hold enough collateral for the real reserve and outstanding fees. Fixed-price and weighted sale accounts must cover outstanding fees and remaining payouts. Fee records contain trade fees; extra deposits remain surplus. Token creation, fund transfers, namespace settings, and pause/freeze controls have separate authority checks. See [ADR 0028](../adr/0028-token-custody-and-freeze.md).

## Fees and collection

Fees use collateral: token1 for curve factory sales. Namespace creation and updates permit integer rates `0..=9_999` basis points, with denominator `10_000`. Calculate `amount * rate / 10_000`, rounded up, using checked arithmetic. Keep outstanding fees and total collected fees separate from reserves and creator proceeds.

| Trade | Calculate the fee on | Pricing and payment |
| --- | --- | --- |
| Curve buy | Full collateral input | Use input after the fee for pricing and the real reserve |
| Curve sell | Collateral output before the fee | Pay the participant after the fee; remove the full output from the real reserve |
| Fixed-price or weighted sale buy | Full collateral input | Use full input for pricing; the creator pays the fee from proceeds |

Execution checks trade amounts, rounding, price limits, and available funds. A fee change affects later trades; amounts already recorded stay fixed.

Fees can be collected at any time. Collection pays all outstanding fees, clears that record, and increases total collected fees by the amount paid. If the curve pool is the current treasury, its fees are already in the destination collateral vault: clear outstanding fees and record collection without a transfer to itself. Reserve withdrawal leaves those collected fees in that vault. Collecting a zero balance succeeds and leaves the records unchanged. Repeated collection gives the same result until a trade adds new fees.

Settlement collects remaining fees in the same transaction as the related reserve or proceeds payment. The payment uses the balance after collection. If that stage fails, its state and balances stay unchanged. Earlier stages stay complete. The full process can use several transactions. Completed payments and token burns are recorded so a retry is safe.

Fixed-price and weighted sale creator withdrawal waits for the original end timestamp. A pause keeps the schedule active; weighted-sale weights continue to change. See [ADR 0015](../adr/0015-fee-accounting.md), [ADR 0016](../adr/0016-lbp-live-fees.md), and [ADR 0020](../adr/0020-settlement-and-graduation.md).

## Events and errors

Events identify the namespace and chain location and record completed changes. History uses the existing indexer and reads scoped to each namespace.

| Event | Required fields |
| --- | --- |
| Namespace creation | Initial administrator, fee, treasury |
| Fee or treasury update | Setting, old value, new value |
| Administrator change or removal | Action, affected administrator identities |
| Trade | Sale, sale type, tokens, input/output, fee base, rate, fee, amount used for pricing or paid after fees, fee records |
| Collection | Sale, collateral definition, amount, treasury, resulting fee records |
| Settlement stage | Sale, sale type, completed stage, fees collected, tokens paid or burned, resulting state |

Program and client errors use these cause categories:

| Category | Cause |
| --- | --- |
| `Unauthorized` | Missing or wrong permission or authority proof |
| `AdministrationRenounced` | Administrative action after administration was permanently removed |
| `NamespaceMismatch` | Sale or account belongs to another namespace |
| `AccountMismatch` | Wrong address, owner, token data, account layout, or clock |
| `InvalidFeeRate` | Fee outside the permitted range or too large to store |
| `InvalidTreasury` | Unset or unsupported treasury; wrong fee account |
| `InvalidSaleState` | Action outside its permitted sale stage |
| `InvalidTradeAmount` | Invalid amount or rounding result |
| `SlippageExceeded` | Trade exceeds the participant's input or output limit |
| `ArithmeticFailure` | Calculation or fee update cannot complete safely |
| `InsufficientCustody` | Balance below fees, trade output, or remaining payments |
