# Private purchase

This is the required SDK-managed purchase route, not a verified implementation. Deshield, public purchase and private returns execute in one transaction. Received tokens, unused input and unused gas end in participant-controlled private accounts; actual fees are excluded. The imported curve-buy guest does not complete this contract. Public pool state and trade effects remain visible. See the [private participation and recovery specification](../private-participation.md).

## Preconditions

- The participant has selected the private route and explicit trade limits.
- The sale is open and the requested trade can pass current mechanism checks.
- Sources and private return targets have compatible account types, token definitions and participant control.
- An allowlisted fixed-price or weighted sale buy satisfies the reviewed product policy; its proof method and cross-route enforcement still need verification.
- LP-0021 provider integration awaits SDK development. Its final API and support for the combined transaction and recovery remain unavailable for verification.

```mermaid
sequenceDiagram
    actor Buyer
    participant App as Configured-sales app
    participant SDK as Shared core and SDK
    participant Wallet as LP-0021 wallet provider
    participant LEZ as LEZ execution

    Buyer->>App: Choose private purchase and trade limits
    App->>SDK: Prepare private purchase
    SDK->>Wallet: Request account access
    Wallet->>Buyer: Request account selection and access approval
    Buyer->>Wallet: Select accounts and approve access
    Wallet-->>SDK: Approved account information if supported
    SDK->>SDK: Validate targets, obtain fresh state and quote
    SDK-->>App: Show quote, limits, fees and public effects
    SDK->>SDK: Construct combined transaction and record attempt
    SDK->>SDK: Persist submission boundary before an operation that can broadcast
    SDK->>Wallet: Propose combined private purchase
    Wallet->>Buyer: Request transaction approval
    Buyer->>Wallet: Approve transaction
    Note over Wallet,LEZ: Whole-route support and failure behavior remain unverified
    Wallet->>LEZ: Sign and submit one transaction if supported
    rect rgb(240, 240, 240)
        Note over LEZ: Application changes must commit together
        LEZ->>LEZ: Fund fresh public accounts with assets and gas
        LEZ->>LEZ: Execute public purchase with trade limits and eligibility checks
        LEZ->>LEZ: Re-shield received tokens, unused input and unused gas
    end
    LEZ-->>Wallet: Transaction outcome when established
    Wallet-->>SDK: Report original attempt outcome
    alt Confirmed
        SDK-->>App: Show confirmed purchase and wallet synchronization status
    else Known rejected
        SDK-->>App: Show rejection and actual charges; require fresh preparation before retry
    else Pending or unknown
        SDK-->>App: Show unresolved outcome and block replacement
        SDK->>Wallet: Reconcile original attempt when access is available
    end
```

## Notes

The provider API is not selected in this diagram. If one provider operation can obtain approval and broadcast, persist the submission boundary before invoking it. A crash or lost response after that boundary leaves an unknown outcome unless provider evidence establishes otherwise. A known refusal before broadcast is different from a missing result.

Funding includes deshield. Internal calls run in order within the same transaction. Required application changes must commit together or fail without a partial purchase or return result. Actual transaction fees may still be charged after a rejected execution. Verify whole-route execution, late-return failure and fee behavior on the target platform.

There is no separately committed purchase followed by a re-shield retry. An unknown outcome blocks a replacement purchase until reconciliation. A confirmed transaction with pending private wallet synchronization must not be repeated. No automatic fee refund, reversal or unverified recovery guarantee is added.

The SDK owns construction and progress. The provider owns account access, approval, signing and submission. Core progress records are participant-local and exclude keys, private-account identifiers and proof secrets. The shared core does not hold user keys.

## Open implementation details

The [platform research](../../research/private-route-transaction-patterns.md) records the current route's call-budget, gas-source ownership and missing-return gaps. The selected provider must support all required accounts, approval, confirmation and recovery. Private curve sell and fixed-price and weighted sale runtime implementation remain future work; allowlist cryptographic construction is still open. See [ADR 0018](../../adr/0018-private-participation.md), [ADR 0021](../../adr/0021-modes-and-open-limits.md) and [ADR 0029](../../adr/0029-wallet-integration.md).
