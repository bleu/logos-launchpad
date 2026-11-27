use std::path::Path;

use clap::Parser;
use common::{HashType, block::TransactionReceipt};
use factory_core::{CreationStage, FactoryState, GraduationPolicy, SettlementStage, TradeMode};
use launchpad_client::{
    BuyPolicy, EndingPolicy, FactorySale, PausePolicy, PublicWriteWallet, WriteTransport,
};
use lee::AccountId;
use lee_core::account::Actor;
use serde_json::Value;

use super::{Cli, run_command as run_cli};

async fn run_command(
    cli: Cli,
    read_client: impl FnOnce() -> eyre::Result<FixtureTransport>,
    output: &mut impl std::io::Write,
    errors: &mut impl std::io::Write,
) -> std::io::Result<u8> {
    run_cli(
        cli,
        read_client,
        || -> eyre::Result<PublicWriteWallet<FixtureTransport>> {
            panic!("sale-info must not open a wallet")
        },
        output,
        errors,
    )
    .await
}

#[path = "../../verify/fixtures/launchpad_transport.rs"]
mod launchpad_transport;
use launchpad_transport::FixtureTransport;

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

#[tokio::test]
async fn namespace_info_shows_only_the_selected_namespaces_saved_settings() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("curve.elf");
    std::fs::write(&path, programs::clock().elf()).unwrap();
    let (namespace, curve, treasury, other_namespace) = (id(8), id(7), id(4), id(9));
    let mut transport = FixtureTransport::default();
    let state = borsh::to_vec(&(
        *b"LZNSCONF",
        3_u16,
        namespace,
        175_u16,
        treasury,
        id(6),
        false,
    ))
    .unwrap();
    for account_namespace in [namespace, other_namespace] {
        transport.save(
            Actor::new(
                curve_core::compute_config_pda(account_namespace, curve),
                curve,
            ),
            state.clone(),
        );
    }
    for selected in [namespace, other_namespace] {
        let cli = Cli::try_parse_from([
            "launchpad",
            "--namespace",
            &selected.to_string(),
            "--json",
            "namespace-info",
            "--curve-program-path",
            path.to_str().unwrap(),
            "--curve-program-account",
            &curve.to_string(),
        ])
        .unwrap();
        let wallet_home = tempfile::tempdir().unwrap();
        let (wallet, _) = launchpad_transport::write_wallet(
            wallet_home.path(),
            transport.clone(),
            WriteTransport::new,
        );
        let mut output = Vec::new();
        let code = run_cli(
            cli,
            || -> eyre::Result<FixtureTransport> { panic!("namespace-info uses the wallet") },
            || Ok(wallet),
            &mut output,
            &mut Vec::new(),
        )
        .await
        .unwrap();
        let result: Value = serde_json::from_slice(&output).unwrap();
        if selected == namespace {
            assert_eq!(code, 0, "{result}");
            assert_eq!(
                result,
                serde_json::json!({
                    "namespace": namespace.to_string(),
                    "protocol_fee_bps": 175,
                    "treasury": treasury.to_string(),
                })
            );
        } else {
            assert_eq!(code, 1, "{result}");
            assert_eq!(result["status"], "error");
            assert_eq!(result["error"]["category"], "pool_selection_mismatch");
        }
    }
    assert_eq!(transport.sent_count(), 0);
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

#[tokio::test(start_paused = true)]
async fn collect_fees_stops_on_pending_and_reports_the_executed_step() {
    use common::{HashType, block::TransactionReceipt};

    let (namespace, curve, collateral, treasury) = (id(1), id(3), id(4), id(5));
    let program = programs::clock();
    let mut transport = FixtureTransport::default();
    transport.deploy(curve, program.id());
    transport.save(
        Actor::new(curve_core::compute_config_pda(namespace, curve), curve),
        borsh::to_vec(&(
            *b"LZNSCONF",
            3_u16,
            namespace,
            0_u16,
            treasury,
            id(6),
            false,
        ))
        .unwrap(),
    );
    let pools = [id(11), id(12), id(13)].map(|definition| {
        let pool = curve_core::PoolAccount {
            buy_policy: BuyPolicy::Open,
            trade_control: curve_core::TradeControl::Unrestricted,
            allowed_input_token: None,
            funded: true,
            closure_observed: false,
            namespace,
            token0_definition_id: definition,
            token1_definition_id: collateral,
            owner: id(8),
            owner_program: None,
            pool: pool::Pool::create(pool::PoolCreateParams {
                token0_amount: 800,
                token1_amount: 100,
                virtual_reserve0: 1_000,
                virtual_reserve1: 100,
                ending_policy: EndingPolicy::Manual,
                sell_fee_bps: 0,
                close_timestamp: None,
                close_on_depletion: None,
            })
            .unwrap(),
            block_allocation: Default::default(),
        };
        let address =
            curve_core::compute_pool_pda(namespace, curve, definition, collateral, pool.owner);
        transport.save(Actor::new(address, curve), borsh::to_vec(&pool).unwrap());
        address.to_string()
    });
    let first = HashType([31; 32]);
    let pending = HashType([32; 32]);
    transport.acknowledge(first, Some(TransactionReceipt::EXEMPT));
    transport.acknowledge(pending, None);
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("curve.elf");
    std::fs::write(&path, program.elf()).unwrap();
    let (wallet, signer) =
        launchpad_transport::write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let cli = Cli::try_parse_from([
        "launchpad",
        "--namespace",
        &namespace.to_string(),
        "--json",
        "collect-fees",
        "--fee-payer",
        &signer.to_string(),
        "--collateral-definition",
        &collateral.to_string(),
        "--curve-program-path",
        path.to_str().unwrap(),
        "--curve-program-account",
        &curve.to_string(),
        "--pool",
        &pools[0],
        &pools[1],
        &pools[2],
    ])
    .unwrap();
    let mut output = Vec::new();
    let code = run_cli(
        cli,
        || -> eyre::Result<FixtureTransport> { panic!("collect-fees uses the wallet") },
        || Ok(wallet),
        &mut output,
        &mut Vec::new(),
    )
    .await
    .unwrap();
    let result: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(code, 3, "{result}");
    assert_eq!(result["status"], "pending");
    assert_eq!(result["transaction_hash"], hex::encode(pending));
    assert_eq!(
        result["executed_transaction_hashes"],
        serde_json::json!([hex::encode(first)])
    );
    assert_eq!(transport.sent_count(), 2);
}

#[tokio::test]
async fn completed_withdrawal_reports_no_new_execution() {
    let sale = curve_sale();
    let factory_account =
        factory_core::compute_factory_pda(sale.namespace, sale.factory_program_id, SALT);
    let actor = Actor::new(factory_account, sale.factory_program_id);
    let mut transport = FixtureTransport::default();
    let mut state = curve_state(sale);
    state.creation_stage = CreationStage::Active;
    state.settlement_stage = SettlementStage::Complete;
    transport.save(actor, borsh::to_vec(&state).unwrap());
    let mut pool = funded_curve_pool(&state, actor);
    pool.closure_observed = true;
    pool.pool.lifecycle = pool::PoolLifecycle::Withdrawn;
    transport.save(
        Actor::new(state.pool_id, sale.curve_program_id),
        borsh::to_vec(&pool).unwrap(),
    );
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("guest.elf");
    std::fs::write(&path, programs::clock().elf()).unwrap();
    let (wallet, signer) =
        launchpad_transport::write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let cli = Cli::try_parse_from(curve_write_args("withdraw", &path, signer)).unwrap();
    let mut output = Vec::new();
    let code = run_cli(
        cli,
        || -> eyre::Result<FixtureTransport> { panic!("withdraw uses the wallet") },
        || Ok(wallet),
        &mut output,
        &mut Vec::new(),
    )
    .await
    .unwrap();
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&output));
    let result: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(result["status"], "no_change");
    assert_eq!(result["submission"], "not_sent");
    assert!(result["transaction_hash"].is_null());
    assert!(result.get("setup_stage").is_none());
    assert!(result.get("funded").is_none());
    assert_eq!(transport.sent_count(), 0);
}

const SALT: [u8; 32] = [0x11; 32];

fn id(byte: u8) -> AccountId {
    AccountId::new([byte; 32])
}

fn sale_args(engine: &str) -> Vec<String> {
    [
        "launchpad".into(),
        "--namespace".into(),
        id(1).to_string(),
        "--json".into(),
        "sale-info".into(),
        "--engine".into(),
        engine.into(),
        "--launch-salt".into(),
        hex::encode(SALT),
        "--collateral-definition".into(),
        id(4).to_string(),
        "--factory-program-account".into(),
        id(2).to_string(),
        "--engine-program-account".into(),
        id(3).to_string(),
    ]
    .into()
}

fn curve_write_args(command: &str, path: &Path, creator: AccountId) -> Vec<String> {
    let sale = curve_sale();
    let path = path.to_str().unwrap();
    [
        "launchpad",
        "--json",
        command,
        "--namespace",
        &sale.namespace.to_string(),
        "--launch-salt",
        &hex::encode(sale.launch_salt),
        "--creator",
        &creator.to_string(),
        "--collateral-definition",
        &sale.collateral_definition.to_string(),
        "--factory-program-path",
        path,
        "--factory-program-account",
        &sale.factory_program_id.to_string(),
        "--curve-program-path",
        path,
        "--curve-program-account",
        &sale.curve_program_id.to_string(),
    ]
    .map(str::to_owned)
    .into()
}

fn create_sale_args(path: &Path, creator: AccountId, root: Option<&str>) -> Vec<String> {
    let mut args = curve_write_args("create-sale", path, creator);
    args.extend(
        [
            "--trade-mode",
            "buy-and-sell",
            "--name",
            "Fixture",
            "--uri",
            "ipfs://fixture",
            "--sale-reserve",
            "800",
            "--creator-allocation",
            "50",
            "--virtual-token-reserve",
            "2000",
            "--virtual-collateral-reserve",
            "100",
        ]
        .map(str::to_owned),
    );
    if let Some(root) = root {
        args.extend(["--allowlist-root".into(), root.into()]);
    }
    args
}

#[test]
fn create_sale_requires_exactly_64_hex_characters_for_an_allowlist_root() {
    for root in ["a".repeat(63), "a".repeat(66), "g".repeat(64)] {
        let error = Cli::try_parse_from(create_sale_args(
            Path::new("unused.elf"),
            id(8),
            Some(&root),
        ))
        .unwrap_err();
        assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("--allowlist-root"));
    }
}

fn curve_sale() -> FactorySale {
    FactorySale {
        namespace: id(1),
        factory_program_id: id(2),
        curve_program_id: id(3),
        launch_salt: SALT,
        collateral_definition: id(4),
    }
}

fn curve_state(sale: FactorySale) -> FactoryState {
    let FactorySale {
        namespace,
        factory_program_id: factory_program,
        curve_program_id: curve_program,
        launch_salt,
        collateral_definition: collateral,
    } = sale;
    let factory = factory_core::compute_factory_pda(namespace, factory_program, launch_salt);
    let definition = factory_core::compute_definition_pda(namespace, factory_program, launch_salt);
    let bytes = borsh::to_vec(&(
        (
            BuyPolicy::Open,
            PausePolicy::Disabled,
            EndingPolicy::Depletion,
            TradeMode::BuyAndSell,
            0_u16,
            None::<u128>,
            None::<factory_core::GraduationHook>,
            CreationStage::Minted,
            SettlementStage::Unstarted,
            None::<u64>,
        ),
        (
            namespace,
            launch_salt,
            definition,
            collateral,
            800_u128,
            0_u128,
            GraduationPolicy::default(),
            50_u128,
            850_u128,
            2_000_u128,
            100_u128,
        ),
        (
            curve_program,
            factory_core::compute_curve_creator_role_pda(namespace, factory_program, launch_salt),
            "Fixture",
            "ipfs://fixture",
            factory_core::compute_escrow_pda(namespace, factory_program, launch_salt),
            curve_core::compute_pool_pda(namespace, curve_program, definition, collateral, factory),
            false,
            None::<u8>,
        ),
    ))
    .unwrap();
    FactoryState::decode(&bytes).unwrap()
}

fn curve_transport() -> FixtureTransport {
    let sale = curve_sale();
    let factory = factory_core::compute_factory_pda(sale.namespace, sale.factory_program_id, SALT);
    let mut transport = FixtureTransport::default();
    transport.save(
        Actor::new(factory, sale.factory_program_id),
        borsh::to_vec(&curve_state(sale)).unwrap(),
    );
    transport
}

fn funded_curve_pool(state: &FactoryState, factory: Actor) -> curve_core::PoolAccount {
    curve_core::PoolAccount {
        buy_policy: state.buy_policy.clone(),
        trade_control: curve_core::TradeControl::Unrestricted,
        allowed_input_token: None,
        funded: true,
        closure_observed: false,
        namespace: state.namespace,
        token0_definition_id: state.token_definition_id,
        token1_definition_id: state.collateral_definition_id,
        owner: factory.account_id,
        owner_program: Some((
            factory.program_account_id,
            *factory_core::compute_factory_seed(state.namespace, state.launch_salt).as_bytes(),
        )),
        pool: pool::Pool::create(pool::PoolCreateParams {
            token0_amount: state.sale_reserve,
            token1_amount: 0,
            virtual_reserve0: state.virtual_token_reserve,
            virtual_reserve1: state.virtual_collateral_reserve,
            ending_policy: state.ending_policy,
            sell_fee_bps: state.sell_fee_bps,
            close_timestamp: state.end_timestamp,
            close_on_depletion: Some(pool::TokenSide::Token0),
        })
        .unwrap(),
        block_allocation: Default::default(),
    }
}

async fn run_create_sale(
    mut transport: FixtureTransport,
    root: Option<&str>,
    after_open: impl FnOnce(&mut FixtureTransport, AccountId),
) -> (u8, Value) {
    let program = programs::clock();
    let sale = curve_sale();
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("guest.elf");
    std::fs::write(&path, program.elf()).unwrap();
    transport.deploy(sale.factory_program_id, program.id());
    let (wallet, signer) =
        launchpad_transport::write_wallet(home.path(), transport.clone(), WriteTransport::new);
    after_open(&mut transport, signer);
    let cli = Cli::try_parse_from(create_sale_args(&path, signer, root)).unwrap();
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let code = run_cli(
        cli,
        || -> eyre::Result<FixtureTransport> { panic!("create-sale uses the wallet") },
        || Ok(wallet),
        &mut output,
        &mut errors,
    )
    .await
    .unwrap();
    assert!(errors.is_empty());
    (code, serde_json::from_slice(&output).unwrap())
}

#[tokio::test]
async fn create_sale_exposes_funded_state_and_only_retries_the_same_allowlist() {
    use wallet::WalletSequencerClient;

    let sale = curve_sale();
    let root = [0xab; 32];
    let root_arg = hex::encode_upper(root);
    let hash = HashType([33; 32]);
    let mut transport = FixtureTransport::default();
    transport.acknowledge(hash, Some(TransactionReceipt::EXEMPT));
    let (code, result) =
        run_create_sale(transport.clone(), Some(&root_arg), |transport, signer| {
            let factory = Actor::new(
                factory_core::compute_factory_pda(sale.namespace, sale.factory_program_id, SALT),
                sale.factory_program_id,
            );
            let mut state = curve_state(sale);
            state.buy_policy = launchpad_client::curve_buy_policy(sale, Some(root));
            state.creation_stage = CreationStage::Active;
            let role = Actor::new(
                state.creator_role_id,
                curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
            );
            for (actor, bytes) in [
                (factory, borsh::to_vec(&state).unwrap()),
                (
                    Actor::new(state.pool_id, sale.curve_program_id),
                    borsh::to_vec(&funded_curve_pool(&state, factory)).unwrap(),
                ),
                (
                    role,
                    launchpad_transport::creator_role_state(role, factory, signer),
                ),
            ] {
                transport.save_after_receipt(hash, actor, bytes);
            }
        })
        .await;
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["status"], "executed");
    assert_eq!(result["submission"], "acknowledged");
    assert_eq!(result["transaction_hash"], hex::encode(hash));
    assert_eq!(
        result["executed_transaction_hashes"],
        serde_json::json!([hex::encode(hash)])
    );
    assert_eq!(result["launch_salt"], hex::encode(SALT));
    assert_eq!(result["setup_stage"], "active");
    assert_eq!(result["funded"], true);
    assert_eq!(transport.sent_count(), 1);

    let (tx, _) = transport.get_transaction(hash).await.unwrap().unwrap();
    let (_, message) = tx.public_call().unwrap();
    assert!(matches!(
        borsh::from_slice(message).unwrap(),
        factory_core::Message::CreateFactoryPool { buy_policy: BuyPolicy::Allowlist(policy), .. }
            if policy.root == root
    ));

    let cli = Cli::try_parse_from(sale_args("bonding-curve")).unwrap();
    let mut output = Vec::new();
    let code = run_command(cli, || Ok(transport.clone()), &mut output, &mut Vec::new())
        .await
        .unwrap();
    let result: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["setup_stage"], "active");
    assert_eq!(
        result["terms"]["buy_policy"]["Allowlist"]["root"],
        serde_json::json!(root)
    );
    assert_eq!(result["pool_observation"]["funded"], true);

    let (code, result) = run_create_sale(transport.clone(), Some(&root_arg), |_, _| {}).await;
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["status"], "no_change");
    assert_eq!(result["submission"], "not_sent");
    assert!(result["transaction_hash"].is_null());
    assert_eq!(result["executed_transaction_hashes"], serde_json::json!([]));
    assert_eq!(result["setup_stage"], "active");
    assert_eq!(result["funded"], true);
    assert_eq!(transport.sent_count(), 1);

    let other_root = hex::encode([0xcd; 32]);
    for root in [Some(other_root.as_str()), None] {
        let (code, result) = run_create_sale(transport.clone(), root, |_, _| {}).await;
        assert_eq!(code, 1, "{result}");
        assert_eq!(result["status"], "preparation_failed");
        assert_eq!(result["submission"], "not_sent");
        assert_eq!(result["error"]["category"], "launch_parameters_mismatch");
        assert_eq!(transport.sent_count(), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn create_sale_preserves_the_acknowledged_hash_without_verified_state() {
    let hash = HashType([34; 32]);
    for (receipt, status, exit) in [
        (None, "pending", 3),
        (
            Some(TransactionReceipt {
                success: false,
                ..TransactionReceipt::EXEMPT
            }),
            "reverted",
            1,
        ),
        (Some(TransactionReceipt::EXEMPT), "verification_failed", 1),
    ] {
        let mut transport = FixtureTransport::default();
        transport.acknowledge(hash, receipt);
        let (code, result) = run_create_sale(transport.clone(), None, |_, _| {}).await;
        assert_eq!(code, exit, "{result}");
        assert_eq!(result["status"], status);
        assert_eq!(result["submission"], "acknowledged");
        assert_eq!(result["transaction_hash"], hex::encode(hash));
        assert!(result.get("setup_stage").is_none());
        assert!(result.get("funded").is_none());
        if status == "verification_failed" {
            assert_eq!(result["error"]["category"], "creation_verification_failed");
            assert_eq!(
                result["executed_transaction_hashes"],
                serde_json::json!([hex::encode(hash)])
            );
        } else {
            assert_eq!(result["executed_transaction_hashes"], serde_json::json!([]));
        }
        assert_eq!(transport.sent_count(), 1);
    }
}

#[tokio::test]
async fn sale_info_shows_saved_curve_terms_without_wallet_storage() {
    let cli = Cli::try_parse_from(sale_args("bonding-curve")).unwrap();
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let code = run_command(cli, || Ok(curve_transport()), &mut output, &mut errors)
        .await
        .unwrap();
    assert_eq!(code, 0);
    assert!(errors.is_empty());
    let view: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(view["engine"], "bonding-curve");
    assert_eq!(view["setup_stage"], "minted");
    assert_eq!(view["terms"]["sale_inventory"], 800);
    assert_eq!(view["terms"]["name"], "Fixture");
    assert_eq!(view["terms"]["uri"], "ipfs://fixture");
    assert_eq!(view["initial_virtual_collateral_reserve"], 100);
    assert!(view["pool_observation"].is_null());
    assert!(view.get("status").is_none());
}

fn lbp_args(engine: &str) -> Vec<String> {
    let mut args = sale_args(engine);
    args.extend(["--namespace-program-account".into(), id(5).to_string()]);
    args
}

fn lbp_transport(weighted: bool) -> FixtureTransport {
    use factory_core::{CreatorAllocationRelease, FixedPrice, LbpCreationStage, LbpSetupMode};
    let (namespace, factory_program, lbp_program, collateral) = (id(1), id(2), id(3), id(4));
    let factory = factory_core::compute_factory_pda(namespace, factory_program, SALT);
    let definition = factory_core::compute_definition_pda(namespace, factory_program, SALT);
    let (format, ending, mode, price) = if weighted {
        (
            *b"LZWGLAUN",
            EndingPolicy::Scheduled,
            LbpSetupMode::Weighted {
                collateral_seed: 100,
                token_weight_start: 600_000_000_000_000_000,
                token_weight_end: 400_000_000_000_000_000,
            },
            FixedPrice {
                collateral_units: 0,
                token_units: 0,
            },
        )
    } else {
        (
            *b"LZFXLAUN",
            EndingPolicy::Either,
            LbpSetupMode::FixedPrice,
            FixedPrice {
                collateral_units: 25,
                token_units: 10,
            },
        )
    };
    let bytes = borsh::to_vec(&(
        (
            None::<u128>,
            BuyPolicy::Open,
            PausePolicy::Disabled,
            ending,
            TradeMode::BuyOnly,
            0_u16,
            None::<factory_core::GraduationHook>,
            GraduationPolicy::default(),
            format,
            8_u16,
        ),
        (
            factory_core::compute_creator_role_pda(namespace, factory_program, SALT),
            mode,
            LbpCreationStage::Allocated,
            lbp_core::SettlementProgress::Unselected,
            namespace,
            SALT,
            definition,
            collateral,
            factory_core::compute_escrow_pda(namespace, factory_program, SALT),
            lbp_core::compute_pool_pda(namespace, lbp_program, definition, collateral, factory),
        ),
        (
            id(5),
            lbp_program,
            800_u128,
            0_u128,
            200_u128,
            1_000_u128,
            price,
            9_u64,
            Some(100_u64),
            CreatorAllocationRelease::AfterEnd,
            false,
            "Fixture",
            "ipfs://fixture",
            None::<u8>,
            None::<u8>,
        ),
    ))
    .unwrap();
    let mut transport = FixtureTransport::default();
    transport.save(Actor::new(factory, factory_program), bytes);
    transport
}

#[tokio::test]
async fn sale_info_shows_the_exact_saved_fixed_price() {
    let cli = Cli::try_parse_from(lbp_args("fixed-price")).unwrap();
    let mut output = Vec::new();
    let code = run_command(
        cli,
        || Ok(lbp_transport(false)),
        &mut output,
        &mut Vec::new(),
    )
    .await
    .unwrap();
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&output));
    let view: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(view["engine"], "fixed-price");
    assert_eq!(view["setup_stage"], "allocated");
    assert_eq!(view["terms"]["creator_allocation"], 200);
    assert_eq!(
        view["price"],
        serde_json::json!({"collateral_units": 25, "token_units": 10})
    );
}

#[tokio::test]
async fn sale_info_shows_saved_weighted_schedule() {
    let cli = Cli::try_parse_from(lbp_args("weighted")).unwrap();
    let mut output = Vec::new();
    let code = run_command(
        cli,
        || Ok(lbp_transport(true)),
        &mut output,
        &mut Vec::new(),
    )
    .await
    .unwrap();
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&output));
    let view: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(view["engine"], "weighted");
    assert_eq!(view["collateral_seed"], 100);
    assert_eq!(view["start_timestamp"], 9);
    assert_eq!(view["terms"]["end_timestamp"], 100);
    assert_eq!(view["token_weight_start"], 600_000_000_000_000_000_u64);
    assert_eq!(view["token_weight_end"], 400_000_000_000_000_000_u64);
}

#[tokio::test]
async fn sale_info_reports_the_wrong_saved_engine() {
    let cli = Cli::try_parse_from(lbp_args("weighted")).unwrap();
    let mut output = Vec::new();
    let code = run_command(
        cli,
        || Ok(lbp_transport(false)),
        &mut output,
        &mut Vec::new(),
    )
    .await
    .unwrap();
    assert_eq!(code, 1);
    let result: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(result["status"], "error");
    assert_eq!(result["error"]["category"], "sale_engine_mismatch");
    assert_eq!(
        result["error"]["message"],
        "saved sale uses a different pricing engine"
    );
}

#[test]
fn sale_info_rejects_invalid_arguments_before_reading() {
    use clap::error::ErrorKind;
    let mut no_engine = sale_args("bonding-curve");
    no_engine.drain(5..7);
    let mut bad_account = sale_args("bonding-curve");
    *bad_account.last_mut().unwrap() = "not-an-account".into();
    for (args, kind) in [
        (no_engine, ErrorKind::MissingRequiredArgument),
        (bad_account, ErrorKind::ValueValidation),
        (sale_args("weighted"), ErrorKind::MissingRequiredArgument),
    ] {
        let error = Cli::try_parse_from(args).unwrap_err();
        assert_eq!(error.kind(), kind);
        assert_eq!(error.exit_code(), 2);
    }
}

#[tokio::test]
async fn sale_info_rejects_malformed_and_mismatched_saved_accounts() {
    use wallet::WalletSequencerClient;

    let factory = factory_core::compute_factory_pda(id(1), id(2), SALT);
    let actor = Actor::new(factory, id(2));
    let mut transport = curve_transport();
    let account = transport.get_account_view(actor).await.unwrap().unwrap();
    let mut wrong_namespace =
        factory_core::FactoryState::decode(account.data.actor_state(id(2)).as_ref()).unwrap();
    wrong_namespace.namespace = id(6);
    for (bytes, category) in [
        (
            borsh::to_vec(&wrong_namespace).unwrap(),
            "factory_sale_mismatch",
        ),
        (vec![0xff], "decode_factory_state"),
    ] {
        transport.save(actor, bytes);
        let cli = Cli::try_parse_from(sale_args("bonding-curve")).unwrap();
        let mut output = Vec::new();
        let code = run_command(cli, || Ok(transport.clone()), &mut output, &mut Vec::new())
            .await
            .unwrap();
        assert_eq!(code, 1);
        let result: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(result["error"]["category"], category);
    }
}
