# Private purchase

This is the required SDK-managed purchase route, not a verified implementation. Deshield, public purchase and private returns execute in one transaction. Received tokens, unused input and unused gas end in participant-controlled private accounts; actual fees are excluded. The imported curve-buy guest does not complete this contract. Public pool state and trade effects remain visible. See the [private participation and recovery specification](../private-participation.md).

## Preconditions

- The participant has selected the private route and explicit trade limits.
- The sale is open and the requested trade can pass current mechanism checks.
- Sources and private return targets have compatible account types, token definitions and participant control.
- For an allowlisted buy in any pricing engine, the participant has precomputed a batch of public trading addresses, had them approved before the fixed list was locked, and received membership data. Only addresses go to the creator, without keys or private funding/return IDs. The actual public trader must prove membership and control bound to the sale, purchase and destinations. Public buy admission is implemented for all three engines with signed development authority and replay checks; the private-funded route remains unverified. The final list is locked at initial sale creation.
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
    Buyer->>Wallet: Select private sources and return accounts; approve access
    Wallet->>Wallet: Select and reserve an unused approved public trader for an allowlisted buy
    Wallet-->>SDK: Approved account information if supported
    SDK->>SDK: Validate targets, obtain fresh state and quote
    SDK-->>App: Show quote, limits, fees and public effects
    SDK->>SDK: Construct combined transaction with local proof and record attempt
    SDK->>SDK: Persist submission boundary before an operation that can broadcast
    SDK->>Wallet: Propose combined private purchase
    Wallet->>Buyer: Request transaction approval
    Buyer->>Wallet: Approve transaction
    Note over Wallet,LEZ: Whole-route support and failure behavior remain unverified
    Wallet->>LEZ: Sign and submit one transaction if supported
    rect rgb(240, 240, 240)
        Note over LEZ: Application changes must commit together
        LEZ->>LEZ: Deshield assets and gas to the selected public accounts
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

The SDK owns construction and progress. The provider owns account access, approval, signing and submission. Core progress records are participant-local and exclude keys, private-account identifiers and proof secrets. Proving stays in the protected participant-local wallet/proving boundary, without a remote prover or remote key custody. The shared core does not hold user keys.

The 2026-11-10 amendment in [ADR 0021](../../adr/0021-modes-and-open-limits.md) selects pre-approved public trading-account batches. The wallet prefers an unused approved address per attempt; this is a privacy rule, not an on-chain one-buy limit. If none remain, report that limit. Do not silently reuse an address or select an unapproved one. Deliberate reuse needs a privacy warning. The creator can link batch addresses, public reuse links purchases, and fresh addresses do not guarantee unlinkability.

Repeated and independent concurrent authorized buys remain allowed, with no allowlist cap, purchase counter or one-buy nullifier. Current pool state and native funding checks still apply. New purchases remain distinct from recovery: do not rotate to another address as a replacement while the original outcome is unknown. Native private-state nullifiers and transaction replay checks remain required. Membership data is delivered through a protected channel before buying; purchases need no per-buy creator service.

## Open implementation details

The [platform research](../../research/private-route-transaction-patterns.md) records the current route's call-budget, gas-source ownership and missing-return gaps. The selected provider must support all required accounts, approval, confirmation and recovery. Private curve sells and private fixed-price and weighted sale buys remain future work. The [allowlist design](../allowlist-proof.md) records the current account model; [public execution evidence](../../../plans/allowlist-program-integration.md#public-execution-evidence) covers implemented admission, native `Execute` authority and replay rejection. Weighted public buys use the pinned upstream clock, with the limits in [ADR 0026](../../adr/0026-lbp-clock-and-poke.md). These public checks do not establish the complete private transaction. Native privacy proving remains required for this private-funded route. See [ADR 0018](../../adr/0018-private-participation.md), [ADR 0021](../../adr/0021-modes-and-open-limits.md) and [ADR 0029](../../adr/0029-wallet-integration.md).
