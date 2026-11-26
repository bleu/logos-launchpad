# Standalone LEZ checks

Run these checks on demand. Use a separate Bash shell from the Launchpad root. Use a fresh node home, a fresh wallet home and upstream test accounts. Do not connect to a shared node or use real funds. Keep the wallet storage, private keys, recovery phrase and signed transactions out of logs and commits.

## Start the node

Use the [pinned tools and emergency-authority fixture](workspace-guide.md#build-contexts). Set `LEZ_SRC` to a separate LEZ checkout at the `ACTOR_LEZ_REV` in [`verify/actor-tools.env`](../../verify/actor-tools.env). Make sure no process is listening on port 3040. If the port is in use, choose an unused port and replace 3040 in the commands and wallet config below. Build both upstream binaries and start the standalone sequencer:

```sh
scratch=$(mktemp -d)
export LEE_WALLET_HOME_DIR="$scratch/wallet"
export LEE_WALLET_PASSWORD='' SUPPRESS_VERBOSE_PRINTS=1
export RISC0_SERVER_PATH="$(command -v r0vm)" RISC0_DEV_MODE=1 RUST_LOG=warn
unset SEQUENCER_CONFIG SEQUENCER_GOSSIP_BOOTSTRAP_PEER
unset OTEL_EXPORTER_OTLP_TRACES_ENDPOINT OTEL_EXPORTER_OTLP_ENDPOINT
mkdir -m 700 "$LEE_WALLET_HOME_DIR"
CARGO_TARGET_DIR="$scratch/lez-target" cargo +1.98.1 build --manifest-path "$LEZ_SRC/Cargo.toml" \
  --locked -p sequencer_service -p wallet --features sequencer_service/standalone
node="$scratch/lez-target/debug/sequencer_service"
wallet="$scratch/lez-target/debug/wallet"
"$node" setup "$scratch/node" --config "$LEZ_SRC/lez/sequencer/service/networks/local.json" \
  --funding-public-key 2e03b2eff5a45478e7e79668d2a146cf2c5c7925bce927f2b1c67f2ab4fc0d26 --create-channel
"$node" start "$scratch/node" --rpc-listen-address 127.0.0.1:3040 \
  --metrics-listen-address 127.0.0.1:0 --gossip-listen-addr 127.0.0.1:0 &
node_pid=$!
```

Wait until the new process is running and `getLastBlockId` returns block 1 or later. To check restart, stop it with `kill "$node_pid"` and wait for it to exit. Repeat the `start` and `node_pid=$!` lines with the same node home. The node must still return its stored blocks.

```sh
kill -0 "$node_pid" && curl --noproxy '*' -fsS http://127.0.0.1:3040 -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getLastBlockId"}'
```

Copy `$LEZ_SRC/lez/wallet/configs/debug/wallet_config.json` into `$LEE_WALLET_HOME_DIR` and set `sequencer.sequencer_addr` to `http://127.0.0.1:3040`. Use `$wallet` to import the two test accounts from the [`wallet-import-test-accounts` recipe](https://github.com/logos-blockchain/logos-execution-zone/blob/55c0bf37184728556e158a9ffa987e3e9b41732b/Justfile#L181-L184). Redirect the first wallet invocation's output to `/dev/null`. It can print a recovery phrase even with `SUPPRESS_VERBOSE_PRINTS=1`. Do not copy the private keys into this repository.

## Check the Rust wallet

From the Launchpad root, set `CURVE_EMERGENCY_AUTHORITY` as shown in the [workspace guide](workspace-guide.md#build-contexts). Set the test accounts to plain base58 IDs, without `Public/`. Run one wallet test at a time so the tests do not use the same signer nonce:

```sh
export LAUNCHPAD_LOCAL_SENDER=6iArKUXxhUJqS7kCaPNhwMWt3ro71PDyBj7jwAyE2VQV
export LAUNCHPAD_LOCAL_RECIPIENT=7wHg9sbJwc6h3NP1S9bekfAzB8CHifEcxKswCKUt3YQo
cargo +1.98.1 test --locked -p launchpad-cli --test local_node wallet_ -- --ignored --nocapture --test-threads=1
```

A send response gives the admission hash. The transaction lookup gives the inclusion block; the receipt gives the execution result. A failed receipt can still cost a fee. A missing lookup or timeout leaves the outcome unknown. If the CLI cannot confirm inclusion after submission, `--json` returns `error.category` as `outcome_unknown` and keeps the hash in `error.transaction_hash`. Check the receipt for that hash before submitting again. The standalone node simulates Bedrock, so its block status does not prove base-chain finality. These checks use the Rust wallet, not the Wallet Provider SDK.

## Configure a namespace

With the same test accounts and `CURVE_EMERGENCY_AUTHORITY` set, build and deploy the compatibility authority guest:

```sh
export CARGO_TARGET_DIR="$scratch/launchpad-target"
export RISC0_BUILD_LOCKED=1
unset RISC0_SKIP_BUILD LAUNCHPAD_AUTHORITY_PROGRAM_ACCOUNT
cargo +1.98.1 build --locked --manifest-path packages/lez-compat/methods/Cargo.toml
compat_elf="$CARGO_TARGET_DIR/riscv-guest/lez-compat-methods/lez-compat-guests/riscv32im-risc0-zkvm-elf/release/compat_authority.bin"
cargo +1.98.1 run --locked -p launchpad-cli --example deploy_local_guest -- "$compat_elf" --payer "$LAUNCHPAD_LOCAL_SENDER"
```

Copy `program_account_hex` into `LAUNCHPAD_AUTHORITY_PROGRAM_ACCOUNT` (64 hex characters, no prefix). Select `standalone-test` for both the curve guest and the CLI so they use the same deployed authority. Normal builds ignore the variable and keep the fixed role-authority address. Stock standalone LEZ has no program at that address.

```sh
export LAUNCHPAD_AUTHORITY_PROGRAM_ACCOUNT='<program_account_hex from the authority deployment>'
cargo +1.98.1 build --locked --features standalone-test --manifest-path methods/Cargo.toml
export LAUNCHPAD_LOCAL_CURVE_ELF="$CARGO_TARGET_DIR/riscv-guest/launchpad_methods/launchpad_programs/riscv32im-risc0-zkvm-elf/release/curve.bin"
cargo +1.98.1 run --locked -p launchpad-cli --example deploy_local_guest -- "$LAUNCHPAD_LOCAL_CURVE_ELF" --payer "$LAUNCHPAD_LOCAL_SENDER"
```

Use the `program_account` value from the curve deployment. It is a base58 address. The test runs `launchpad configure`, checks the execution receipt, and reads the saved namespace, fee and treasury with `launchpad namespace-info`.

```sh
export LAUNCHPAD_LOCAL_CURVE_ACCOUNT='<program_account from the curve deployment>'
export LAUNCHPAD_LOCAL_NAMESPACE="$LAUNCHPAD_LOCAL_SENDER"
export LAUNCHPAD_LOCAL_TREASURY="$LAUNCHPAD_LOCAL_RECIPIENT"
cargo +1.98.1 test --locked -p launchpad-cli --features standalone-test --test local_node configure_namespace_on_standalone_lez -- --ignored --nocapture
```

When done, stop the node with `kill "$node_pid"`, then wait for it to exit. Delete the scratch directory created above. It contains unencrypted test wallet keys.
