use std::path::Path;

use clap::Parser;
use common::{HashType, block::TransactionReceipt};
use factory_core::CreationStage;
use launchpad_client::{BuyPolicy, WriteTransport};
use lee::AccountId;
use lee_core::account::Actor;
use serde_json::Value;

use super::{
    SALT,
    curve_fixture::{curve_sale, curve_state, curve_write_args, funded_curve_pool},
    id,
    launchpad_transport::{self, FixtureTransport},
    run_sale_info, sale_args,
};
use crate::{args::Cli, run_command as run_cli};

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
    let code = run_sale_info(cli, || Ok(transport.clone()), &mut output, &mut Vec::new())
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
