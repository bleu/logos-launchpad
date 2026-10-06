# ADR 0028: Token custody and freeze boundaries

Status: Accepted; platform compatibility and instruction gates remain unverified

Date: 2026-09-29

## Context

Token or mint authority, ATA vault ownership, namespace administration and pause or freeze controls are distinct capabilities. An administration extension alone does not provide token custody or freeze support.

## Decision

Use LP-0013 platform mechanisms for token and mint authority and associated token-account vault ownership. Keep namespace administration under [ADR 0006](0006-namespaces-and-authority.md), separate from creator authority and token custody.

Treat RFP-002 pause and freeze integration as a separate concern. Reuse platform mechanisms where compatible, but verify compatibility and explicit instruction gates on the selected platform. Do not claim automatic global freezing is the selected policy. Required settlement and recovery paths must remain available unless a reviewed policy explicitly gates them.

## Consequences

Implementation must separately test authority changes, vault custody, namespace administration, pause and freeze behavior. A successful admin-library integration does not prove token custody or freeze compatibility.

The [historical authority experiment](https://github.com/bleu/logos-launchpad/blob/f1198315569cfc8ada9f6c39964cb77314e2520c/packages/platform-probes/authority/src/lib.rs) used freeze `a4743468bd3a87093931c48c965cc084aa4690c3` (v0.1.5) with the component stack recorded in [ADR 0006](0006-namespaces-and-authority.md). It checked fixed emergency authority, namespace isolation and swaps-only freeze admission. Fee collection, settlement and creator recovery remained available under their own checks. These were mock operations with saved creator/custody references, not real swaps or token transfers. Guest checks used controlled authorization flags and development receipts. The experiment is no longer maintained; lasting coverage belongs with the real curve controls. LBP integration remains unverified.

## Sources

- [Curve proposal](https://github.com/logos-co/rfp/issues/118)
- [LBP proposal](https://github.com/logos-co/rfp/issues/119)
