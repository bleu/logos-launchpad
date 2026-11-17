# Allowlist proof design

Status: Public-account batch enrollment accepted under [ADR 0021](../adr/0021-modes-and-open-limits.md). Public buy admission is implemented for all three engines through the maintained `crates/eligibility-policy` module, with signed development-execution evidence. Wallet support and complete private execution remain unverified. Native private proving was not measured.

Date: 2026-11-10

## What goes on the list?

The creator approves public trading account addresses. Participants keep their keys. Private funding and return account IDs do not go to the creator.

This applies to bonding-curve, fixed-price and weighted buys. The approved public account is the account that trades. There is no separate approval-only or private checking account. This replaces the earlier proposal to approve private accounts directly.

The full list is not published. The public address used for a purchase is visible, as are the trade amounts and timing. Its membership path is public too. The creator locks the final list commitment in the initial sale terms, before staged setup; it cannot change during setup or trading.

## Example: Alex prepares three accounts

Before the creator locks the list:

1. Alex's wallet creates public accounts A, B and C locally, without first funding or using them on-chain.
2. Alex gives the creator only those three public addresses.
3. The creator adds all three to the fixed list and delivers their membership data through a protected channel.
4. Alex's wallet checks and stores that data with the matching accounts. It keeps their keys and recovery data protected.

The wallet must verify that Alex controls the accounts and can use them on the target platform. Support for this preparation still needs verification. After membership delivery, Alex needs no per-purchase approval or service from the creator.

## How membership works

The method uses a SHA-256 Merkle tree: hashes combine approved addresses into one saved root. Each address has its own random salt and a randomly selected position. The creator delivers the salt, position and 17 sibling hashes needed to check that address against the root. The wallet can prepare this path at enrollment and reuse it; no new eligibility zero-knowledge proof is needed per buy.

The fixed tree holds up to 131,072 addresses. Its context includes a version, the actual namespace and a sale-policy ID equal to the canonical pool `AccountId` bytes. The pool address does not depend on the root, so it can be computed before the tree is built. The pool must use its stored context/root and the actual public trader, rather than trust identity or policy values claimed in the proof. Native authorization must still bind each purchase.

## Membership format

Accounts, salts and hashes are raw 32-byte values. Reject duplicate accounts during enrollment. Choose each salt with a cryptographically secure random source and select positions randomly across the full tree.

| Value | Encoding |
| --- | --- |
| Context | Version `1:u8`, namespace `[u8; 32]`, canonical pool ID `[u8; 32]`; 65 bytes |
| Allowlist policy payload | Context followed by root `[u8; 32]`; 97 bytes |
| Membership path | Index `u32` little-endian, salt `[u8; 32]`, exactly 17 sibling hashes; 580 bytes |

Hash concatenated raw bytes with SHA-256. Each domain tag below ends with one zero byte.

```text
leaf = SHA256("LP/allowlist/leaf/v1\0" || context || account || salt)
padding = SHA256("LP/allowlist/pad/v1\0" || context || index:u32 little-endian)
node = SHA256("LP/allowlist/node/v1\0" || context || left || right)
```

Start at the leaf. At each level, the low bit of the current index selects whether the current hash is the left child (`0`) or right child (`1`), then shift the index right. Reject unsupported versions, indices at or above 131,072 and malformed encodings. Production messages use fixed-size fields; Borsh enum and optional-path tags add message bytes. The [maintained verifier](../../crates/eligibility-policy/src/lib.rs) and [independent vector](../../crates/eligibility-policy/tests/fixtures/merkle-vector.json) define and check these rules.

## Earlier measurements

In the original experiment with 100,000 synthetic addresses, list construction took about 0.65 seconds and one path took about 0.14 ms to prepare. Signed local verification took about 41 ms and 260,365 cycles, below the unchanged 10,000,000-cycle gate. A later signed run using the maintained verifier adapter used 271,709 cycles; the original timings were not rerun. These were single self-execution measurements with cached local tools, not pool buys, native private proofs or latency guarantees. Guest time excluded compilation. The [research snapshot](https://github.com/bleu/logos-launchpad/blob/b17de27db81bf2f0acbae891580c8d230c9f56db/packages/eligibility-proof-lite/README.md#measured-results) preserves the full report; the experiment is not part of this program change.

## Purchase authorization

The pool checks membership and control of the actual public trader, even when a caller bypasses the app or factory. Membership data alone is insufficient. Spending from funding accounts also requires valid authority.

An approved address can make repeated or independent concurrent buys when current native and sale state permit. There is no allowlist spending cap, purchase counter or consumed allowance. A new address from the same wallet is not automatically approved. List capacity counts addresses, not people.

The [private transaction contract](private-participation.md#transaction-contract) owns funding, complete returns, wallet address selection and unresolved-attempt rules. See the [private-purchase flow](sequence-diagrams/private-purchase.md) for the planned sequence.

## Privacy limits

The creator knows that A, B and C were submitted together and can link their purchases. This is accepted for now. Reusing one address also links its purchases publicly. Different addresses do not prevent links from timing, amounts or other information.

The creator does not need Alex's private account IDs or keys. Private funding and return data must stay protected during proving and execution, outside app logs and public records. Proof creation stays local, without a remote prover or remote key custody. This privacy still needs verification on LEZ.

Public paths reveal the used address's salt, position and sibling hashes. These can reveal structure or help compare purchases. They do not guarantee that roster size or all list metadata stays hidden. Protect unused addresses with independent random salts and keep the full roster and enrollment bundles confidential. A revealed salt does not hide the already-public used address.

Account approval does not prove that each account belongs to a different person. Sharing full account keys can transfer control.

## Public execution evidence

Public pools store immutable policy and check admission for all three engines. [Workspace evidence](workspace-guide.md#execution-and-evidence-limits) covers actual-trader authority, replay rejection and complete public buys. Weighted public buys use the pinned upstream clock, which normal builds use. This evidence does not establish native private proving or deployment readiness.

## Remaining private work

Wallet preparation, protected membership delivery and recovery, native private holdings, complete private returns and full-route proof time/cost remain unverified. Their requirements and final-review gate are in [private participation](private-participation.md#evidence-still-required). Public-funded buys do not need the old private eligibility guest. No reduction in native privacy proof time is claimed.
