# ADR 0034: LEZ v0.4 migration and local compatibility support

Status: Accepted; application and temporary development base use one actor revision.

## Decision

Target one fixed LEZ actor-development revision. Recheck compatibility when the official v0.4 release is available. The old platform is not a second supported path. There are no testnet or production deployments, users or important stored data to migrate; breaking account and interface changes are permitted.

Keep platform adaptation separate from pricing and engine-specific lifecycle rules. Change handlers, source IDLs, generated artifacts, SDK callers and CLI behavior together. Root handlers use `handle(&ReceiveInput) -> Result<Response, Error>` and typed Borsh messages.

Use minimal temporary token, ATA and role-authority support in the separate `packages/lez-compat/` workspace. Prefer reviewed source reuse with provenance and required notices. Keep replacement conditions and requirements in the [development base guide](../../packages/lez-compat/README.md#sources-and-replacement). Use direct actor interfaces with narrow helpers rather than recreating SPEL.

Use per-definition and per-holding token actors with synchronous credits and burns. Store temporary token metadata in the definition actor. Key role successors co-sign; PDA successors require owner-program approval and runtime seed grants. Authenticate callback program and account addresses before accepting token or role proofs. Owner programs must integrate approval messages; source compatibility does not prove integration. Do not claim upstream wire equivalence.

Use supported upstream wallet, network-fee, clock and event primitives where available. Do not build a replacement wallet/provider or separate network-fee mechanism. Repository-owned build, interface-generation and verification tools can remain after runtime support is replaced.

Use ordinary host tests by default and optional node-free guest execution without a running node or local network. Keep heavy execution checks explicit and report gaps. Host tests and generated messages do not prove cross-program execution or rollback. [ADR 0022](0022-verification-boundaries.md) defines readiness requirements.

## Reason and limits

The fixed actor revision provides one development target while upstream dependencies are incomplete. Temporary support adds maintenance and needs later comparison with upstream behavior. Exact revisions, toolchains, commands and current execution evidence belong in the [workspace guide](../architecture/workspace-guide.md) and [development base guide](../../packages/lez-compat/README.md), not a second version inventory here.

Temporary tokens have no private holdings. Private purchase and authority helpers return explicit unsupported errors; they do not complete private routes. Weighted trades need the [temporary pricing clock](../architecture/workspace-guide.md#temporary-weighted-pricing-clock). Package loading, service integration, wallet workflows and real proofs remain evidence gaps. Distinguish source compatibility, host checks, guest execution, package loading and deferred checks. No migration result proves deployment or production readiness.
