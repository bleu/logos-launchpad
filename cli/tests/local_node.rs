use std::{path::PathBuf, process::Command, sync::atomic::Ordering, time::Duration};

use common::{HashType, transaction::LeeTransaction};
use eyre::{Result, WrapErr, ensure, eyre};
use lee::{AccountId, native_token::Message, program::Program};
use serde_json::Value;
use wallet::{
    AccountIdentity, ExecutionFailureKind, WalletCore, WalletSequencerClient,
    program_facades::{CreditDelivery, native_token_transfer::NativeTokenTransfer},
};

#[path = "local_node/recording_client.rs"]
mod recording_client;
#[path = "local_node/wallet_config.rs"]
mod wallet_config;

use recording_client::{RecordingClient, SavedTransaction};
use wallet_config::{local_wallet_config, required};

fn launchpad(args: &[&str]) -> Result<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_launchpad"))
        .arg("--json")
        .args(args)
        .env(wallet::SUPPRESS_VERBOSE_PRINTS, "1")
        .output()?;
    let json: serde_json::Result<Value> = serde_json::from_slice(&output.stdout);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = json
            .as_ref()
            .ok()
            .and_then(|value| value["error"]["message"].as_str())
            .unwrap_or(&stderr);
        return Err(eyre!("launchpad {} failed: {message}", args[2]));
    }
    json.wrap_err("read Launchpad CLI JSON")
}

async fn check_receipt<C: WalletSequencerClient>(
    wallet: &WalletCore<C>,
    hash: HashType,
    success: bool,
) -> Result<LeeTransaction> {
    let (transaction, block_id) =
        tokio::time::timeout(Duration::from_secs(60), wallet.poller().poll_tx(hash))
            .await
            .wrap_err_with(|| format!("inclusion of {hash} is unknown: timed out"))?
            .map_err(|err| eyre!(err.into_boxed_dyn_error()))?;
    ensure!(transaction.hash() == hash, "transaction hash mismatch");
    let client = wallet.client();
    let receipt = client
        .get_transaction_receipt(hash)
        .await?
        .ok_or_else(|| eyre!("no execution receipt for {hash}"))?;
    ensure!(
        receipt.success == success,
        "unexpected execution result for {hash}"
    );
    let block = client
        .get_block(block_id)
        .await?
        .ok_or_else(|| eyre!("missing inclusion block {block_id}"))?;
    ensure!(
        block.body.transactions.iter().any(|tx| tx.hash() == hash),
        "inclusion block {block_id} does not contain {hash}"
    );
    println!(
        "hash={hash} included_in={block_id} execution_success={} standalone_status={:?}",
        receipt.success, block.bedrock_status
    );
    Ok(transaction)
}

#[tokio::test]
#[ignore = "requires disposable standalone LEZ and deployed Launchpad guests"]
async fn configure_namespace_on_standalone_lez() -> Result<()> {
    local_wallet_config()?;
    let namespace = required("LAUNCHPAD_LOCAL_NAMESPACE")?;
    let treasury = required("LAUNCHPAD_LOCAL_TREASURY")?;
    let curve_elf = required("LAUNCHPAD_LOCAL_CURVE_ELF")?;
    let curve_account = required("LAUNCHPAD_LOCAL_CURVE_ACCOUNT")?;

    let result = launchpad(&[
        "--namespace",
        &namespace,
        "configure",
        "--admin",
        &namespace,
        "--treasury",
        &treasury,
        "--protocol-fee-bps",
        "75",
        "--curve-program-path",
        &curve_elf,
        "--curve-program-account",
        &curve_account,
    ])?;
    assert_eq!(result["status"], "submitted");
    let raw_hash = result["transaction_hash"]
        .as_str()
        .ok_or_else(|| eyre!("CLI did not return a transaction hash"))?;
    let mut bytes = [0; 32];
    hex::decode_to_slice(raw_hash, &mut bytes)?;
    let hash = HashType(bytes);
    println!("admitted_hash={hash}");
    let wallet = WalletCore::from_env().map_err(|err| eyre!(err.into_boxed_dyn_error()))?;
    check_receipt(&wallet, hash, true).await?;

    let config = launchpad(&[
        "--namespace",
        &namespace,
        "namespace-info",
        "--curve-program-path",
        &curve_elf,
        "--curve-program-account",
        &curve_account,
    ])?;
    assert_eq!(config["namespace"], namespace);
    assert_eq!(config["treasury"], treasury);
    assert_eq!(config["protocol_fee_bps"], 75);
    Ok(())
}

async fn transfer<C: WalletSequencerClient>(wallet: &WalletCore<C>) -> Result<HashType> {
    let sender: AccountId = required("LAUNCHPAD_LOCAL_SENDER")?.parse()?;
    let recipient: AccountId = required("LAUNCHPAD_LOCAL_RECIPIENT")?.parse()?;
    let (hash, _) = NativeTokenTransfer(wallet)
        .transfer(
            AccountIdentity::Public(sender),
            AccountIdentity::Public(recipient),
            1,
            CreditDelivery::Automatic,
        )
        .await?;
    Ok(hash)
}

fn recording_wallet(
    record_path: PathBuf,
) -> Result<WalletCore<RecordingClient<impl WalletSequencerClient>>> {
    let config = local_wallet_config()?;
    let client =
        WalletCore::client_from_config(&config).map_err(|err| eyre!(err.into_boxed_dyn_error()))?;
    WalletCore::from_env_with_client(RecordingClient::new(client, record_path))
        .map_err(|err| eyre!(err.into_boxed_dyn_error()))
}

#[tokio::test]
#[ignore = "requires disposable standalone LEZ and a funded test wallet"]
async fn wallet_does_not_broadcast_when_save_fails() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let wallet = recording_wallet(scratch.path().join("missing/transaction.json"))?;
    let result = transfer(&wallet).await;
    assert_eq!(wallet.client().forwarded.load(Ordering::Relaxed), 0);
    assert!(matches!(
        result.as_ref().err().and_then(|err| err.downcast_ref()),
        Some(ExecutionFailureKind::SequencerClientError(message))
            if message.starts_with("save signed transaction:")
    ));
    Ok(())
}

#[tokio::test]
#[ignore = "requires disposable standalone LEZ and a funded test wallet"]
async fn wallet_recovers_a_saved_public_transfer_by_hash() -> Result<()> {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("transaction.json");
    let wallet = recording_wallet(path.clone())?;
    let admitted_hash = transfer(&wallet).await?;
    println!("admitted_hash={admitted_hash}");
    assert_eq!(wallet.client().forwarded.load(Ordering::Relaxed), 1);

    let saved = SavedTransaction::read(&path)?;
    assert_eq!(saved.hash, admitted_hash);
    let included = check_receipt(&wallet, saved.hash, true).await?;
    ensure!(
        included.public_call().is_some(),
        "expected a public transfer"
    );
    ensure!(
        included == saved.transaction,
        "included transaction differs from saved payload"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires disposable standalone LEZ and a funded test wallet"]
async fn wallet_finds_a_failed_receipt_for_an_included_revert() -> Result<()> {
    local_wallet_config()?;
    let mut wallet = WalletCore::from_env().map_err(|err| eyre!(err.into_boxed_dyn_error()))?;
    let sender: AccountId = required("LAUNCHPAD_LOCAL_SENDER")?.parse()?;
    let accounts = vec![AccountIdentity::Public(sender).balance()];
    // A root balance read has no actor to receive its reply.
    let message = Program::serialize_message(Message::ReadState)?;

    let result = wallet
        .send_pub_tx_paid_by(accounts.clone(), 0, message.clone(), Some(sender))
        .await;
    ensure!(
        matches!(result, Err(ExecutionFailureKind::SimulatedRevert(_))),
        "expected the wallet to refuse a simulated revert"
    );

    wallet.set_allow_revert(true);
    let hash = wallet
        .send_pub_tx_paid_by(accounts, 0, message, Some(sender))
        .await?;
    println!("admitted_hash={hash}");
    check_receipt(&wallet, hash, false).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires disposable standalone LEZ"]
async fn wallet_finds_no_transaction_or_receipt_for_an_unknown_hash() -> Result<()> {
    let config = local_wallet_config()?;
    let client =
        WalletCore::client_from_config(&config).map_err(|err| eyre!(err.into_boxed_dyn_error()))?;
    let hash = HashType([0; 32]);
    ensure!(
        client.get_transaction(hash).await?.is_none(),
        "unexpected transaction for {hash}"
    );
    ensure!(
        client.get_transaction_receipt(hash).await?.is_none(),
        "unexpected execution receipt for {hash}"
    );
    Ok(())
}
