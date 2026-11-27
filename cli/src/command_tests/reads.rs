use clap::Parser;
use factory_core::{GraduationPolicy, TradeMode};
use launchpad_client::{BuyPolicy, EndingPolicy, PausePolicy, WriteTransport};
use lee_core::account::Actor;
use serde_json::Value;

use super::{
    SALT,
    curve_fixture::curve_transport,
    id,
    launchpad_transport::{self, FixtureTransport},
    lbp_sale, run_sale_info, sale_args,
};
use crate::{args::Cli, run_command as run_cli};

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

#[tokio::test]
async fn sale_info_shows_saved_curve_terms_without_wallet_storage() {
    let cli = Cli::try_parse_from(sale_args("bonding-curve")).unwrap();
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let code = run_sale_info(cli, || Ok(curve_transport()), &mut output, &mut errors)
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

pub(super) fn lbp_state(weighted: bool) -> (Actor, factory_core::LbpLaunchState) {
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
    (
        Actor::new(factory, factory_program),
        factory_core::LbpLaunchState::decode(&bytes).unwrap(),
    )
}

fn lbp_transport(weighted: bool) -> FixtureTransport {
    let (actor, state) = lbp_state(weighted);
    let mut transport = FixtureTransport::default();
    transport.save(actor, borsh::to_vec(&state).unwrap());
    transport
}

#[tokio::test]
async fn sale_info_shows_the_exact_saved_fixed_price() {
    let cli = Cli::try_parse_from(lbp_args("fixed-price")).unwrap();
    let mut output = Vec::new();
    let code = run_sale_info(
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
async fn sale_info_shows_funded_pool_observations_without_a_trading_estimate() {
    let (actor, mut state) = lbp_state(false);
    state.creation_stage = factory_core::LbpCreationStage::Complete;
    state.pause_policy = PausePolicy::AuthorityControlled;
    let mut pool = lbp_sale::funded_pool(actor, &state);
    pool.trade_control.set_paused(true).unwrap();
    pool.closure_observed = true;
    pool.settlement_progress = lbp_core::SettlementProgress::ManualComplete;
    let mut transport = FixtureTransport::default();
    transport.save(actor, borsh::to_vec(&state).unwrap());
    transport.save(
        Actor::new(state.pool_id, state.lbp_program_id),
        borsh::to_vec(&pool).unwrap(),
    );
    for json in [false, true] {
        let mut args = lbp_args("fixed-price");
        if !json {
            args.retain(|arg| arg != "--json");
        }
        let mut output = Vec::new();
        let code = run_sale_info(
            Cli::try_parse_from(args).unwrap(),
            || Ok(transport.clone()),
            &mut output,
            &mut Vec::new(),
        )
        .await
        .unwrap();
        assert_eq!(code, 0, "{}", String::from_utf8_lossy(&output));
        let view: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(view["setup_stage"], "complete");
        assert_eq!(view["settlement_progress"], "unselected");
        assert_eq!(
            view["pool_observation"],
            serde_json::json!({
                "funded": true,
                "paused": true,
                "closure_observed": true,
                "settlement_progress": "manual_complete",
            })
        );
        assert!(view.get("trading_open").is_none());
    }
}

#[tokio::test]
async fn sale_info_shows_saved_weighted_schedule() {
    let cli = Cli::try_parse_from(lbp_args("weighted")).unwrap();
    let mut output = Vec::new();
    let code = run_sale_info(
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
    let code = run_sale_info(
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
        let code = run_sale_info(cli, || Ok(transport.clone()), &mut output, &mut Vec::new())
            .await
            .unwrap();
        assert_eq!(code, 1);
        let result: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(result["error"]["category"], category);
    }
}
