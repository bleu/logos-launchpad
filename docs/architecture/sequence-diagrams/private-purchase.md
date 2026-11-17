# Private purchase

This is a required route, not a verified implementation. Compatible private holdings, complete returns and wallet support remain missing. The [private participation contract](../private-participation.md) owns account, fee, privacy, progress and recovery requirements. Public admission has separate [signed evidence](../allowlist-proof.md#public-execution-evidence).

Before preparation, select the private route and explicit trade limits. The sale must permit the buy. The provider must verify control and asset compatibility for sources and private return targets. An allowlisted buy needs membership for the actual public trader; see [enrollment](../allowlist-proof.md).

```mermaid
sequenceDiagram
    actor Buyer
    participant App as Configured-sales app: planned
    participant SDK as Shared core and SDK: required route
    participant Wallet as LP-0021 provider: unverified
    participant LEZ as LEZ execution

    Buyer->>App: Choose private purchase and limits
    App->>SDK: Prepare purchase
    SDK->>Wallet: Request source and return-account access
    Wallet->>Buyer: Request account selection and access approval
    Buyer->>Wallet: Approve access
    Wallet->>Wallet: Prepare trader and reserve approved address for an allowlisted buy
    Wallet-->>SDK: Verified account information if supported
    SDK->>SDK: Read fresh state, quote and validate targets
    SDK-->>App: Show quote, limits, fees and public effects
    SDK->>SDK: Build combined transaction with local proof and record attempt
    SDK->>SDK: Persist submission boundary before any call that can broadcast
    SDK->>Wallet: Request approval and submission
    Wallet->>Buyer: Request transaction approval
    Buyer->>Wallet: Approve
    Note over Wallet,LEZ: Combined transaction and recovery support remain unverified
    Wallet->>LEZ: Sign and submit one transaction if supported
    Note over LEZ: All application changes must commit together
    LEZ->>LEZ: Deshield assets and gas
    LEZ->>LEZ: Execute public buy with limits and admission checks
    LEZ->>LEZ: Shield output, unused input and unused gas
    LEZ-->>Wallet: Original transaction outcome when established
    Wallet-->>SDK: Report original attempt outcome
    alt Confirmed
        SDK-->>App: Show success and separate wallet synchronization status
    else Known rejected
        SDK-->>App: Show rejection and actual charges. Refresh before retry
    else Pending or unknown
        SDK-->>App: Block replacement
        SDK->>Wallet: Reconcile original attempt
    end
```

A late required return failure must reject purchase application changes. Actual fees are excluded from returns and may still be charged after rejection. There is no separately committed purchase followed by a shielding retry or separate-transaction fallback.

The diagram does not select provider method names. Persist the submission boundary before an operation that can both request approval and broadcast. A lost result after that boundary is unknown until evidence establishes the outcome. Do not switch traders or repeat a confirmed purchase to recover wallet synchronization.

For required tests and remaining platform work, see [acceptance scenarios](../private-participation.md#acceptance-scenarios) and [evidence still required](../private-participation.md#evidence-still-required). The [older platform research](../../research/private-route-transaction-patterns.md) records earlier call-budget and return gaps; it does not establish current runtime limits.
