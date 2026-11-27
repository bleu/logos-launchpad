use clap::Parser;
use factory_core::{CreationStage, SettlementStage};
use launchpad_client::{BuyPolicy, EndingPolicy, WriteTransport};
use lee_core::account::Actor;
use serde_json::Value;

use super::{
    SALT,
    curve_fixture::{curve_sale, curve_state, curve_write_args, funded_curve_pool},
    id,
    launchpad_transport::{self, FixtureTransport},
};
use crate::{args::Cli, run_command as run_cli};

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
