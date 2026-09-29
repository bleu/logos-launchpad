# Private purchase

This is the planned staged purchase route. Private curve sells, private LBP buys and allowlist proof details still need specifications. Public pool state and trade effects remain visible. LP-0021 provider integration awaits SDK development; its final API is unavailable, and private deshield, re-shield and recovery support remain unverified. See [ADR 0029](../../adr/0029-wallet-integration.md).

## Preconditions

- The buyer has selected the planned private route and explicit trade limits.
- The sale is open and the requested trade can pass current on-chain checks.
- LP-0021 provider integration awaits SDK development; its final API is unavailable. Private deshield, re-shield and recovery support remain unverified.

```mermaid
sequenceDiagram
    actor Buyer
    participant App as Configured-sales app
    participant SDK as Shared core and SDK
    participant Wallet as LP-0021 wallet provider
    participant Sale as Public sale mechanism

    Buyer->>App: Choose private purchase and trade limits
    App->>SDK: Request staged purchase
    SDK-->>App: Disclose public effects and separate stages
    Buyer->>App: Confirm route
    App->>SDK: Continue
    SDK->>Wallet: Request deshield of assets and gas
    Note over Wallet: LP-0021 requires account selection user approval signing and submission; private-route support remains unverified
    Wallet-->>SDK: Report funded public account if supported
    SDK->>Wallet: Hand off public purchase transaction with limits
    Wallet->>Sale: Submit public purchase if supported
    Sale-->>Wallet: Purchase result
    Wallet-->>SDK: Report purchase result if supported
    SDK->>Wallet: Request re-shield of purchased tokens
    alt Re-shield succeeds
        Wallet-->>SDK: Confirm shielded receipt if supported
        SDK-->>App: Purchase route complete
    else Re-shield interrupted
        SDK-->>App: Re-shield remains incomplete
        Buyer->>App: Resume recovery
        App->>SDK: Resume purchased-token re-shield
        SDK->>Wallet: Request re-shield retry
    end
```

## Notes

The first stage must deshield assets and gas together. The route has separate stages, so a later stage can fail after an earlier stage succeeds. The diagram shows a successful public purchase followed by either successful or interrupted re-shield.

Recovery only resumes purchased-token re-shield. It does not promise automatic refunds, trade reversal or recovery for every failure. The SDK is planned to coordinate the route through the LP-0021 provider once developed and integrated. The provider specification covers account selection, user approval, signing and submission; private deshield, re-shield and recovery support remain unverified. The shared core does not custody user keys.

## Open details

LP-0021 is the selected provider integration; its final API is unavailable. Private deshield, re-shield and recovery support remain unverified. Private curve sells, private LBP buys and allowlist proofs require separate specifications. See [ADR 0018](../../adr/0018-private-participation.md) and [ADR 0029](../../adr/0029-wallet-integration.md).
