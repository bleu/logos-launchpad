use std::io::Write;

use clap::{Args, ValueEnum};
use eyre::Result;
use launchpad_client::{
    CreationStage, FactorySale, LbpCreationStage, LbpSale, LbpSaleInfo, LbpSettlementProgress,
    SaleInfo, SaleSelection, SavedSaleTerms, SettlementStage, load_sale_info,
};
use lee::AccountId;
use serde_json::{Value, json};
use wallet::WalletSequencerClient;

use super::args::{LaunchArgs, TradeModeArg, parse_public_account_id};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Engine {
    BondingCurve,
    FixedPrice,
    Weighted,
}

#[derive(Debug, Args)]
pub(crate) struct SaleInfoArgs {
    #[arg(long, value_enum)]
    engine: Engine,
    #[command(flatten)]
    launch: LaunchArgs,
    #[arg(long, value_parser = parse_public_account_id)]
    collateral_definition: AccountId,
    #[arg(long, value_parser = parse_public_account_id)]
    factory_program_account: AccountId,
    /// Deployed curve or LBP program account, for the selected engine.
    #[arg(long, value_parser = parse_public_account_id)]
    engine_program_account: AccountId,
    /// Deployed namespace program account for LBP sales.
    #[arg(long, value_parser = parse_public_account_id, required_if_eq_any([("engine", "fixed-price"), ("engine", "weighted")]))]
    namespace_program_account: Option<AccountId>,
}

pub(crate) async fn show(
    namespace: AccountId,
    args: SaleInfoArgs,
    transport: &impl WalletSequencerClient,
    json_output: bool,
    output: &mut impl Write,
) -> Result<()> {
    let selection = match args.engine {
        Engine::BondingCurve => SaleSelection::BondingCurve(FactorySale {
            namespace,
            factory_program_id: args.factory_program_account,
            curve_program_id: args.engine_program_account,
            launch_salt: args.launch.launch_salt,
            collateral_definition: args.collateral_definition,
        }),
        Engine::FixedPrice | Engine::Weighted => {
            let sale = LbpSale {
                namespace,
                factory_program_id: args.factory_program_account,
                namespace_program_id: args.namespace_program_account.ok_or_else(|| {
                    eyre::eyre!("--namespace-program-account is required for LBP sales")
                })?,
                lbp_program_id: args.engine_program_account,
                launch_salt: args.launch.launch_salt,
                collateral_definition: args.collateral_definition,
            };
            match args.engine {
                Engine::FixedPrice => SaleSelection::FixedPrice(sale),
                Engine::Weighted => SaleSelection::Weighted(sale),
                Engine::BondingCurve => unreachable!(),
            }
        }
    };
    let view = match load_sale_info(transport, selection).await? {
        SaleInfo::BondingCurve(info) => json!({
            "engine": "bonding-curve",
            "setup_stage": match info.setup_stage {
                CreationStage::Minted => "minted",
                CreationStage::Allocated => "allocated",
                CreationStage::PoolPrepared => "pool_prepared",
                CreationStage::Active => "active",
            },
            "terms": terms(&info.terms),
            "settlement_stage": match info.settlement_stage {
                SettlementStage::Unstarted => "unstarted",
                SettlementStage::Ready => "ready",
                SettlementStage::Withdrawn { .. } => "withdrawn",
                SettlementStage::HookReady { .. } => "hook_ready",
                SettlementStage::Complete => "complete",
                SettlementStage::Graduated => "graduated",
            },
            "creator_allocation_claimed": info.creator_allocation_claimed,
            "initial_virtual_token_reserve": info.initial_virtual_token_reserve,
            "initial_virtual_collateral_reserve": info.initial_virtual_collateral_reserve,
            "pool_observation": info.pool.map(|pool| json!({
                "funded": pool.funded,
                "saved_lifecycle": match pool.state.lifecycle {
                    pool::PoolLifecycle::Open => "open",
                    pool::PoolLifecycle::Closed => "closed",
                    pool::PoolLifecycle::Withdrawn => "withdrawn",
                },
                "tokens_sold": pool.tokens_sold,
                "real_token_reserve": pool.state.real_reserve0,
                "real_collateral_reserve": pool.state.real_reserve1,
                "virtual_token_reserve": pool.state.virtual_reserve0,
                "virtual_collateral_reserve": pool.state.virtual_reserve1,
                "fees_accrued": pool.state.fees_accrued,
                "fees_collected": pool.state.fees_collected,
            })),
        }),
        SaleInfo::FixedPrice(info) => {
            let mut view = lbp_view("fixed-price", &info.sale);
            view["price"] = json!(info.price);
            view
        }
        SaleInfo::Weighted(info) => {
            let mut view = lbp_view("weighted", &info.sale);
            view["collateral_seed"] = json!(info.collateral_seed);
            view["token_weight_start"] = json!(info.token_weight_start);
            view["token_weight_end"] = json!(info.token_weight_end);
            view
        }
    };
    if json_output {
        writeln!(output, "{view}")?;
    } else {
        writeln!(output, "{}", serde_json::to_string_pretty(&view)?)?;
    }
    Ok(())
}

fn terms(terms: &SavedSaleTerms) -> Value {
    json!({
        "namespace": terms.namespace.to_string(),
        "factory": terms.factory.to_string(),
        "launch_salt": hex::encode(terms.launch_salt),
        "pool": terms.pool.to_string(),
        "token_definition": terms.launch_token.to_string(),
        "name": terms.name,
        "uri": terms.uri,
        "collateral_definition": terms.collateral.to_string(),
        "creator_role": terms.creator_role.to_string(),
        "sale_inventory": terms.sale_inventory,
        "graduation_reserve": terms.graduation_reserve,
        "creator_allocation": terms.creator_allocation,
        "total_supply": terms.total_supply,
        "buy_policy": terms.buy_policy,
        "pause_policy": terms.pause_policy,
        "ending_policy": terms.ending_policy,
        "trade_mode": TradeModeArg::from(terms.trade_mode),
        "sell_fee_bps": terms.sell_fee_bps,
        "per_block_token_limit": terms.per_block_token_limit,
        "graduation_hook": terms.graduation_hook.as_ref().map(|hook| json!({
            "program_id": hook.program_id.to_string(),
            "configuration": hex::encode(&hook.configuration),
        })),
        "graduation_launch_bps": terms.graduation_policy.launch_bps,
        "graduation_collateral_bps": terms.graduation_policy.collateral_bps,
        "end_timestamp": terms.end_timestamp,
    })
}

fn lbp_view(engine: &str, sale: &LbpSaleInfo) -> Value {
    json!({
        "engine": engine,
        "setup_stage": match sale.setup_stage {
            LbpCreationStage::Minted => "minted",
            LbpCreationStage::Allocated => "allocated",
            LbpCreationStage::PoolPrepared => "pool_prepared",
            LbpCreationStage::Complete => "complete",
        },
        "terms": terms(&sale.terms),
        "settlement_progress": match sale.settlement_progress {
            LbpSettlementProgress::Unselected => "unselected",
            LbpSettlementProgress::ManualStarted => "manual_started",
            LbpSettlementProgress::ManualComplete => "manual_complete",
            LbpSettlementProgress::Graduated => "graduated",
        },
        "start_timestamp": sale.start_timestamp,
        "allocation_release": sale.allocation_release,
        "creator_allocation_released": sale.creator_allocation_released,
    })
}
