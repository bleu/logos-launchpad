# Allowlist program integration

Status: Public program slice complete with local development evidence, 2026-11-10. Wallet support and complete private execution remain incomplete. See [ADR 0021](../docs/adr/0021-modes-and-open-limits.md) for the rules and the [design guide](../docs/architecture/allowlist-proof.md) for the account model and membership format. The broader fee/ending-policy refactor remains separate in [ADR 0031](../docs/adr/0031-shared-launch-factory.md) and LOG-54.

## Public execution evidence

After merging current main, the locked signed actor suite passes all 32 tests. It covers fresh factory setup for all three engines, both buy forms, actual-trader control, missing and copied membership, changed signed messages, admission-only Key/PDA attempts, native replay, repeat buys and full state rollback on late output failure. Authority relies on the current narrow forwarding rule: only authorized `Execute` forwards buys to another pool. Preserve this rule when adding messages.

Complete allowlisted buys passed the unchanged 10,000,000-cycle budget:

| Engine | Exact input | Exact output | Buy after fresh setup |
| --- | ---: | ---: | ---: |
| Bonding curve | 658,565 | 658,680 | 665,182 |
| Fixed price | 758,814 | 758,859 | 758,828 |
| Weighted, fake clock | 838,557 | 840,447 | 870,899-874,364 |

Verification passed offline with locked dependencies: `verify/check-dev.sh`, IDL generation and drift checks, 104 client tests, CLI compilation, `verify/check-weighted-clock.sh`, the signed actor suite, release overflow checks, scoped strict Rust lint and formatting. The 31 ELF-dependent client guest tests remain ignored; signed execution evidence comes from the separate actor suite. Local document links and whitespace checks pass. No native private proof or deployment was run.

These are local signed public executions, not native privacy proofs or deployment evidence. Weighted setup covers both allocation-release choices; normal weighted builds still lack a supported clock provider. The locked host regression script passes with unchanged lockfiles. Public rollback does not establish the separate private-return contract. Earlier isolated verifier measurements and their limits are in the [design guide](../docs/architecture/allowlist-proof.md#earlier-measurements).

## Remaining work

- Implement wallet account preparation, protected membership delivery/storage and target-platform usability checks. Keep keys in the protected wallet. Reserve an approved public trader per attempt and reconcile unknown outcomes before replacing an attempt or changing accounts. The final LP-0021 provider interface remains unverified.
- Add compatible native private token holdings and authority. `private-flow-core` and `launchpad-client::submit_private_buy` still report unsupported; the compatibility token layer supports public holdings only.
- Complete the [private transaction contract](../docs/architecture/private-participation.md): deshield funding, public buy and private return of output, unused input and unused gas in one transaction. Actual fees are excluded. Any required late return failure must reject purchase application changes; no separately committed buy followed by a shielding retry is allowed.
- Agree private-route test seams before implementation. Verify success for each engine, late output/input/gas-return failures, incompatible destinations, call/cycle limits, native replay/spend checks and recovery after lost results. Keep proving local and do not publish inner receipts, journals or witnesses. Then measure full-route proof time and cost.
- Add a supported real clock provider for normal weighted execution. Fake-clock tests do not establish this support.

The full private feature remains incomplete until this work is verified and its evidence and limits receive final user review. No deployment, live funds, platform-limit changes or tool installation is authorized by this plan.
