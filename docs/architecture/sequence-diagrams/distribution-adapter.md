# Distribution adapter flow

[ADR 0036](../../adr/0036-graduation-adapter-delivery.md) defines configuration, rounding and custody rules. [ADR 0020](../../adr/0020-settlement-and-graduation.md) defines factory settlement.

```mermaid
sequenceDiagram
    actor Caller
    participant Factory
    participant Adapter
    participant Token
    Caller->>Adapter: Initialize sale identity and kind
    Adapter->>Factory: Read saved state
    Factory-->>Adapter: Saved hook and assets
    Adapter->>Adapter: Bind configuration and identities
    Caller->>Factory: Execute graduation after close
    Factory->>Token: Pay creator and adapter shares
    Factory->>Token: Pay recorded R from mint custody to adapter
    Factory->>Adapter: Graduate with R, unsold share and combined total
    Adapter->>Token: Read full custody balances
    Adapter->>Token: Prepare and read recipient holdings
    Adapter->>Token: Transfer saved percentage payouts
    Adapter->>Token: Check custody and recipient balances
    Adapter->>Adapter: Save receipt and actual amounts
    Adapter-->>Factory: Return request hash
    Factory->>Factory: Complete graduation
    Note over Factory,Token: Final payouts, distribution and callback commit together
    Caller->>Adapter: DistributeRemaining after graduation
    Adapter->>Token: Read full balances and distribute by saved rules
    Adapter->>Adapter: Verify balances and emit distribution event
    Note over Adapter,Token: Each later call commits or rolls back together
```
