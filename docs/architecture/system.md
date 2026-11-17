# System overview

Public setup, trading, controls, fee collection, settlement and graduation use the selected LEZ actor interface. Local checks do not establish live readiness. The QML apps and shared core are package placeholders; wallet, indexed reads and private routes remain incomplete. See [evidence limits](workspace-guide.md#execution-and-evidence-limits).

## Components

```mermaid
flowchart TB
    Admin[Admin app: planned workflows] -.-> Core[Shared core: planned SDK and read boundary]
    Sales[Configured-sales app: planned workflows] -.-> Core
    Core -.-> Wallet[LP-0021 wallet provider: pending integration]
    Core -.-> Indexer[Existing indexer: pending integration]
    Core -.-> Client[Launchpad client]
    CLI[CLI] --> Client
    Client --> LEZ[LEZ actor execution]

    subgraph Programs[Public program boundary]
        Namespace[Namespace settings and separate role authorities]
        Factory[Shared launch factory]
        Curve[Curve pool]
        LBP[Fixed-price and weighted pools]
        Factory --> Curve
        Factory --> LBP
        Factory --> Namespace
        Curve --> Namespace
        LBP --> Namespace
    end

    LEZ --> Factory
    LEZ --> Curve
    LEZ --> LBP
    LEZ --> Namespace
    LBP --> Math[Weighted math: unaudited]
    Programs -.->|Event delivery unverified| Indexer
```

Solid lines show implemented code boundaries. Dashed lines show pending integration. The factory shares setup and policies, not pricing state or custody. Weighted trades need a [supported clock provider](workspace-guide.md#temporary-weighted-pricing-clock). A successful graduation callback does not prove DEX liquidity.

The admin app is for namespace administration. The configured-sales app is for creators and participants. Planned QML reads go through the shared-core backend module API. Indexed data can lag; programs enforce current state.

## Flows and contracts

- [Creation and private-purchase flows](sequence-diagrams/README.md)
- [Shared launch setup and policies](../adr/0031-shared-launch-factory.md)
- [Namespace and role authority](../adr/0006-namespaces-and-authority.md)
- [Settlement and graduation](../adr/0020-settlement-and-graduation.md)
- [Package responsibilities](repository-layout.md)
- [Private participation and recovery](private-participation.md)
