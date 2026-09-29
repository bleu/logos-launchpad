# Sequence diagrams

These pages show planned logical flows. Each page contains one Mermaid sequence diagram. They do not define instruction names, account schemas, transaction counts, wallet capabilities or production behavior.

- [Create a curve pool](create-curve-pool.md)
- [Create an LBP pool](create-lbp-pool.md)
- [Private purchase](private-purchase.md)

The curve creation flow retains the reviewed staged workflow, pending guard and factory custody. The LBP flow does not inherit those details. The LP-0021 provider requires account selection, user approval, signing and submission; integration awaits SDK development, its final API is unavailable, and private deshield, re-shield and recovery support remain unverified. See [ADR 0029](../../adr/0029-wallet-integration.md).
