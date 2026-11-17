# Logos launchpad

Token sales on the Logos Execution Zone (LEZ). Creators set up sales. Participants buy tokens and, when the sale permits it, sell them back. Each namespace has its own administrator, fee settings and treasury.

## Sale types

| Sale type | Pricing | Ending policy |
| --- | --- | --- |
| Fixed-price sale | A set price in collateral. | `Scheduled`, `Depletion` or `Either`. |
| Bonding-curve sale | A price that changes with pool reserves. | `Scheduled`, `Depletion` or `Either`. |
| Weighted sale | Token balances and weights that change on a schedule. | `Scheduled` or `Either`, with a required schedule end. |

See the [product guide](docs/product/prd.md) for user flows and the [ending decision](docs/adr/0025-sale-closing.md) for policy rules.

## Current state

This is a pre-production project. Nothing is deployed to testnet or production. Public sale handlers have local tests, but these do not prove production readiness. Weighted trades need a test clock; normal builds reject them until a supported clock provider is available. Weighted math is unaudited.

The planned product has an admin app for namespace settings and a configured-sales app for creators and participants. The QML apps, wallet integration and private trading route are not ready. See the [system view](docs/architecture/system.md) and [checks and evidence limits](docs/architecture/workspace-guide.md#execution-and-evidence-limits).

## Development

Run `bash verify/check-dev.sh` for light host tests. See the [workspace guide](docs/architecture/workspace-guide.md) for tools, full checks and CI. Start with [CONTEXT.md](CONTEXT.md) for domain terms and [AGENTS.md](AGENTS.md) for contribution rules.

## Documentation

- [Product goals and user flows](docs/product/prd.md)
- [Architecture and package layout](docs/architecture/system.md)
- [Current design decisions](docs/adr/README.md)
- [Plain-English glossary](docs/glossary.md)

## Proposals

| Product | Request | Proposal |
| --- | --- | --- |
| Bonding-curve launchpad | [RFP-015](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-015-bonding-curve-launchpad.md) | [Proposal #118](https://github.com/logos-co/rfp/issues/118) |
| Fixed-price and weighted sales | [RFP-016](https://github.com/logos-co/rfp/blob/master/RFPs/RFP-016-lbp-launchpad.md) | [Proposal #119](https://github.com/logos-co/rfp/issues/119) |

Internal decisions can differ from these requests. The [ADR index](docs/adr/README.md) identifies decisions that still need Logos-team review.

## License

Project code uses the [MIT license](LICENSE). Third-party code keeps its own licenses and notices.
