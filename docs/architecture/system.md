# System overview

This is the planned architecture. The production code is not built yet.

## Components

```mermaid
flowchart TB
    Admin[Admin app] --> Core[Shared core and SDK]
    Sales[Sales app] --> Core
    CLI[CLI] -->|SDK calls| Core
    Core -->|State and transaction requests| LEZ[LEZ access]
    Core -->|Read listings and history| Indexer[Existing indexer]
    Core -.->|Awaiting development| Wallet[LP-0021 Wallet Provider SDK]

    subgraph Programs[On-chain components]
        Namespace[Namespace settings]
        CurveFactory[Curve factory] -->|Create and manage| CurvePool[Curve pool]
        LBPFactory[LBP factory] -->|Create| LBPPool[LBP pool]
        CurveFactory -.-> Namespace
        LBPFactory -.-> Namespace
        CurvePool -.-> Namespace
        LBPPool -.-> Namespace
    end

    LEZ -->|Namespace administration| Namespace
    LEZ -->|Curve creation and settlement| CurveFactory
    LEZ -->|Curve trades| CurvePool
    LEZ -->|LBP creation| LBPFactory
    LEZ -->|LBP operations| LBPPool
    Programs -->|Events| Indexer
    LBPPool -.->|Library calls| Math[Weighted math]
```

The admin app manages namespaces. Creators and participants use the sales app. Both apps read indexed data through the shared core. The CLI calls the SDK directly.

The indexer provides listings and history. Its data can lag behind the chain. Programs check current state before they accept an action. Dashed lines show dependencies or an open integration.

## Flows

- [Create a curve pool](sequence-diagrams/create-curve-pool.md)
- [Create an LBP pool](sequence-diagrams/create-lbp-pool.md)
- [Make a private purchase](sequence-diagrams/private-purchase.md)

## Details

- [Factory and pool boundaries](../adr/0005-mechanism-boundaries.md)
- [Namespaces and authority](../adr/0006-namespaces-and-authority.md)
- [App packages and interfaces](../adr/0019-product-packages.md)
- [Wallet Provider SDK integration](../adr/0029-wallet-integration.md)
- [Package layout and checks](repository-layout.md)
- [Glossary](../glossary.md)
