# Create an LBP pool

This is the planned logical LBP-sale creation flow. It keeps LBP factory and pool responsibilities separate from the curve mechanism. It does not define instruction names, account schemas, transaction count or transaction atomicity. LP-0021 provider integration awaits SDK development; its final API is unavailable. See [ADR 0029](../../adr/0029-wallet-integration.md).

## Preconditions

- The creator has the required creator authority and sale funding.
- The target namespace exists with its per-namespace administration and configuration state.
- The creator supplies typed LBP inputs for weighted or fixed-price mode and the applicable schedule.
- LP-0021 provider integration awaits SDK development; its final API is unavailable.

```mermaid
sequenceDiagram
    actor Creator
    participant App as Configured-sales app
    participant SDK as Shared core and SDK
    participant Wallet as LP-0021 wallet provider
    participant Factory as LBP factory
    participant Namespace as Namespace state
    participant Pool as LBP pool and token custody
    participant Indexer as Existing indexer
    participant ReadAPI as Backend module read API

    Creator->>App: Configure LBP sale
    App->>SDK: Validate and prepare creation
    SDK->>Wallet: Request creation transaction approval
    Note over Wallet: Account selection user approval signing and submission follow LP-0021; integration awaits SDK development
    Wallet->>Factory: Submit creation request if supported
    Factory->>Namespace: Check namespace binding and creation settings
    Namespace-->>Factory: Namespace state
    Factory->>Pool: Create LBP pool and mechanism state
    Pool-->>Factory: LBP pool creation result
    Factory-->>Indexer: Emit creation events
    Factory-->>Wallet: Creation result
    Wallet-->>SDK: Result from LP-0021 provider if available
    SDK-->>App: Show creation result
    App->>ReadAPI: Request indexed sale status
    ReadAPI->>Indexer: Read indexed creation events
    Indexer-->>ReadAPI: Indexed sale status
    ReadAPI-->>App: Return indexed sale status
```

## Notes

The LBP factory owns LBP-specific creation validation and the sale-to-namespace binding. The LBP pool owns LBP runtime trading, custody and state rules. Namespace administration does not grant creator authority or require administrator approval for every creator action.

The curve staged workflow, pending guard and factory-custody rule do not define LBP creation. This diagram makes no claim about LBP creation stages, permissions, transaction atomicity or final custody account layout.

The backend module read API exposes indexed status after events reach the existing indexer. It can lag. The factory and pool enforce authoritative current state.

## Open details

Final creation inputs, account schema, custody arrangement and transaction construction need mechanism and platform review. LP-0021 integration awaits SDK development and its final API is unavailable. LBP lifecycle permissions need mechanism review. The [private participation contract](../private-participation.md) and [allowlist product policy](../../adr/0021-modes-and-open-limits.md) are defined; runtime implementation, enrollment tooling and proof construction still need verification.
