# Logos launchpad

Token sales on the Logos Execution Zone (LEZ), with two ways to set the sale price: a bonding curve and a liquidity bootstrapping pool (LBP).

The goal is to let creators launch sales and participants trade through clear, reusable tools. Independent namespaces give launchpad operators their own administration, fee settings and treasury. A shared core supports both launch mechanisms without making them use the same sale rules.

This repository currently contains design documentation; the production code and apps are not available yet.

## Two sale mechanisms

| Mechanism | How it works | When it ends |
| --- | --- | --- |
| Bonding curve | Reserve-based pricing changes as participants buy or sell. Selling back is available when the sale mode permits it. | When all sale tokens are purchased, or at an optional expiry. |
| LBP | Pool weights change over a chosen time schedule to support price discovery. Fixed-price mode is also planned. | At the scheduled end time. |

## Product view

Two Basecamp QML mini-apps serve different users. The admin app handles namespace administration only. The configured-sales app handles creator and participant flows within a selected namespace. Both use a shared core module; it is not a third mini-app. App placement does not grant or merge authority roles.

```mermaid
flowchart TB
    Admin[Namespace administrator] --> AdminApp[Admin app]
    Creator[Sale creator] --> SalesApp[Configured-sales app]
    Participant[Participant] --> SalesApp[Configured-sales app]
    AdminApp --> Core[Shared core and SDK]
    SalesApp --> Core
    Core --> Curve[Bonding-curve sales]
    Core --> LBP[LBP sales]
```

This is the planned product view. Participants will be able to inspect sales, get quotes, choose slippage limits and view trade history. The SDK design includes a staged private purchase route, while pool state and trade effects remain public. Private curve sells and private LBP buys still need further design.

## RFPs and proposals

| Product | Request | Proposal |
| --- | --- | --- |
| Bonding-curve launchpad | [RFP-015](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-015-bonding-curve-launchpad.md) | [Proposal #118](https://github.com/logos-co/rfp/issues/118) |
| LBP launchpad | [RFP-016](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md) | [Proposal #119](https://github.com/logos-co/rfp/issues/119) |

## Read more

- [Product goals and use cases](docs/product/prd.md)
- [Architecture and interfaces](docs/architecture/system.md)
- [Creation and private-route sequences](docs/architecture/sequence-diagrams/README.md)
- [Planned repository layout and checks](docs/architecture/repository-layout.md)
- [Domain terms](CONTEXT.md)
- [Plain-English glossary](docs/glossary.md)
- [Architecture decisions](docs/adr/README.md)
- [Agent instructions](AGENTS.md)
