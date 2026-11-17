# Create a curve pool

This is the planned logical curve-sale creation flow. It shows the reviewed staged creation model without defining instruction names, account schemas or transaction construction. LP-0021 provider integration awaits SDK development; its final API is unavailable. See [ADR 0029](../../adr/0029-wallet-integration.md).

## Preconditions

- The creator has the required creator authority and supplies `D` plus any separate creator allocation.
- The target namespace exists with its per-namespace administration and configuration state.
- Curve creation inputs are typed and immutable after acceptance. Exact input bounds still need mechanism specifications.
- LP-0021 provider integration awaits SDK development; its final API is unavailable.

```mermaid
sequenceDiagram
    actor Creator
    participant App as Configured-sales app
    participant SDK as Shared core and SDK
    participant Wallet as LP-0021 wallet provider
    participant Factory as Curve factory
    participant Namespace as Namespace state
    participant Pool as Curve pool and token custody
    participant Indexer as Existing indexer
    participant ReadAPI as Backend module read API

    Creator->>App: Configure curve sale
    App->>SDK: Validate and prepare creation
    SDK->>Wallet: Request creation transaction approval
    Note over Wallet: Account selection user approval signing and submission follow LP-0021. Integration awaits SDK development
    Wallet->>Factory: Submit creation stage if supported
    Factory->>Namespace: Check namespace binding and creation settings
    Namespace-->>Factory: Namespace state
    Factory->>Pool: Create pending pool in factory custody
    Pool-->>Factory: Pending pool created
    Factory-->>Wallet: Creation stage result
    Wallet-->>SDK: Result from LP-0021 provider if available
    SDK-->>App: Show pending creation status
    Creator->>App: Confirm allocation funding
    App->>SDK: Prepare funding stage
    SDK->>Wallet: Request funding transaction approval
    Wallet->>Factory: Submit funding stage if supported
    Factory->>Pool: Fund D and activate when valid
    Note over Factory: Save immutable payout terms; keep creator allocation separate
    Pool-->>Factory: Active pool state
    Factory-->>Indexer: Emit creation events
    SDK-->>App: Show creation result
    App->>ReadAPI: Request indexed sale status
    ReadAPI->>Indexer: Read indexed creation events
    Indexer-->>ReadAPI: Indexed sale status
    ReadAPI-->>App: Return indexed sale status
```

## Notes

The curve factory validates the creation namespace and retains curve sale policy. The namespace administrator does not approve every creator action. The curve pool remains under factory program custody.

This workflow can span transactions. A pending curve pool cannot trade before funding completes. Each curve stage rechecks the relevant authority and commits atomically with its child calls. The full workflow is not claimed to be atomic.

The backend module read API exposes indexed status after events reach the existing indexer. It can lag. The factory and pool enforce authoritative current state.

## Open details

The final instruction names, account schema, transaction construction and input limits need implementation and platform review. LP-0021 integration awaits SDK development and its final API is unavailable. Closing and unsold-`D` rules are defined in [ADR 0025](../../adr/0025-sale-closing.md).
