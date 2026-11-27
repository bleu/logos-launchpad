use clap::Parser;
use launchpad_client::{PublicWriteWallet, WriteTransport};
use lee::AccountId;
use lee_core::account::Actor;
use serde_json::Value;

use super::{
    id,
    launchpad_transport::{self, FixtureTransport},
};
use crate::{args::Cli, run_command as run_cli};

use ConfigureDeployment::{Clock, HeaderOnly, Missing, MissingSegments};

#[tokio::test(start_paused = true)]
async fn configure_reports_pending_with_the_acknowledged_hash() {
    let hash = common::HashType([21; 32]);
    let mut transport = FixtureTransport::default();
    transport.acknowledge(hash, None);
    let (code, result) = run_configure(transport.clone(), HeaderOnly).await;
    assert_eq!(code, 3, "{result}");
    assert_eq!(result["status"], "pending");
    assert_eq!(result["submission"], "acknowledged");
    assert_eq!(result["transaction_hash"], hex::encode(hash));
    assert_eq!(transport.sent_count(), 1);
}

#[tokio::test]
async fn configure_reports_pending_when_receipt_lookup_fails() {
    let hash = common::HashType([24; 32]);
    let mut transport = FixtureTransport::default();
    transport.acknowledge(hash, None);
    transport.fail_receipt_lookup();
    let (code, result) = run_configure(transport.clone(), HeaderOnly).await;
    assert_eq!(code, 3, "{result}");
    assert_eq!(result["status"], "pending");
    assert_eq!(result["submission"], "acknowledged");
    assert_eq!(result["transaction_hash"], hex::encode(hash));
    assert_eq!(transport.sent_count(), 1);
}

#[tokio::test]
async fn configure_reports_unknown_submission_without_a_hash() {
    let mut transport = FixtureTransport::default();
    transport.lose_send_response();
    let (code, result) = run_configure(transport.clone(), HeaderOnly).await;
    assert_eq!(code, 3, "{result}");
    assert_eq!(result["status"], "pending");
    assert_eq!(result["submission"], "unknown");
    assert!(result["transaction_hash"].is_null());
    assert_eq!(transport.sent_count(), 1);
}

#[tokio::test]
async fn configure_reports_execution_after_a_successful_receipt() {
    use common::block::TransactionReceipt;

    let hash = common::HashType([22; 32]);
    let mut transport = FixtureTransport::default();
    transport.acknowledge(hash, Some(TransactionReceipt::EXEMPT));
    let (code, result) = run_configure(transport, HeaderOnly).await;
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["status"], "executed");
    assert_eq!(result["submission"], "acknowledged");
    assert_eq!(result["transaction_hash"], hex::encode(hash));
}

#[tokio::test]
async fn configure_reports_a_reverted_receipt_as_failure() {
    use common::block::TransactionReceipt;

    let hash = common::HashType([23; 32]);
    let mut transport = FixtureTransport::default();
    transport.acknowledge(
        hash,
        Some(TransactionReceipt {
            success: false,
            ..TransactionReceipt::EXEMPT
        }),
    );
    let (code, result) = run_configure(transport, HeaderOnly).await;
    assert_eq!(code, 1);
    assert_eq!(result["status"], "reverted");
    assert_eq!(result["submission"], "acknowledged");
    assert_eq!(result["transaction_hash"], hex::encode(hash));
}

#[tokio::test]
async fn configure_reports_preparation_failure_without_sending() {
    let transport = FixtureTransport::default();
    let (code, result) = run_configure(transport.clone(), Missing).await;
    assert_eq!(code, 1);
    assert_eq!(result["status"], "preparation_failed");
    assert_eq!(result["submission"], "not_sent");
    assert!(result["transaction_hash"].is_null());
    assert_eq!(result["error"]["category"], "read_program_header");
    assert_eq!(transport.sent_count(), 0);
}

#[tokio::test]
async fn configure_reports_preparation_failure_when_wallet_cannot_load_program() {
    let transport = FixtureTransport::default();
    let (code, result) = run_configure(transport.clone(), MissingSegments).await;
    assert_eq!(transport.sent_count(), 0);
    assert_eq!(code, 1, "{result}");
    assert_eq!(result["status"], "preparation_failed");
    assert_eq!(result["submission"], "not_sent");
    assert!(result["transaction_hash"].is_null());
}

#[tokio::test]
async fn configure_reports_simulation_rejection_without_sending() {
    let transport = FixtureTransport::default();
    let (code, result) = run_configure(transport.clone(), Clock).await;
    assert_eq!(code, 1, "{result}");
    assert_eq!(result["status"], "rejected");
    assert_eq!(result["submission"], "not_sent");
    assert!(result["transaction_hash"].is_null());
    assert_eq!(transport.sent_count(), 0);
}

enum ConfigureDeployment {
    Missing,
    HeaderOnly,
    MissingSegments,
    Clock,
}

async fn run_configure(
    mut transport: FixtureTransport,
    deployment: ConfigureDeployment,
) -> (u8, Value) {
    let program = programs::clock();
    let program_account = id(7);
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("curve.elf");
    std::fs::write(&path, program.elf()).unwrap();
    match deployment {
        Missing => {}
        HeaderOnly | MissingSegments => transport.deploy(program_account, program.id()),
        Clock => {
            use lee_core::program::PROGRAM_LOADER_ACCOUNT_ID;
            use program_loader_core::{LoaderActorState, MAX_SEGMENT_DATA_LEN, ProgramSegment};

            let elf = program.user_elf().unwrap();
            let chunks = elf.chunks(MAX_SEGMENT_DATA_LEN);
            let segment_id = |index: usize| id(77 + u8::try_from(index).unwrap());
            let count = chunks.len();
            for (index, bytes) in chunks.enumerate() {
                transport.save(
                    Actor::new(segment_id(index), PROGRAM_LOADER_ACCOUNT_ID),
                    ProgramSegment {
                        bytecode: bytes.to_vec(),
                        next_segment: (index + 1 < count).then(|| segment_id(index + 1)),
                    }
                    .to_loader_actor_state(),
                );
            }
            // The wallet loads all declared programs before it runs the root actor.
            transport.deploy(curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID, program.id());
            transport.deploy(program_account, program.id());
        }
    }
    let (mut wallet, signer) =
        launchpad_transport::write_wallet(home.path(), transport, WriteTransport::new);
    if matches!(deployment, Clock | MissingSegments) {
        wallet.set_fee_params(wallet::FeeParams::default());
    }
    let cli = Cli::try_parse_from([
        "launchpad",
        "--namespace",
        &signer.to_string(),
        "--json",
        "configure",
        "--admin",
        &signer.to_string(),
        "--treasury",
        &id(4).to_string(),
        "--curve-program-path",
        path.to_str().unwrap(),
        "--curve-program-account",
        &program_account.to_string(),
    ])
    .unwrap();
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let code = run_cli(
        cli,
        || -> eyre::Result<FixtureTransport> { panic!("configure uses the wallet") },
        || Ok(wallet),
        &mut output,
        &mut errors,
    )
    .await
    .unwrap();
    assert!(errors.is_empty());
    (code, serde_json::from_slice(&output).unwrap())
}

async fn run_namespace_write(
    mut transport: FixtureTransport,
    command: &str,
    args: &[&str],
    successor_key: Option<lee::PrivateKey>,
) -> (u8, Value) {
    let program = programs::clock();
    let program_account = id(7);
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("curve.elf");
    std::fs::write(&path, program.elf()).unwrap();
    transport.deploy(program_account, program.id());
    let (mut wallet, admin) =
        launchpad_transport::write_wallet(home.path(), transport, WriteTransport::new);
    if let Some(key) = successor_key {
        wallet
            .storage_mut()
            .key_chain_mut()
            .add_imported_public_account(key);
    }
    let cli = Cli::try_parse_from(
        [
            "launchpad",
            "--namespace",
            &id(8).to_string(),
            "--json",
            command,
            "--admin",
            &admin.to_string(),
            "--curve-program-path",
            path.to_str().unwrap(),
            "--curve-program-account",
            &program_account.to_string(),
        ]
        .into_iter()
        .chain(args.iter().copied()),
    )
    .unwrap();
    let mut output = Vec::new();
    let code = run_cli(
        cli,
        || -> eyre::Result<FixtureTransport> { panic!("namespace writes use the wallet") },
        || Ok(wallet),
        &mut output,
        &mut Vec::new(),
    )
    .await
    .unwrap();
    (code, serde_json::from_slice(&output).unwrap())
}

async fn submitted_namespace_message(
    transport: &FixtureTransport,
    hash: common::HashType,
) -> curve_core::Message {
    use wallet::WalletSequencerClient;

    let (tx, _) = transport.get_transaction(hash).await.unwrap().unwrap();
    let (_, bytes) = tx.public_call().unwrap();
    borsh::from_slice(bytes).unwrap()
}

#[tokio::test]
async fn update_fee_accepts_a_rate_without_a_treasury() {
    use common::block::TransactionReceipt;

    let hash = common::HashType([25; 32]);
    let mut transport = FixtureTransport::default();
    transport.acknowledge(hash, Some(TransactionReceipt::EXEMPT));
    let (code, result) = run_namespace_write(
        transport.clone(),
        "update-fee",
        &["--protocol-fee-bps", "125"],
        None,
    )
    .await;

    assert_eq!(code, 0, "{result}");
    assert_eq!(result["status"], "executed");
    assert_eq!(result["transaction_hash"], hex::encode(hash));
    assert!(matches!(
        submitted_namespace_message(&transport, hash).await,
        curve_core::Message::UpdateFee { namespace, protocol_fee_bps: 125 } if namespace == id(8)
    ));
}

#[tokio::test]
async fn update_treasury_accepts_an_account_without_a_fee_rate() {
    use common::block::TransactionReceipt;

    let hash = common::HashType([26; 32]);
    let mut transport = FixtureTransport::default();
    transport.acknowledge(hash, Some(TransactionReceipt::EXEMPT));
    let (code, result) = run_namespace_write(
        transport.clone(),
        "update-treasury",
        &["--treasury", &id(4).to_string()],
        None,
    )
    .await;

    assert_eq!(code, 0, "{result}");
    assert_eq!(result["status"], "executed");
    assert_eq!(result["transaction_hash"], hex::encode(hash));
    assert!(matches!(
        submitted_namespace_message(&transport, hash).await,
        curve_core::Message::UpdateTreasury { namespace, treasury }
            if namespace == id(8) && treasury == id(4)
    ));
}

#[tokio::test]
async fn fee_and_treasury_updates_report_reverted_receipts_as_failure() {
    use common::{HashType, block::TransactionReceipt};

    let treasury = id(4).to_string();
    let hash = HashType([30; 32]);
    let reverted = TransactionReceipt {
        success: false,
        ..TransactionReceipt::EXEMPT
    };
    for (command, args) in [
        ("update-fee", ["--protocol-fee-bps", "125"]),
        ("update-treasury", ["--treasury", treasury.as_str()]),
    ] {
        let mut transport = FixtureTransport::default();
        transport.acknowledge(hash, Some(reverted));
        let (code, result) = run_namespace_write(transport.clone(), command, &args, None).await;

        assert_eq!(code, 1, "{command}: {result}");
        assert_eq!(result["status"], "reverted");
        assert_eq!(result["submission"], "acknowledged");
        assert_eq!(result["transaction_hash"], hex::encode(hash));
        assert_eq!(transport.sent_count(), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn transfer_admin_names_the_successor_only_after_execution() {
    use common::block::TransactionReceipt;

    let hash = common::HashType([27; 32]);
    let reverted = TransactionReceipt {
        success: false,
        ..TransactionReceipt::EXEMPT
    };
    for (receipt, status, exit) in [
        (None, "pending", 3),
        (Some(reverted), "reverted", 1),
        (Some(TransactionReceipt::EXEMPT), "executed", 0),
    ] {
        let mut transport = FixtureTransport::default();
        transport.acknowledge(hash, receipt);
        let key = lee::PrivateKey::try_new([2; 32]).unwrap();
        let new_admin = AccountId::from(&lee::PublicKey::new_from_private_key(&key)).to_string();
        let (code, result) = run_namespace_write(
            transport.clone(),
            "transfer-admin",
            &["--new-admin", &new_admin],
            Some(key),
        )
        .await;

        assert_eq!(code, exit, "{result}");
        assert_eq!(result["status"], status);
        assert_eq!(result["transaction_hash"], hex::encode(hash));
        if exit == 0 {
            assert_eq!(result["authority_holders"], serde_json::json!([new_admin]));
        } else {
            assert!(result.get("authority_holders").is_none(), "{result}");
        }
        assert_eq!(transport.sent_count(), 1);
    }
}

#[tokio::test]
async fn transfer_admin_without_the_successor_key_sends_nothing() {
    let transport = FixtureTransport::default();
    let (code, result) = run_namespace_write(
        transport.clone(),
        "transfer-admin",
        &["--new-admin", &id(9).to_string()],
        None,
    )
    .await;

    assert_eq!(code, 1, "{result}");
    assert_eq!(result["status"], "preparation_failed");
    assert_eq!(result["submission"], "not_sent");
    assert_eq!(
        result["error"]["category"],
        "public_signing_key_unavailable"
    );
    assert_eq!(transport.sent_count(), 0);
}

#[tokio::test]
async fn renounce_admin_requires_confirmation_before_opening_the_wallet() {
    let home = tempfile::tempdir().unwrap();
    let missing = home.path().join("missing.elf");
    let cli = Cli::try_parse_from([
        "launchpad",
        "--namespace",
        &id(8).to_string(),
        "--json",
        "renounce-admin",
        "--admin",
        &id(1).to_string(),
        "--curve-program-path",
        missing.to_str().unwrap(),
        "--curve-program-account",
        &id(7).to_string(),
    ])
    .unwrap();
    let mut output = Vec::new();
    let code = run_cli(
        cli,
        || -> eyre::Result<FixtureTransport> { panic!("must not connect without confirmation") },
        || -> eyre::Result<PublicWriteWallet<FixtureTransport>> {
            panic!("must not open a wallet without confirmation")
        },
        &mut output,
        &mut Vec::new(),
    )
    .await
    .unwrap();

    let result: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(code, 1, "{result}");
    assert_eq!(result["status"], "preparation_failed");
    assert_eq!(result["submission"], "not_sent");
    let message = result["error"]["message"].as_str().unwrap();
    assert!(message.contains("permanently"), "{message}");
    assert!(message.contains("--confirm-renounce"), "{message}");
}

#[tokio::test]
async fn confirmed_renunciation_reports_the_execution_result_on_each_call() {
    use common::{HashType, block::TransactionReceipt};

    let mut transport = FixtureTransport::default();
    let reverted = TransactionReceipt {
        success: false,
        ..TransactionReceipt::EXEMPT
    };
    for (index, receipt, status, exit) in [
        (28, TransactionReceipt::EXEMPT, "executed", 0),
        (29, reverted, "reverted", 1),
    ] {
        let hash = HashType([index; 32]);
        transport.acknowledge(hash, Some(receipt));
        let (code, result) = run_namespace_write(
            transport.clone(),
            "renounce-admin",
            &["--confirm-renounce"],
            None,
        )
        .await;

        assert_eq!(code, exit, "{result}");
        assert_eq!(result["status"], status);
        assert_eq!(result["transaction_hash"], hex::encode(hash));
    }
    assert_eq!(transport.sent_count(), 2);
}
