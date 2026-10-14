# Sequence diagrams

These pages show planned logical flows. Each page contains one Mermaid sequence diagram. They do not define instruction names, account schemas, wallet capabilities or production behavior. The private-purchase flow specifies one transaction; the creation flows do not define transaction counts.

- [Create a curve pool](create-curve-pool.md)
- [Create a fixed-price sale pool](create-lbp-pool.md)
- [Private purchase](private-purchase.md)

The curve creation flow retains the reviewed staged workflow, pending guard and factory custody. The fixed-price sale flow defines the implemented four setup stages. Locked local verification passed using development receipts; real proofs and deployment readiness remain pending. It does not inherit curve custody or lifecycle rules, and it does not claim fixed-price sale trading or client support. The LP-0021 provider requires account selection, user approval, signing and submission; integration awaits SDK development, its final API is unavailable, and support for the combined private transaction remains unverified. See [ADR 0029](../../adr/0029-wallet-integration.md) and [ADR 0032](../../adr/0032-fixed-price-lbp-setup.md).
