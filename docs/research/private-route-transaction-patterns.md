# Private route: one transaction or three

Date: 2026-10-01. This is source research, not proof that the route works. No builds, tests or live transactions were run for this research.

The findings use LEZ revision `a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a`, pinned by the application at the time. They are historical and do not describe the selected actor revision. The [actor migration](../adr/0034-lez-v04-migration.md) replaces private helper execution with explicit unsupported errors because temporary token support has no private holdings. Code links below refer to the pre-migration implementation. The selected product rules remain in [ADR 0018](../adr/0018-private-participation.md).

## What changes when we split the transaction?

| Flow | What happens | Failure risk |
| --- | --- | --- |
| One transaction | Move private funds to fresh public accounts, trade, then return funds to private accounts. All application changes must finish together. | The historical route exceeds the reviewed call limit. Complete returns and wallet support were not established. |
| Three transactions | Fund public accounts, trade, then return funds in separate transactions. | A failed trade can leave the input public. A failed return can leave purchased tokens public. |

LEZ checks the whole transaction before applying its application changes. Three submissions have three separate completion points. One wallet approval does not make them one transaction. [State checks][state], [wallet submission][wallet].

Both flows expose public trade amounts and pool changes. Three transactions also leave public balances between steps. Fresh accounts reduce address reuse, but do not guarantee that trades cannot be linked. [Public transaction data][message], [official transfer guide][visibility].

## Historical call-limit failure

The reviewed private proving code allows 10 total program runs, including the first program. The historical buy needs 14:

| Work | Program runs |
| --- | --- |
| Start the private route | 1 |
| Fund gas | 1 |
| Create the two public token accounts | 4 |
| Move collateral to the public account | 1 |
| Trade, including token transfers | 5 |
| Return purchased tokens to the private account | 2 |
| Total | 14 |

The 11th run is rejected. The limit counts all runs, not just nested calls. The [existing test](https://github.com/bleu/logos-launchpad/blob/5b6927cbd3b5a12941f3956bcab8458446ed52db/crates/launchpad-client/tests/private_authority.rs#L401-L410) expects this failure; it does not show a successful purchase. [Private proving code][runner], [historical route](https://github.com/bleu/logos-launchpad/blob/5b6927cbd3b5a12941f3956bcab8458446ed52db/methods/guest/src/bin/private_buy.rs#L108-L189), [trade calls](https://github.com/bleu/logos-launchpad/blob/5b6927cbd3b5a12941f3956bcab8458446ed52db/crates/curve-core/src/dispatch.rs#L562-L585).

Code review found two more gaps:

- The route uses the same private source for collateral and gas. That account belongs to the token program, but a different program tries to spend its native balance. LEZ requires the spending program to own that account. This needs a separate test; the call-limit failure occurs first. [Funding code](https://github.com/bleu/logos-launchpad/blob/5b6927cbd3b5a12941f3956bcab8458446ed52db/methods/guest/src/bin/private_buy.rs#L61-L114), [native transfer][native], [ownership check][owner].
- Only purchased tokens are returned privately. Unused collateral and gas have no return calls. These are gaps in the route, not funds stranded by the failing test. [Return code](https://github.com/bleu/logos-launchpad/blob/5b6927cbd3b5a12941f3956bcab8458446ed52db/methods/guest/src/bin/private_buy.rs#L134-L189).

The newer inspected LEZ revision, `c31a6bca60537ec08f595cf786f1ae6275ab7f43`, allows 11 total runs. That still does not fit 14 or prove compatibility with this client. [Newer call-limit test][new-limit].

## What still needs testing

A smaller route with a separate, correctly owned gas source may fit. It must still return purchased tokens, unused input and unused gas. Removing checks or splitting the transaction would not meet the selected rules.

Tests must show that the whole route succeeds, and that a failed final return leaves no partial purchase. They must also check wallet approval, fees, app restart and a lost submission response. See the [required test cases](../architecture/private-participation.md).

A quote does not reserve a price. Changed public state can invalidate a private proof. A timeout does not prove failure, so the client must check the original result before sending a replacement trade. A confirmed purchase with delayed wallet updates must not be repeated. [State checks][state], [wallet submission and updates][wallet].

Application rollback does not promise a fee refund. The newer public-transaction code can charge fees even when application changes fail. Private-route fee behavior still needs version-specific checks. [Newer fee handling][fees].

LP-0021 lists wallet requirements. It does not establish a released API that supports this whole route and safe recovery. See [ADR 0029](../adr/0029-wallet-integration.md). [Provider requirements][provider].

[state]: https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lee/state_machine/src/state.rs#L197-L308
[wallet]: https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lez/wallet/src/lib.rs#L511-L783
[message]: https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lee/state_machine/src/privacy_preserving_transaction/message.rs#L14-L24
[visibility]: https://github.com/logos-co/logos-docs/blob/ef4aaf0e11bed9685c04a50fd2c8a783be2a1afe/docs/lez/transfer-tokens/transfer-native-tokens-on-the-logos-execution-zone.md#L18-L148
[runner]: https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lee/state_machine/src/privacy_preserving_transaction/circuit.rs#L67-L148
[native]: https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lez/programs/authenticated_transfer/src/main.rs#L22-L58
[owner]: https://github.com/logos-blockchain/logos-execution-zone/blob/a58fbce2ff48c58b7bb5001b1a27e64b9596ee3a/lee/state_machine/core/src/program.rs#L663-L751
[new-limit]: https://github.com/logos-blockchain/logos-execution-zone/blob/c31a6bca60537ec08f595cf786f1ae6275ab7f43/lee/state_machine/core/src/execution_state/tests.rs#L1202-L1216
[fees]: https://github.com/logos-blockchain/logos-execution-zone/blob/c31a6bca60537ec08f595cf786f1ae6275ab7f43/lez/chain_state/src/apply.rs#L378-L500
[provider]: https://github.com/logos-co/lambda-prize/blob/7a52d8f011c28858bdedc6eaa9be0250cdcfc033/prizes/LP-0021.md#L11-L64
