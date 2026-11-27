//! The `launchpad` binary. Argument parsing belongs here; wallet handling,
//! account derivation, quoting, and transaction construction belong in
//! `launchpad-client`.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use clap::{Args, Parser, Subcommand, builder::Styles};
use eyre::{Context, Result};
use launchpad_client::{
    AccountLocation, CreateSaleRequest, ExactInputTradeTerms, ExactOutputTradeTerms, FactorySale,
    NamespaceAction, PoolFeeTarget, PrivateAccountId, PrivateBuyRequest, ProgramHandle,
    PublicWriteWallet, WriteTransport, build_claim_creator_allocation_invocation,
    load_clock_block_id, load_curve_config, load_factory_pool, load_factory_state,
    prepare_buy_invocation, prepare_buy_with_collateral_invocation, prepare_sell_invocation,
    quote_buy, quote_buy_with_collateral, quote_sell, submit_namespace_action,
    submit_public_invocation,
};
use serde::Serialize;
use wallet::{WalletCore, WalletSequencerClient};

mod sale_info;
mod write_result;

use write_result::WriteSuccess;

#[cfg(test)]
mod command_tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
enum ErrorCategory {
    #[error("slippage_floor")]
    SlippageFloor,
    #[error("sale_reserve_overshoot")]
    SaleReserveOvershoot,
    #[error("collateral_reserve_overshoot")]
    CollateralReserveOvershoot,
    #[error("general")]
    General,
}

#[derive(Debug, Parser)]
#[command(name = "launchpad", version, about = "Launchpad on the Logos Execution Zone", styles = Styles::styled())]
struct Cli {
    /// Emit stable machine-readable output.
    #[arg(long, global = true)]
    json: bool,
    /// Stable namespace AccountId used at initialization.
    #[arg(long, global = true)]
    #[arg(value_parser = parse_account_location)]
    namespace: Option<AccountLocation>,
    /// Legacy private authority router path; unsupported on the actor development base.
    #[arg(long, global = true)]
    authority_router_path: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

impl Cli {
    fn validate_route(&self) -> Result<()> {
        if let Command::CreateSale(args) = &self.command {
            let request = args.sale_request();
            if args.graduation_hook_program_id.is_none()
                && (request.graduation_policy.launch_bps != 0
                    || request.graduation_policy.collateral_bps != 0)
            {
                eyre::bail!("nonzero graduation percentages require --graduation-hook-program-id");
            }
            request
                .ending_policy
                .validate(None, request.end_timestamp)?;
        }
        if let Command::TransferAuthority(args) = &self.command
            && (args.from.is_private() || args.to.is_private())
        {
            return Err(launchpad_client::ClientError::UnsupportedPrivateAuthority.into());
        }
        if matches!(
            self.command,
            Command::CreateAuthority(_) | Command::TransferAuthority(_)
        ) {
            eyre::bail!(
                "legacy NFT authority commands are unsupported; launch roles use role authority"
            );
        }
        let creator = match &self.command {
            Command::CreateSale(args) => Some(args.creator),
            Command::Withdraw(args) | Command::Claim(args) => Some(args.creator),
            _ => None,
        };
        if creator.is_some_and(AccountLocation::is_private)
            || (creator.is_some() && self.authority_router_path.is_some())
        {
            return Err(launchpad_client::ClientError::UnsupportedPrivateAuthority.into());
        }
        let collateral = match &self.command {
            Command::CreateSale(args) => Some(args.collateral_definition),
            Command::Withdraw(args) | Command::Claim(args) => Some(args.collateral_definition),
            Command::Buy(args) => Some(args.trade.collateral_definition),
            Command::BuyWithCollateral(args) => Some(args.trade.collateral_definition),
            Command::Sell(args) => Some(args.trade.collateral_definition),
            Command::Price(args) => Some(args.launch.collateral_definition),
            Command::CollectFees(args) => Some(args.collateral_definition),
            _ => None,
        };
        if self.namespace.is_some_and(AccountLocation::is_private)
            || collateral.is_some_and(AccountLocation::is_private)
            || matches!(&self.command, Command::CollectFees(args) if args.pool.iter().any(|pool| pool.is_private()))
        {
            return Err(launchpad_client::ClientError::UnsupportedPrivateRoute.into());
        }
        let participant = match &self.command {
            Command::Buy(args) => Some(args.trade.participant),
            Command::BuyWithCollateral(args) => Some(args.trade.participant),
            Command::Sell(args) => Some(args.trade.participant),
            _ => None,
        };
        if participant.is_some_and(AccountLocation::is_private) {
            return Err(launchpad_client::ClientError::UnsupportedPrivateRoute.into());
        }
        if matches!(
            self.command,
            Command::Configure(_)
                | Command::RenounceAdmin(_)
                | Command::NamespaceInfo(_)
                | Command::UpdateFee(_)
                | Command::UpdateTreasury(_)
                | Command::TransferAdmin(_)
        ) && (self.authority_router_path.is_some()
            || self.namespace.is_some_and(AccountLocation::is_private))
        {
            eyre::bail!(
                "namespace administration supports public AccountId signatures only; no private namespace or authority router"
            );
        }
        if let Command::Configure(args) = &self.command
            && self
                .namespace
                .is_some_and(|namespace| namespace.account_id() != args.admin)
        {
            eyre::bail!("--admin must equal --namespace for namespace initialization");
        }
        if matches!(self.command, Command::Graduate(_))
            && (self.authority_router_path.is_some()
                || self.namespace.is_some_and(AccountLocation::is_private))
        {
            eyre::bail!(
                "graduation supports public execution only; no private namespace or authority router"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create and fund a factory bonding-curve sale.
    #[command(alias = "launch")]
    CreateSale(CreateSaleArgs),
    Withdraw(FactoryLifecycleArgs),
    /// Execute or retry one public post-close graduation stage.
    Graduate(GraduationArgs),
    /// Encode, preview, initialize or run a distribution adapter.
    Distribution {
        #[command(subcommand)]
        command: DistributionCommand,
    },
    Claim(FactoryLifecycleArgs),
    Buy(BuyArgs),
    /// Spend an exact collateral amount, with a minimum launch-token output.
    BuyWithCollateral(BuyWithCollateralArgs),
    /// Private purchase request; unsupported on the actor development base.
    PrivateBuy(PrivateBuyArgs),
    Sell(SellArgs),
    Price(PriceArgs),
    /// Read saved sale terms and setup progress.
    SaleInfo(sale_info::SaleInfoArgs),
    /// Collect one or more pools in this namespace (sequential, resumable).
    CollectFees(CollectFeesArgs),
    /// Initialize a namespace once, using its signing AccountId as administrator.
    Configure(ConfigArgs),
    /// Unsupported legacy NFT command. Launch roles use role authority.
    CreateAuthority(CreateAuthorityArgs),
    /// Unsupported legacy NFT command. Launch roles use role authority.
    TransferAuthority(TransferAuthorityArgs),
    NamespaceInfo(NamespaceReadArgs),
    /// Change the namespace protocol fee rate.
    UpdateFee(UpdateFeeArgs),
    /// Change the account that receives namespace protocol fees.
    UpdateTreasury(UpdateTreasuryArgs),
    /// Transfer namespace administration with signatures from both accounts.
    TransferAdmin(TransferAdminArgs),
    /// Permanently remove the namespace administrator. The emergency role stays in place.
    RenounceAdmin(RenounceAdminArgs),
}

#[derive(Debug, Subcommand)]
enum DistributionCommand {
    Encode {
        #[arg(long)]
        file: PathBuf,
    },
    Preview {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        launch_balance: u128,
        #[arg(long)]
        collateral_balance: u128,
    },
    Initialize {
        #[command(flatten)]
        adapter: DistributionIdentity,
        #[arg(long, value_enum)]
        kind: DistributionKind,
    },
    Remaining {
        #[command(flatten)]
        adapter: DistributionIdentity,
    },
}
#[derive(Debug, Args)]
struct DistributionIdentity {
    #[arg(long)]
    adapter_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    adapter_program_account: lee::AccountId,
    #[arg(long, value_parser = parse_public_account_id)]
    factory_program_account: lee::AccountId,
    #[arg(long, value_parser = parse_public_account_id)]
    fee_payer: lee::AccountId,
    #[command(flatten)]
    launch: LaunchArgs,
}
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum DistributionKind {
    Curve,
    FixedPrice,
    Weighted,
}

#[derive(Debug, Args)]
struct CreateAuthorityArgs {
    #[arg(long)]
    token_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    fee_payer: lee::AccountId,
    #[arg(long)]
    name: String,
    #[arg(long)]
    uri: String,
}
#[derive(Debug, Args)]
struct TransferAuthorityArgs {
    #[arg(long)]
    token_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    definition: lee::AccountId,
    #[arg(long, value_parser = parse_account_location)]
    from: AccountLocation,
    #[arg(long, value_parser = parse_account_location)]
    to: AccountLocation,
}

#[derive(Debug, Args)]
struct NamespaceReadArgs {
    #[arg(long)]
    curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    curve_program_account: lee::AccountId,
}
#[derive(Debug, Args)]
struct NamespaceAdminArgs {
    #[arg(long)]
    curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    curve_program_account: lee::AccountId,
    #[arg(long, value_parser = parse_public_account_id)]
    admin: lee::AccountId,
}

#[derive(Debug, Args)]
struct UpdateFeeArgs {
    #[command(flatten)]
    authority: NamespaceAdminArgs,
    #[arg(long)]
    protocol_fee_bps: u16,
}

#[derive(Debug, Args)]
struct UpdateTreasuryArgs {
    #[command(flatten)]
    authority: NamespaceAdminArgs,
    #[arg(long, value_parser = parse_public_account_id)]
    treasury: lee::AccountId,
}

#[derive(Debug, Args)]
struct TransferAdminArgs {
    #[command(flatten)]
    authority: NamespaceAdminArgs,
    #[arg(long, value_parser = parse_public_account_id)]
    new_admin: lee::AccountId,
}

#[derive(Debug, Args)]
struct RenounceAdminArgs {
    #[command(flatten)]
    authority: NamespaceAdminArgs,
    /// Confirm permanent removal of the namespace administrator.
    #[arg(long)]
    confirm_renounce: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
enum TradeModeArg {
    BuyOnly,
    BuyAndSell,
}

impl From<TradeModeArg> for launchpad_client::TradeMode {
    fn from(mode: TradeModeArg) -> Self {
        match mode {
            TradeModeArg::BuyOnly => Self::BuyOnly,
            TradeModeArg::BuyAndSell => Self::BuyAndSell,
        }
    }
}

impl From<launchpad_client::TradeMode> for TradeModeArg {
    fn from(mode: launchpad_client::TradeMode) -> Self {
        match mode {
            launchpad_client::TradeMode::BuyOnly => Self::BuyOnly,
            launchpad_client::TradeMode::BuyAndSell => Self::BuyAndSell,
        }
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum EndingPolicyArg {
    Depletion,
    Scheduled,
    Either,
}

impl From<EndingPolicyArg> for launchpad_client::EndingPolicy {
    fn from(ending: EndingPolicyArg) -> Self {
        match ending {
            EndingPolicyArg::Depletion => Self::Depletion,
            EndingPolicyArg::Scheduled => Self::Scheduled,
            EndingPolicyArg::Either => Self::Either,
        }
    }
}

#[derive(Debug, Args)]
struct CreateSaleArgs {
    #[arg(long, value_enum)]
    trade_mode: TradeModeArg,
    /// Optional sale allowlist root (64 hexadecimal characters).
    #[arg(long, value_parser = parse_allowlist_root)]
    allowlist_root: Option<[u8; 32]>,
    /// Collateral retained by the sale on sells, in basis points.
    #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u16).range(0..10_000))]
    sell_fee_bps: u16,
    /// Maximum launch tokens all buys can receive per block, in smallest units.
    #[arg(long)]
    per_block_token_limit: Option<u128>,
    /// Let the current creator pause and resume swaps.
    #[arg(long)]
    pausable: bool,
    /// Saved adapter deployment address.
    #[arg(long, value_parser = parse_public_account_id, requires_all = ["graduation_launch_bps", "graduation_collateral_bps"])]
    graduation_hook_program_id: Option<lee::AccountId>,
    /// Saved opaque adapter configuration as hexadecimal bytes.
    #[arg(long, value_parser = parse_hook_configuration, requires = "graduation_hook_program_id")]
    graduation_hook_configuration: Option<HookConfiguration>,
    /// Distribution rules as JSON. Check recipients for this sale before creation.
    #[arg(
        long,
        requires = "graduation_hook_program_id",
        conflicts_with = "graduation_hook_configuration"
    )]
    distribution_configuration: Option<PathBuf>,
    #[command(flatten)]
    launch: LaunchArgs,
    #[arg(long)]
    name: String,
    #[arg(long)]
    uri: String,
    #[arg(long = "sale-reserve", alias = "d")]
    sale_reserve: u128,
    /// Launch tokens kept outside trading for graduation or manual settlement.
    #[arg(long, default_value_t = 0)]
    graduation_reserve: u128,
    /// Adapter share of unsold launch tokens, in basis points.
    #[arg(long, value_parser = clap::value_parser!(u16).range(0..=10_000))]
    graduation_launch_bps: Option<u16>,
    /// Adapter share of recorded collateral, in basis points.
    #[arg(long, value_parser = clap::value_parser!(u16).range(0..=10_000))]
    graduation_collateral_bps: Option<u16>,
    #[arg(long = "creator-allocation")]
    creator_allocation: u128,
    #[arg(long = "virtual-token-reserve", alias = "vt")]
    virtual_token_reserve: u128,
    #[arg(long = "virtual-collateral-reserve", alias = "vc")]
    virtual_collateral_reserve: u128,
    /// Ending rule; defaults to depletion without an end, or either with an end.
    #[arg(long, value_enum)]
    ending_policy: Option<EndingPolicyArg>,
    /// Sale end time in Unix seconds; required for scheduled or either ending.
    #[arg(long = "end-timestamp", required_if_eq_any = [("ending_policy", "scheduled"), ("ending_policy", "either")])]
    end_timestamp: Option<u64>,
    #[arg(long = "collateral-definition", value_parser = parse_account_location)]
    collateral_definition: AccountLocation,
    /// Account that authorizes the factory launch.
    #[arg(long, value_parser = parse_account_location)]
    creator: AccountLocation,
    /// Clock lower bound in Unix seconds, preserved as supplied, including zero.
    #[arg(long, default_value_t = 0)]
    now: u64,
    /// Compiled factory guest binary deployed for this walkthrough.
    #[arg(long = "factory-program-path")]
    factory_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    factory_program_account: lee::AccountId,
    /// Compiled curve guest binary paired with the factory launch.
    #[arg(long = "curve-program-path")]
    curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    curve_program_account: lee::AccountId,
}

impl CreateSaleArgs {
    fn sale(&self, namespace: lee::AccountId) -> FactorySale {
        FactorySale {
            namespace,
            factory_program_id: self.factory_program_account,
            curve_program_id: self.curve_program_account,
            launch_salt: self.launch.launch_salt,
            collateral_definition: self.collateral_definition.account_id(),
        }
    }

    fn prepared_sale_request(&self, sale: FactorySale) -> Result<CreateSaleRequest> {
        let mut request = self.sale_request();
        request.buy_policy = launchpad_client::curve_buy_policy(sale, self.allowlist_root);
        request.total_supply()?;
        if let Some(file) = &self.distribution_configuration {
            let configuration = read_distribution_configuration(file)?;
            request.graduation_hook = Some(launchpad_client::distribution::hook(
                &configuration,
                self.graduation_hook_program_id.ok_or_else(|| {
                    eyre::eyre!("distribution requires --graduation-hook-program-id")
                })?,
                sale.factory_program_id,
                sale.namespace,
                sale.launch_salt,
            )?);
        }
        Ok(request)
    }

    fn sale_request(&self) -> CreateSaleRequest {
        CreateSaleRequest {
            buy_policy: launchpad_client::BuyPolicy::Open,
            pause_policy: if self.pausable {
                launchpad_client::PausePolicy::AuthorityControlled
            } else {
                launchpad_client::PausePolicy::Disabled
            },
            ending_policy: self.ending_policy.map(Into::into).unwrap_or_else(|| {
                if self.end_timestamp.is_some() {
                    launchpad_client::EndingPolicy::Either
                } else {
                    launchpad_client::EndingPolicy::Depletion
                }
            }),
            trade_mode: self.trade_mode.into(),
            sell_fee_bps: self.sell_fee_bps,
            per_block_token_limit: self.per_block_token_limit,
            graduation_hook: self.graduation_hook_program_id.map(|program_id| {
                launchpad_client::GraduationHook {
                    program_id,
                    configuration: self
                        .graduation_hook_configuration
                        .as_ref()
                        .map_or_else(Vec::new, |configuration| configuration.0.clone()),
                }
            }),
            name: self.name.clone(),
            uri: self.uri.clone(),
            sale_reserve: self.sale_reserve,
            graduation_reserve: self.graduation_reserve,
            graduation_policy: launchpad_client::GraduationPolicy {
                launch_bps: self.graduation_launch_bps.unwrap_or(0),
                collateral_bps: self.graduation_collateral_bps.unwrap_or(0),
            },
            creator_allocation: self.creator_allocation,
            virtual_token_reserve: self.virtual_token_reserve,
            virtual_collateral_reserve: self.virtual_collateral_reserve,
            end_timestamp: self.end_timestamp,
            now: self.now,
        }
    }
}

#[derive(Debug, Clone, Args)]
struct LaunchArgs {
    /// Explicit 32-byte hexadecimal sale salt within the selected namespace.
    #[arg(long, value_parser = parse_launch_salt)]
    launch_salt: [u8; 32],
}

#[derive(Debug, Args)]
struct FactoryTradeArgs {
    #[command(flatten)]
    launch: LaunchArgs,
    #[arg(long = "collateral-definition", value_parser = parse_account_location)]
    collateral_definition: AccountLocation,
    /// Public account that authorizes the trade.
    #[arg(long, value_parser = parse_account_location)]
    participant: AccountLocation,
    #[arg(long = "factory-program-path")]
    factory_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    factory_program_account: lee::AccountId,
    #[arg(long = "curve-program-path")]
    curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    curve_program_account: lee::AccountId,
}

#[derive(Debug, Args)]
struct BuyArgs {
    #[command(flatten)]
    trade: FactoryTradeArgs,
    #[arg(long)]
    tokens: u128,
    #[arg(long = "max-collateral")]
    max_collateral: u128,
}

#[derive(Debug, Args)]
struct BuyWithCollateralArgs {
    #[command(flatten)]
    trade: FactoryTradeArgs,
    #[arg(long)]
    collateral: u128,
    #[arg(long = "min-tokens")]
    min_tokens: u128,
}

#[derive(Debug, Args)]
struct PrivateBuyArgs {
    #[command(flatten)]
    launch: LaunchArgs,
    #[arg(long = "collateral-definition", value_parser = parse_account_location)]
    collateral_definition: AccountLocation,
    #[arg(long)]
    tokens: u128,
    #[arg(long = "max-collateral")]
    max_collateral: u128,
    /// Owned private account providing collateral and native gas.
    #[arg(long = "from-private", value_parser = parse_private_account_id)]
    from_private: PrivateAccountId,
    /// Owned private account that receives the purchased launch tokens.
    #[arg(long = "to-private", value_parser = parse_private_account_id)]
    to_private: PrivateAccountId,
    /// Native units required for the transient public account's transaction fees.
    #[arg(long = "gas-reserve")]
    gas_reserve: u128,
    #[arg(long = "factory-program-path")]
    factory_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    factory_program_account: lee::AccountId,
    #[arg(long = "curve-program-path")]
    curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    curve_program_account: lee::AccountId,
    #[arg(long = "private-buy-program-path")]
    private_buy_program_path: PathBuf,
}

#[derive(Debug, Args)]
struct FactoryLifecycleArgs {
    #[command(flatten)]
    launch: LaunchArgs,
    #[arg(long = "collateral-definition", value_parser = parse_account_location)]
    collateral_definition: AccountLocation,
    #[arg(long, value_parser = parse_account_location)]
    creator: AccountLocation,
    #[arg(long = "factory-program-path")]
    factory_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    factory_program_account: lee::AccountId,
    #[arg(long = "curve-program-path")]
    curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    curve_program_account: lee::AccountId,
}

#[derive(Debug, Args)]
struct GraduationArgs {
    /// Admit saved distribution destinations and validate its typed configuration.
    #[arg(long)]
    distribution: bool,
    /// Current creator role holder. The factory verifies this hint through role authority.
    #[arg(long, value_parser = parse_public_account_id)]
    creator_hint: lee::AccountId,
    #[command(flatten)]
    launch: LaunchArgs,
    #[arg(long, value_parser = parse_public_account_id)]
    collateral_definition: lee::AccountId,
    #[arg(long)]
    factory_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    factory_program_account: lee::AccountId,
    #[arg(long)]
    curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    curve_program_account: lee::AccountId,
    #[arg(long, value_parser = parse_public_account_id)]
    fee_payer: lee::AccountId,
}

#[derive(Debug, Args)]
struct CollectFeesArgs {
    #[arg(long, value_parser = parse_public_account_id)]
    fee_payer: lee::AccountId,
    #[arg(long, required = true, num_args = 1.., value_parser = parse_account_location)]
    pool: Vec<AccountLocation>,
    #[arg(long, value_parser = parse_account_location)]
    collateral_definition: AccountLocation,
    #[arg(long)]
    curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    curve_program_account: lee::AccountId,
}

#[derive(Debug, Args)]
struct LaunchReadArgs {
    #[command(flatten)]
    launch: LaunchArgs,
    #[arg(long = "collateral-definition", value_parser = parse_account_location)]
    collateral_definition: AccountLocation,
    #[arg(long = "factory-program-path")]
    factory_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    factory_program_account: lee::AccountId,
    #[arg(long = "curve-program-path")]
    curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    curve_program_account: lee::AccountId,
}

#[derive(Debug, Args)]
struct SellArgs {
    #[command(flatten)]
    trade: FactoryTradeArgs,
    #[arg(long)]
    tokens: u128,
    #[arg(long = "min-collateral")]
    min_collateral: u128,
}

#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("price-input").required(true).args(["tokens", "collateral"]))) ]
struct PriceArgs {
    /// Quote selling launch tokens for net collateral; requires --tokens.
    #[arg(long, requires = "tokens", conflicts_with = "collateral")]
    sell: bool,
    #[command(flatten)]
    launch: LaunchReadArgs,
    #[arg(long)]
    tokens: Option<u128>,
    /// Collateral to spend for an exact-input purchase quote.
    #[arg(long)]
    collateral: Option<u128>,
}

#[derive(Debug, Args)]
struct ConfigArgs {
    /// Signing namespace AccountId; must equal --namespace.
    #[arg(long, value_parser = parse_public_account_id)]
    admin: lee::AccountId,
    #[arg(long, default_value_t = 0)]
    protocol_fee_bps: u16,
    #[arg(long, value_parser = parse_public_account_id)]
    treasury: lee::AccountId,
    #[arg(long = "curve-program-path")]
    curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    curve_program_account: lee::AccountId,
}

fn parse_account_location(raw: &str) -> Result<AccountLocation, String> {
    raw.parse()
        .map_err(|error: launchpad_client::ClientError| error.to_string())
}

fn parse_public_account_id(raw: &str) -> Result<lee::AccountId, String> {
    match parse_account_location(raw)? {
        AccountLocation::Public(id) => Ok(id),
        AccountLocation::Private(_) => Err("this command requires public accounts".to_owned()),
    }
}

fn parse_private_account_id(raw: &str) -> Result<PrivateAccountId, String> {
    raw.parse()
        .map_err(|error: launchpad_client::ClientError| error.to_string())
}

fn factory_sale(
    namespace: lee::AccountId,
    factory_program: &ProgramHandle,
    curve_program: &ProgramHandle,
    launch_salt: [u8; 32],
    collateral_definition: lee::AccountId,
) -> FactorySale {
    FactorySale {
        namespace,
        factory_program_id: factory_program.account_id,
        curve_program_id: curve_program.account_id,
        launch_salt,
        collateral_definition,
    }
}

fn load_deployment(
    path: &Path,
    account_id: lee::AccountId,
) -> launchpad_client::Result<ProgramHandle> {
    Ok(ProgramHandle {
        account_id,
        image: launchpad_client::load_program(path)?,
    })
}

fn current_time() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("reading the current time")?
        .as_secs())
}

#[derive(Debug, Clone)]
struct HookConfiguration(Vec<u8>);

fn parse_hook_configuration(raw: &str) -> Result<HookConfiguration, String> {
    hex::decode(raw)
        .map(HookConfiguration)
        .map_err(|_| "hook configuration must be hexadecimal bytes".to_owned())
}

fn parse_allowlist_root(raw: &str) -> Result<[u8; 32], String> {
    let mut root = [0; 32];
    hex::decode_to_slice(raw, &mut root)
        .map_err(|_| "allowlist root must contain exactly 64 hexadecimal characters".to_owned())?;
    Ok(root)
}

fn parse_launch_salt(raw: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(raw).map_err(|_| "launch salt must be hexadecimal".to_owned())?;
    bytes
        .try_into()
        .map_err(|_| "launch salt must contain exactly 32 bytes (64 hex characters)".to_owned())
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum QuoteMode {
    Buy,
    Sell,
}

#[derive(Serialize)]
struct PriceQuote {
    namespace: String,
    protocol_fee_bps: u16,
    treasury: String,
    kind: QuoteMode,
    amount_in: u128,
    raw_amount_out: u128,
    amount_out: u128,
    protocol_fee: u128,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let result = run_command(
        Cli::parse(),
        read_client,
        open_wallet,
        &mut io::stdout(),
        &mut io::stderr(),
    )
    .await;
    match result {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            eprintln!("Error writing command result: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn read_client() -> Result<impl WalletSequencerClient> {
    use wallet::{config::WalletConfig, utils::fetch_config_path};
    let path = fetch_config_path().map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))?;
    let config = match std::fs::File::open(&path) {
        Ok(file) => serde_json::from_reader::<_, WalletConfig>(io::BufReader::new(file)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(WalletConfig::default()),
        Err(error) => return Err(error).wrap_err_with(|| format!("opening {}", path.display())),
    }
    .wrap_err_with(|| format!("reading {}", path.display()))?;
    WalletCore::client_from_config(&config)
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))
}

fn open_wallet() -> Result<PublicWriteWallet<impl WalletSequencerClient>> {
    WalletCore::from_env_with_client(WriteTransport::new(read_client()?))
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))
        .context("opening the project wallet")
}

async fn run_command<R: WalletSequencerClient, C: WalletSequencerClient>(
    cli: Cli,
    read_client: impl FnOnce() -> Result<R>,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<u8> {
    let json = cli.json;
    let public_write = matches!(
        cli.command,
        Command::Configure(_)
            | Command::UpdateFee(_)
            | Command::UpdateTreasury(_)
            | Command::TransferAdmin(_)
            | Command::RenounceAdmin(_)
            | Command::CreateSale(_)
            | Command::Withdraw(_)
            | Command::Graduate(_)
            | Command::Claim(_)
            | Command::CollectFees(_)
            | Command::Buy(_)
            | Command::BuyWithCollateral(_)
            | Command::Sell(_)
            | Command::Distribution {
                command: DistributionCommand::Initialize { .. }
                    | DistributionCommand::Remaining { .. },
            }
    );
    match run(json, cli, read_client, open_wallet, output).await {
        Ok(result) => {
            if let Some(result) = result {
                result.write(json, output)?;
            }
            Ok(0)
        }
        Err(error) => {
            use launchpad_client::ClientError;
            let (source, executed) = match error.downcast_ref::<ClientError>() {
                Some(ClientError::WriteInterrupted {
                    executed_hashes,
                    source,
                }) => (Some(source.as_ref()), executed_hashes.as_slice()),
                Some(source @ ClientError::CreationVerificationFailed { hash, .. }) => {
                    (Some(source), std::slice::from_ref(hash))
                }
                source => (source, &[][..]),
            };
            let write_failure = match source {
                Some(ClientError::PublicWritePending { hash, .. }) => {
                    let submission = if hash.is_some() {
                        "acknowledged"
                    } else {
                        "unknown"
                    };
                    Some(("pending", submission, *hash, 3))
                }
                Some(ClientError::PublicWriteReverted { hash }) => {
                    Some(("reverted", "acknowledged", Some(*hash), 1))
                }
                Some(ClientError::CreationVerificationFailed { hash, .. }) => {
                    Some(("verification_failed", "acknowledged", Some(*hash), 1))
                }
                Some(ClientError::PublicWriteRejected(_)) => {
                    Some(("rejected", "not_sent", None, 1))
                }
                _ if public_write => Some(("preparation_failed", "not_sent", None, 1)),
                _ => None,
            };
            if let Some((status, submission, hash, code)) = write_failure {
                let hash = hash.map(hex::encode);
                let executed = executed.iter().map(hex::encode).collect::<Vec<_>>();
                if json {
                    let (category, message) = if executed.is_empty() {
                        (command_error_category(&error), error.to_string())
                    } else {
                        let cause = source.expect("interrupted write has a cause");
                        (cause.into(), cause.to_string())
                    };
                    let view = serde_json::json!({
                        "status": status,
                        "submission": submission,
                        "transaction_hash": hash,
                        "executed_transaction_hashes": executed,
                        "error": { "category": category, "message": message },
                    });
                    writeln!(output, "{view}")?;
                } else {
                    write!(errors, "{status}: submission={submission}")?;
                    if let Some(hash) = hash {
                        write!(errors, " tx_hash={hash}")?;
                    }
                    writeln!(errors, ": {error:#}")?;
                    for executed_hash in executed {
                        writeln!(errors, "executed tx_hash={executed_hash}")?;
                    }
                }
                return Ok(code);
            }
            if json {
                let view = serde_json::json!({
                    "status": "error",
                    "error": {
                        "category": command_error_category(&error),
                        "message": error.to_string(),
                    },
                });
                writeln!(output, "{view}")?;
            } else {
                writeln!(errors, "Error: {error:#}")?;
            }
            Ok(1)
        }
    }
}

async fn run<R: WalletSequencerClient, C: WalletSequencerClient>(
    json: bool,
    cli: Cli,
    read_client: impl FnOnce() -> Result<R>,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
    output: &mut impl Write,
) -> Result<Option<WriteSuccess>> {
    cli.validate_route()?;
    let router = cli.authority_router_path.as_deref();
    if let Command::Distribution { command } = &cli.command {
        match command {
            DistributionCommand::Encode { file } => {
                let config = read_distribution_configuration(file)?;
                let bytes = launchpad_client::distribution::encode_configuration(&config)?;
                let encoded = hex::encode(bytes);
                if json {
                    writeln!(
                        output,
                        "{}",
                        serde_json::json!({ "configuration": encoded })
                    )?;
                } else {
                    writeln!(output, "{encoded}")?;
                }
                return Ok(None);
            }
            DistributionCommand::Preview {
                file,
                launch_balance,
                collateral_balance,
            } => {
                let config = read_distribution_configuration(file)?;
                let plan = config
                    .plan([*launch_balance, *collateral_balance])
                    .map_err(|error| eyre::eyre!("{error:?}"))?;
                writeln!(output, "{}", serde_json::to_string_pretty(&plan)?)?;
                return Ok(None);
            }
            _ => {}
        }
    }
    let namespace = cli
        .namespace
        .ok_or_else(|| eyre::eyre!("--namespace is required; use the stable namespace AccountId"))?
        .account_id();
    let receipt = match cli.command {
        Command::Distribution { command } => {
            run_distribution(namespace, command, open_wallet).await?
        }
        Command::CreateSale(args) => create_sale(namespace, args, router, open_wallet).await?,
        Command::CollectFees(args) => {
            let wallet = open_wallet()?;
            let curve = load_deployment(&args.curve_program_path, args.curve_program_account)?;
            let collateral_definition = args.collateral_definition.account_id();
            let pools = args
                .pool
                .into_iter()
                .map(|pool| PoolFeeTarget {
                    pool: pool.account_id(),
                    collateral_definition,
                })
                .collect::<Vec<_>>();
            let hashes = launchpad_client::collect_pool_fees(
                &wallet,
                namespace,
                &curve,
                &pools,
                args.fee_payer,
            )
            .await?;
            WriteSuccess::new(hashes)
        }
        Command::Withdraw(args) => {
            withdraw_factory_proceeds(namespace, args, router, open_wallet).await?
        }
        Command::Graduate(args) => graduate(namespace, args, open_wallet).await?,
        Command::Claim(args) => claim_creator_allocation(namespace, args, open_wallet).await?,
        Command::Buy(args) => buy(namespace, args, open_wallet).await?,
        Command::BuyWithCollateral(args) => {
            buy_with_collateral(namespace, args, open_wallet).await?
        }
        Command::PrivateBuy(args) => return private_buy(namespace, args).await.map(|()| None),
        Command::Sell(args) => sell(namespace, args, open_wallet).await?,
        Command::Price(args) => return price(namespace, json, args).await.map(|()| None),
        Command::SaleInfo(args) => {
            sale_info::show(namespace, args, &read_client()?, json, output).await?;
            return Ok(None);
        }
        Command::Configure(args) => configure(namespace, args, open_wallet).await?,
        Command::CreateAuthority(_) | Command::TransferAuthority(_) => unreachable!(),
        Command::NamespaceInfo(args) => {
            let program = load_deployment(&args.curve_program_path, args.curve_program_account)?;
            let wallet = open_wallet()?;
            let config = load_curve_config(namespace, &wallet, program.account_id).await?;
            writeln!(
                output,
                "{}",
                serde_json::json!({"namespace": namespace.to_string(), "protocol_fee_bps": config.protocol_fee_bps, "treasury": config.treasury.to_string()})
            )?;
            return Ok(None);
        }
        Command::UpdateFee(args) => {
            namespace_write(
                namespace,
                &args.authority.curve_program_path,
                args.authority.curve_program_account,
                NamespaceAction::UpdateFee {
                    admin: args.authority.admin,
                    protocol_fee_bps: args.protocol_fee_bps,
                },
                open_wallet,
            )
            .await?
        }
        Command::UpdateTreasury(args) => {
            namespace_write(
                namespace,
                &args.authority.curve_program_path,
                args.authority.curve_program_account,
                NamespaceAction::UpdateTreasury {
                    admin: args.authority.admin,
                    treasury: args.treasury,
                },
                open_wallet,
            )
            .await?
        }
        Command::TransferAdmin(args) => {
            namespace_write(
                namespace,
                &args.authority.curve_program_path,
                args.authority.curve_program_account,
                NamespaceAction::TransferAdmin {
                    admin: args.authority.admin,
                    new_admin: args.new_admin,
                },
                open_wallet,
            )
            .await?
        }
        Command::RenounceAdmin(args) => {
            if !args.confirm_renounce {
                eyre::bail!(
                    "renounce-admin permanently removes the namespace administrator; pass --confirm-renounce to proceed"
                );
            }
            namespace_write(
                namespace,
                &args.authority.curve_program_path,
                args.authority.curve_program_account,
                NamespaceAction::RenounceAdmin {
                    admin: args.authority.admin,
                },
                open_wallet,
            )
            .await?
        }
    };
    Ok(Some(receipt))
}

fn command_error_category(error: &eyre::Report) -> &'static str {
    match error_category(error) {
        ErrorCategory::General => error
            .downcast_ref::<launchpad_client::ClientError>()
            .map_or("general", Into::into),
        category => category.into(),
    }
}

fn error_category(error: &eyre::Report) -> ErrorCategory {
    if let Some(category) = error.downcast_ref::<ErrorCategory>() {
        return *category;
    }
    let Some(error) = error.downcast_ref::<launchpad_client::ClientError>() else {
        return ErrorCategory::General;
    };
    use launchpad_client::ClientError;
    use pool::SwapError;
    match error {
        ClientError::QuoteBuy(SwapError::SlippageExceeded)
        | ClientError::QuoteBuyWithCollateral(SwapError::SlippageExceeded)
        | ClientError::QuoteSell(SwapError::SlippageExceeded) => ErrorCategory::SlippageFloor,
        ClientError::QuoteBuy(SwapError::InsufficientRealOutputReserve)
        | ClientError::QuoteBuyWithCollateral(SwapError::InsufficientRealOutputReserve) => {
            ErrorCategory::SaleReserveOvershoot
        }
        ClientError::QuoteSell(SwapError::InsufficientRealOutputReserve) => {
            ErrorCategory::CollateralReserveOvershoot
        }
        _ => ErrorCategory::General,
    }
}

async fn configure<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: ConfigArgs,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    namespace_write(
        namespace,
        &args.curve_program_path,
        args.curve_program_account,
        NamespaceAction::Initialize {
            protocol_fee_bps: args.protocol_fee_bps,
            treasury: args.treasury,
        },
        open_wallet,
    )
    .await
}

async fn namespace_write<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    program_path: &Path,
    program_account: lee::AccountId,
    action: NamespaceAction,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let program = load_deployment(program_path, program_account)?;
    let wallet = open_wallet()?;
    let receipt = submit_namespace_action(&wallet, &program, namespace, action).await?;
    Ok(WriteSuccess::namespace(receipt))
}

async fn withdraw_factory_proceeds<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: FactoryLifecycleArgs,
    router: Option<&Path>,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let factory_program =
        load_deployment(&args.factory_program_path, args.factory_program_account)?;
    let curve_program = load_deployment(&args.curve_program_path, args.curve_program_account)?;
    let sale = factory_sale(
        namespace,
        &factory_program,
        &curve_program,
        args.launch.launch_salt,
        args.collateral_definition.account_id(),
    );
    let wallet = open_wallet()?;
    if router.is_some() {
        return Err(launchpad_client::ClientError::UnsupportedPrivateAuthority.into());
    }
    let receipt = launchpad_client::FactorySession::new(
        &wallet,
        &factory_program,
        &curve_program,
        None,
        namespace,
        args.creator,
    )
    .withdraw(sale, current_time()?)
    .await?;
    Ok(WriteSuccess::lifecycle(receipt, args.launch.launch_salt))
}

async fn graduate<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: GraduationArgs,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let factory = load_deployment(&args.factory_program_path, args.factory_program_account)?;
    let curve = load_deployment(&args.curve_program_path, args.curve_program_account)?;
    let sale = factory_sale(
        namespace,
        &factory,
        &curve,
        args.launch.launch_salt,
        args.collateral_definition,
    );
    let wallet = open_wallet()?;
    let state = load_factory_state(&wallet, sale).await?;
    state
        .graduation_hook
        .as_ref()
        .ok_or(launchpad_client::ClientError::GraduationNotConfigured)?;
    let treasury = launchpad_client::prepare_fee_collection(
        &wallet,
        namespace,
        curve.account_id,
        sale.collateral_definition,
    )
    .await?;
    let target = launchpad_client::GraduationTarget::Curve {
        sale,
        state: &state,
        creator_hint: args.creator_hint,
    };
    let mut invocation = if args.distribution {
        launchpad_client::distribution::graduation(target, treasury)?
    } else {
        launchpad_client::build_graduation_invocation(target, treasury)?
    };
    invocation.fee_payer = Some(args.fee_payer);
    let transaction_hash = submit_public_invocation(&wallet, &factory, invocation).await?;
    Ok(WriteSuccess::single(
        transaction_hash,
        Some(args.launch.launch_salt),
    ))
}

fn read_distribution_configuration(
    path: &Path,
) -> Result<launchpad_client::distribution::Configuration> {
    let config = serde_json::from_slice(&std::fs::read(path)?)?;
    launchpad_client::distribution::encode_configuration(&config)?;
    Ok(config)
}

async fn run_distribution<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    command: DistributionCommand,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    use launchpad_client::distribution;
    let (identity, kind) = match command {
        DistributionCommand::Initialize { adapter, kind } => (adapter, Some(kind)),
        DistributionCommand::Remaining { adapter } => (adapter, None),
        _ => unreachable!("offline commands handled before wallet setup"),
    };
    let adapter = load_deployment(
        &identity.adapter_program_path,
        identity.adapter_program_account,
    )?;
    let wallet = open_wallet()?;
    let mut call = if let Some(kind) = kind {
        let kind = match kind {
            DistributionKind::Curve => distribution::GraduationIntentKind::Curve,
            DistributionKind::FixedPrice => distribution::GraduationIntentKind::FixedPrice,
            DistributionKind::Weighted => distribution::GraduationIntentKind::Weighted,
        };
        distribution::initialize(
            adapter.account_id,
            identity.factory_program_account,
            namespace,
            identity.launch.launch_salt,
            kind,
        )
    } else {
        let state = distribution::load_state(
            &wallet,
            adapter.account_id,
            identity.factory_program_account,
            namespace,
            identity.launch.launch_salt,
        )
        .await?;
        distribution::distribute_remaining(adapter.account_id, &state)
            .map_err(|error| eyre::eyre!("{error:?}"))?
    };
    call.fee_payer = Some(identity.fee_payer);
    let transaction_hash = submit_public_invocation(&wallet, &adapter, call).await?;
    Ok(WriteSuccess::single(
        transaction_hash,
        Some(identity.launch.launch_salt),
    ))
}

async fn price(namespace: lee::AccountId, json: bool, args: PriceArgs) -> Result<()> {
    let factory_program = load_deployment(
        &args.launch.factory_program_path,
        args.launch.factory_program_account,
    )?;
    let curve_program = load_deployment(
        &args.launch.curve_program_path,
        args.launch.curve_program_account,
    )?;
    let sale = factory_sale(
        namespace,
        &factory_program,
        &curve_program,
        args.launch.launch.launch_salt,
        args.launch.collateral_definition.account_id(),
    );
    let wallet = WalletCore::from_env()
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))
        .context("opening the project wallet")?;
    let pool = load_factory_pool(&wallet, sale).await?;
    let config = load_curve_config(namespace, &wallet, curve_program.account_id).await?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("reading the current time")?
        .as_secs();
    let observed_block = if !args.sell && pool.block_allocation.limit.is_some() {
        Some(load_clock_block_id(&wallet).await?)
    } else {
        None
    };
    let (kind, quote) = match (args.tokens, args.collateral) {
        (Some(tokens), None) if args.sell => {
            (QuoteMode::Sell, quote_sell(&pool, &config, tokens, now)?)
        }
        (Some(tokens), None) => (
            QuoteMode::Buy,
            quote_buy(&pool, &config, tokens, now, observed_block)?,
        ),
        (None, Some(collateral)) => (
            QuoteMode::Buy,
            quote_buy_with_collateral(&pool, &config, collateral, now, observed_block)?,
        ),
        _ => eyre::bail!("provide exactly one of --tokens or --collateral"),
    };
    let output = PriceQuote {
        namespace: namespace.to_string(),
        protocol_fee_bps: config.protocol_fee_bps,
        treasury: config.treasury.to_string(),
        kind,
        amount_in: quote.amount_in,
        raw_amount_out: quote.raw_amount_out,
        amount_out: quote.amount_out,
        protocol_fee: quote.protocol_fee,
    };
    if json {
        println!("{}", serde_json::to_string(&output)?);
    } else {
        let kind = match output.kind {
            QuoteMode::Buy => "buy",
            QuoteMode::Sell => "sell",
        };
        println!(
            "{kind} quote: namespace={} fee_bps={} treasury={} input={} raw_output={} output={} protocol_fee={}",
            output.namespace,
            output.protocol_fee_bps,
            output.treasury,
            output.amount_in,
            output.raw_amount_out,
            output.amount_out,
            output.protocol_fee
        );
    }
    Ok(())
}

async fn buy<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: BuyArgs,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let factory_program = load_deployment(
        &args.trade.factory_program_path,
        args.trade.factory_program_account,
    )?;
    let curve_program = load_deployment(
        &args.trade.curve_program_path,
        args.trade.curve_program_account,
    )?;
    let sale = factory_sale(
        namespace,
        &factory_program,
        &curve_program,
        args.trade.launch.launch_salt,
        args.trade.collateral_definition.account_id(),
    );
    let participant = args.trade.participant.account_id();
    let wallet = open_wallet()?;
    let config = load_curve_config(namespace, &wallet, curve_program.account_id).await?;
    let pool = load_factory_pool(&wallet, sale).await?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("reading the current time")?
        .as_secs();
    let observed_block = if pool.block_allocation.limit.is_some() {
        Some(load_clock_block_id(&wallet).await?)
    } else {
        None
    };
    let invocation = prepare_buy_invocation(
        sale,
        participant,
        &pool,
        &config,
        ExactOutputTradeTerms {
            amount_out: args.tokens,
            max_amount_in: args.max_collateral,
        },
        None,
        now,
        observed_block,
    )?;
    let transaction_hash = submit_public_invocation(&wallet, &curve_program, invocation).await?;
    Ok(WriteSuccess::single(
        transaction_hash,
        Some(args.trade.launch.launch_salt),
    ))
}

async fn buy_with_collateral<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: BuyWithCollateralArgs,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let factory_program = load_deployment(
        &args.trade.factory_program_path,
        args.trade.factory_program_account,
    )?;
    let curve_program = load_deployment(
        &args.trade.curve_program_path,
        args.trade.curve_program_account,
    )?;
    let sale = factory_sale(
        namespace,
        &factory_program,
        &curve_program,
        args.trade.launch.launch_salt,
        args.trade.collateral_definition.account_id(),
    );
    let participant = args.trade.participant.account_id();
    let wallet = open_wallet()?;
    let config = load_curve_config(namespace, &wallet, curve_program.account_id).await?;
    let pool = load_factory_pool(&wallet, sale).await?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("reading the current time")?
        .as_secs();
    let observed_block = if pool.block_allocation.limit.is_some() {
        Some(load_clock_block_id(&wallet).await?)
    } else {
        None
    };
    let invocation = prepare_buy_with_collateral_invocation(
        sale,
        participant,
        &pool,
        &config,
        ExactInputTradeTerms {
            amount_in: args.collateral,
            min_amount_out: args.min_tokens,
        },
        None,
        now,
        observed_block,
    )?;
    let transaction_hash = submit_public_invocation(&wallet, &curve_program, invocation).await?;
    Ok(WriteSuccess::single(
        transaction_hash,
        Some(args.trade.launch.launch_salt),
    ))
}

async fn private_buy(_namespace: lee::AccountId, args: PrivateBuyArgs) -> Result<()> {
    launchpad_client::validate_private_buy_request(PrivateBuyRequest {
        amount_out: args.tokens,
        max_collateral_in: args.max_collateral,
        from_private: args.from_private,
        to_private: args.to_private,
        gas_reserve: args.gas_reserve,
    })?;
    Err(launchpad_client::ClientError::UnsupportedPrivateRoute.into())
}

async fn sell<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: SellArgs,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let factory_program = load_deployment(
        &args.trade.factory_program_path,
        args.trade.factory_program_account,
    )?;
    let curve_program = load_deployment(
        &args.trade.curve_program_path,
        args.trade.curve_program_account,
    )?;
    let sale = factory_sale(
        namespace,
        &factory_program,
        &curve_program,
        args.trade.launch.launch_salt,
        args.trade.collateral_definition.account_id(),
    );
    let participant = args.trade.participant.account_id();
    let wallet = open_wallet()?;
    let config = load_curve_config(namespace, &wallet, curve_program.account_id).await?;
    let pool = load_factory_pool(&wallet, sale).await?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("reading the current time")?
        .as_secs();
    let invocation = prepare_sell_invocation(
        sale,
        participant,
        &pool,
        &config,
        ExactInputTradeTerms {
            amount_in: args.tokens,
            min_amount_out: args.min_collateral,
        },
        now,
    )?;
    let transaction_hash = submit_public_invocation(&wallet, &curve_program, invocation).await?;
    Ok(WriteSuccess::single(
        transaction_hash,
        Some(args.trade.launch.launch_salt),
    ))
}

async fn claim_creator_allocation<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: FactoryLifecycleArgs,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let factory_program =
        load_deployment(&args.factory_program_path, args.factory_program_account)?;
    let curve_program = load_deployment(&args.curve_program_path, args.curve_program_account)?;
    let sale = factory_sale(
        namespace,
        &factory_program,
        &curve_program,
        args.launch.launch_salt,
        args.collateral_definition.account_id(),
    );

    let AccountLocation::Public(creator) = args.creator else {
        return Err(launchpad_client::ClientError::UnsupportedPrivateAuthority.into());
    };
    let wallet = open_wallet()?;
    load_factory_state(&wallet, sale).await?;
    let call = build_claim_creator_allocation_invocation(sale, creator, current_time()?);
    let transaction_hash = submit_public_invocation(&wallet, &factory_program, call).await?;
    Ok(WriteSuccess::single(
        transaction_hash,
        Some(args.launch.launch_salt),
    ))
}

async fn create_sale<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: CreateSaleArgs,
    router: Option<&Path>,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let sale = args.sale(namespace);
    let request = args.prepared_sale_request(sale)?;
    let factory_program =
        load_deployment(&args.factory_program_path, args.factory_program_account)?;
    let curve_program = load_deployment(&args.curve_program_path, args.curve_program_account)?;
    let wallet = open_wallet()?;
    if router.is_some() {
        return Err(launchpad_client::ClientError::UnsupportedPrivateAuthority.into());
    }
    let receipt = launchpad_client::FactorySession::new(
        &wallet,
        &factory_program,
        &curve_program,
        None,
        namespace,
        args.creator,
    )
    .create_sale(sale, request)
    .await?;
    Ok(WriteSuccess::creation(receipt, sale.launch_salt))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    #[test]
    fn create_authority_requires_a_public_fee_payer() {
        let command = [
            "launchpad",
            "create-authority",
            "--token-program-path",
            "token.bin",
            "--name",
            "Creator",
            "--uri",
            "https://example.invalid/creator",
        ];
        let error = Cli::try_parse_from(command).unwrap_err();
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
        assert!(error.to_string().contains("--fee-payer"));
        for payer in [
            "11111111111111111111111111111111",
            "Public/11111111111111111111111111111111",
        ] {
            let cli =
                Cli::try_parse_from(command.into_iter().chain(["--fee-payer", payer])).unwrap();
            let Command::CreateAuthority(args) = cli.command else {
                panic!("expected create-authority");
            };
            assert_eq!(args.fee_payer, parse_public_account_id(payer).unwrap());
        }
        assert!(
            Cli::try_parse_from(
                command
                    .into_iter()
                    .chain(["--fee-payer", "Private/11111111111111111111111111111111",])
            )
            .is_err()
        );
    }

    #[test]
    fn legacy_authority_command_cannot_claim_to_create_a_launch_role() {
        let cli = Cli::try_parse_from([
            "launchpad",
            "create-authority",
            "--token-program-path",
            "token.bin",
            "--fee-payer",
            "11111111111111111111111111111111",
            "--name",
            "Creator",
            "--uri",
            "https://example.invalid/creator",
        ])
        .unwrap();
        assert!(
            cli.validate_route()
                .unwrap_err()
                .to_string()
                .contains("role authority")
        );
    }

    #[test]
    fn private_nft_transfer_is_rejected_before_wallet_access() {
        let cli = Cli::try_parse_from([
            "launchpad",
            "transfer-authority",
            "--token-program-path",
            "token.bin",
            "--definition",
            "11111111111111111111111111111111",
            "--from",
            "Public/11111111111111111111111111111111",
            "--to",
            "Private/11111111111111111111111111111111",
        ])
        .unwrap();
        let error = cli.validate_route().unwrap_err();
        assert!(matches!(
            error.downcast_ref::<launchpad_client::ClientError>(),
            Some(launchpad_client::ClientError::UnsupportedPrivateAuthority)
        ));
    }

    #[test]
    fn private_creator_lifecycle_is_rejected_before_wallet_access() {
        let salt = "11".repeat(32);
        for command in ["withdraw", "claim"] {
            let cli = Cli::try_parse_from([
                "launchpad",
                command,
                "--launch-salt",
                &salt,
                "--collateral-definition",
                "11111111111111111111111111111111",
                "--creator",
                "Private/11111111111111111111111111111111",
                "--factory-program-path",
                "factory.bin",
                "--factory-program-account",
                "11111111111111111111111111111111",
                "--curve-program-path",
                "curve.bin",
                "--curve-program-account",
                "11111111111111111111111111111111",
            ])
            .unwrap();
            assert!(matches!(
                cli.validate_route()
                    .unwrap_err()
                    .downcast_ref::<launchpad_client::ClientError>(),
                Some(launchpad_client::ClientError::UnsupportedPrivateAuthority)
            ));
        }
    }

    #[test]
    fn private_sale_selectors_cannot_alias_public_actors() {
        let salt = "11".repeat(32);
        let public = "11111111111111111111111111111111";
        let private = "Private/11111111111111111111111111111111";
        for (namespace, collateral) in [(private, public), (public, private)] {
            let cli = Cli::try_parse_from([
                "launchpad",
                "--namespace",
                namespace,
                "buy",
                "--launch-salt",
                &salt,
                "--tokens",
                "1",
                "--max-collateral",
                "2",
                "--collateral-definition",
                collateral,
                "--participant",
                public,
                "--factory-program-path",
                "factory.bin",
                "--factory-program-account",
                public,
                "--curve-program-path",
                "curve.bin",
                "--curve-program-account",
                public,
            ])
            .unwrap();
            assert!(cli.validate_route().is_err());
        }
    }

    #[test]
    fn public_trade_rejects_private_participant_before_wallet_access() {
        let salt = "11".repeat(32);
        let cli = Cli::try_parse_from([
            "launchpad",
            "buy",
            "--launch-salt",
            &salt,
            "--tokens",
            "1",
            "--max-collateral",
            "2",
            "--collateral-definition",
            "11111111111111111111111111111111",
            "--participant",
            "Private/11111111111111111111111111111111",
            "--factory-program-path",
            "factory.bin",
            "--factory-program-account",
            "11111111111111111111111111111111",
            "--curve-program-path",
            "curve.bin",
            "--curve-program-account",
            "11111111111111111111111111111111",
        ])
        .unwrap();
        assert!(cli.validate_route().is_err());
    }

    #[test]
    fn legacy_authority_syntax_and_private_namespace_admin_are_distinct() {
        assert!(
            Cli::try_parse_from([
                "launchpad",
                "create-authority",
                "--token-program-path",
                "token.bin",
                "--fee-payer",
                "11111111111111111111111111111111",
                "--name",
                "Admin",
                "--uri",
                "https://example.invalid/admin"
            ])
            .is_ok()
        );
        assert!(
            Cli::try_parse_from([
                "launchpad",
                "--namespace",
                "11111111111111111111111111111111",
                "--authority-router-path",
                "private_authority.bin",
                "configure",
                "--curve-program-path",
                "curve.bin",
                "--curve-program-account",
                "11111111111111111111111111111111",
                "--admin",
                "Private/11111111111111111111111111111111",
                "--treasury",
                "Public/11111111111111111111111111111111"
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "launchpad",
                "transfer-authority",
                "--token-program-path",
                "token.bin",
                "--definition",
                "11111111111111111111111111111111",
                "--from",
                "Public/11111111111111111111111111111111",
                "--to",
                "Private/11111111111111111111111111111111"
            ])
            .is_ok()
        );
    }

    #[test]
    fn namespace_admin_commands_accept_public_initialization_and_renunciation() {
        let cli = Cli::try_parse_from([
            "launchpad",
            "--namespace",
            "Public/11111111111111111111111111111111",
            "configure",
            "--curve-program-path",
            "curve.bin",
            "--curve-program-account",
            "11111111111111111111111111111111",
            "--admin",
            "Public/11111111111111111111111111111111",
            "--protocol-fee-bps",
            "0",
            "--treasury",
            "Public/11111111111111111111111111111111",
        ])
        .unwrap();
        assert!(matches!(cli.namespace, Some(AccountLocation::Public(_))));
        assert!(cli.validate_route().is_ok());
        let Command::Configure(config) = cli.command else {
            panic!("configure command")
        };
        assert_eq!(config.admin, cli.namespace.unwrap().account_id());
        for command in ["namespace-info", "renounce-admin"] {
            let mut args = vec![
                "launchpad",
                "--namespace",
                "Public/11111111111111111111111111111111",
                command,
                "--curve-program-path",
                "curve.bin",
                "--curve-program-account",
                "11111111111111111111111111111111",
            ];
            if command == "renounce-admin" {
                args.extend(["--admin", "Public/11111111111111111111111111111111"]);
            }
            assert!(Cli::try_parse_from(args).is_ok());
        }
    }

    #[test]
    fn launchpad_exposes_creator_and_participant_commands() {
        let help = Cli::command().render_help().to_string();
        for command in [
            "create-sale",
            "withdraw",
            "claim",
            "buy",
            "sell",
            "price",
            "sale-info",
        ] {
            assert!(help.contains(command), "missing command: {command}");
        }
        assert!(
            !Cli::command()
                .get_subcommands()
                .any(|command| command.get_name() == "close")
        );
    }

    #[test]
    fn buy_requires_exact_output_and_collateral_cap() {
        let cli = Cli::try_parse_from([
            "launchpad",
            "buy",
            "--launch-salt",
            &"11".repeat(32),
            "--tokens",
            "25",
            "--max-collateral",
            "100",
            "--collateral-definition",
            "11111111111111111111111111111111",
            "--participant",
            "Public/11111111111111111111111111111111",
            "--factory-program-path",
            "factory.bin",
            "--factory-program-account",
            "11111111111111111111111111111111",
            "--curve-program-path",
            "curve.bin",
            "--curve-program-account",
            "11111111111111111111111111111111",
        ])
        .expect("buy command should parse");
        assert!(matches!(
            cli.command,
            Command::Buy(BuyArgs {
                tokens: 25,
                max_collateral: 100,
                ..
            })
        ));
    }

    #[test]
    fn private_buy_requires_private_source_and_explicit_gas_reserve() {
        let command = Cli::command();
        let private_buy = command
            .get_subcommands()
            .find(|subcommand| subcommand.get_name() == "private-buy")
            .expect("private buy command should exist");

        assert!(
            private_buy
                .get_arguments()
                .any(|argument| argument.get_long() == Some("from-private")
                    && argument.is_required_set())
        );
        assert!(
            private_buy
                .get_arguments()
                .any(|argument| argument.get_long() == Some("gas-reserve")
                    && argument.is_required_set())
        );
        assert!(
            private_buy
                .get_arguments()
                .any(|argument| argument.get_long() == Some("to-private")
                    && argument.is_required_set())
        );
        assert!(
            private_buy
                .get_arguments()
                .any(
                    |argument| argument.get_long() == Some("private-buy-program-path")
                        && argument.is_required_set()
                )
        );
    }

    #[test]
    fn selectors_parse_at_the_cli_boundary() {
        assert!(matches!(
            parse_account_location("11111111111111111111111111111111"),
            Ok(AccountLocation::Public(_))
        ));
        assert!(matches!(
            parse_account_location("Private/11111111111111111111111111111111"),
            Ok(AccountLocation::Private(_))
        ));
        assert!(parse_account_location("Unknown/11111111111111111111111111111111").is_err());
        assert!(parse_private_account_id("11111111111111111111111111111111").is_ok());
        assert!(parse_private_account_id("Public/11111111111111111111111111111111").is_err());
    }

    #[test]
    fn sell_accepts_the_factory_and_curve_context() {
        let cli = Cli::try_parse_from([
            "launchpad",
            "sell",
            "--launch-salt",
            &"11".repeat(32),
            "--tokens",
            "25",
            "--min-collateral",
            "10",
            "--collateral-definition",
            "11111111111111111111111111111111",
            "--participant",
            "Public/11111111111111111111111111111111",
            "--factory-program-path",
            "factory.bin",
            "--factory-program-account",
            "11111111111111111111111111111111",
            "--curve-program-path",
            "curve.bin",
            "--curve-program-account",
            "11111111111111111111111111111111",
        ])
        .expect("sell command should parse");
        assert!(matches!(cli.command, Command::Sell(_)));
    }

    #[test]
    fn collateral_buy_accepts_a_collateral_budget_and_token_floor() {
        let cli = Cli::try_parse_from([
            "launchpad",
            "buy-with-collateral",
            "--launch-salt",
            &"11".repeat(32),
            "--collateral",
            "25",
            "--min-tokens",
            "100",
            "--collateral-definition",
            "11111111111111111111111111111111",
            "--participant",
            "Public/11111111111111111111111111111111",
            "--factory-program-path",
            "factory.bin",
            "--factory-program-account",
            "11111111111111111111111111111111",
            "--curve-program-path",
            "curve.bin",
            "--curve-program-account",
            "11111111111111111111111111111111",
        ])
        .expect("collateral buy command should parse");
        assert!(matches!(cli.command, Command::BuyWithCollateral(_)));
    }

    #[test]
    fn graduation_uses_a_typed_callback_without_arbitrary_adapter_accounts() {
        let salt = "11".repeat(32);
        let mut base = vec![
            "launchpad",
            "graduate",
            "--creator-hint",
            "11111111111111111111111111111111",
            "--launch-salt",
            &salt,
            "--collateral-definition",
            "11111111111111111111111111111111",
            "--factory-program-path",
            "factory.bin",
            "--factory-program-account",
            "11111111111111111111111111111111",
            "--curve-program-path",
            "curve.bin",
            "--curve-program-account",
            "11111111111111111111111111111111",
            "--fee-payer",
            "11111111111111111111111111111111",
        ];
        assert!(Cli::try_parse_from(&base).is_ok());
        base.extend(["--adapter-account", "11111111111111111111111111111111"]);
        assert!(Cli::try_parse_from(base).is_err());
    }

    #[test]
    fn graduation_rejects_private_accounts_and_authority_routing() {
        let salt = "11".repeat(32);
        let base = vec![
            "launchpad",
            "graduate",
            "--creator-hint",
            "11111111111111111111111111111111",
            "--fee-payer",
            "11111111111111111111111111111111",
            "--launch-salt",
            &salt,
            "--collateral-definition",
            "Public/11111111111111111111111111111111",
            "--factory-program-path",
            "factory.bin",
            "--factory-program-account",
            "11111111111111111111111111111111",
            "--curve-program-path",
            "curve.bin",
            "--curve-program-account",
            "11111111111111111111111111111111",
        ];
        assert!(Cli::try_parse_from(&base).unwrap().validate_route().is_ok());
        let mut routed = base.clone();
        routed.extend(["--authority-router-path", "private_authority.bin"]);
        assert!(
            Cli::try_parse_from(routed)
                .unwrap()
                .validate_route()
                .is_err()
        );
        let mut private_namespace = base.clone();
        private_namespace.extend(["--namespace", "Private/11111111111111111111111111111111"]);
        assert!(
            Cli::try_parse_from(private_namespace)
                .unwrap()
                .validate_route()
                .is_err()
        );
        let mut private_adapter = base.clone();
        private_adapter.extend([
            "--adapter-account",
            "Private/11111111111111111111111111111111",
        ]);
        assert!(Cli::try_parse_from(private_adapter).is_err());
        let mut private_collateral = base;
        private_collateral[7] = "Private/11111111111111111111111111111111";
        assert!(Cli::try_parse_from(private_collateral).is_err());
    }

    #[test]
    fn withdraw_accepts_the_factory_lifecycle_context() {
        let cli = Cli::try_parse_from([
            "launchpad",
            "withdraw",
            "--launch-salt",
            &"11".repeat(32),
            "--collateral-definition",
            "11111111111111111111111111111111",
            "--creator",
            "Public/11111111111111111111111111111111",
            "--factory-program-path",
            "factory.bin",
            "--factory-program-account",
            "11111111111111111111111111111111",
            "--curve-program-path",
            "curve.bin",
            "--curve-program-account",
            "11111111111111111111111111111111",
        ])
        .expect("withdraw command should parse");
        assert!(matches!(cli.command, Command::Withdraw(_)));
    }

    #[test]
    fn create_sale_requires_trade_mode_with_only_the_two_named_values() {
        let command = Cli::command();
        let create = command
            .get_subcommands()
            .find(|command| command.get_name() == "create-sale")
            .unwrap();
        let mode = create
            .get_arguments()
            .find(|arg| arg.get_long() == Some("trade-mode"))
            .expect("trade mode argument");
        assert!(mode.is_required_set());
        let values: Vec<_> = mode
            .get_value_parser()
            .possible_values()
            .unwrap()
            .map(|value| value.get_name().to_owned())
            .collect();
        assert_eq!(values, ["buy-only", "buy-and-sell"]);
    }

    #[test]
    fn create_sale_accepts_the_program_binaries_and_creator_account() {
        let cli = Cli::try_parse_from([
            "launchpad",
            "create-sale",
            "--trade-mode",
            "buy-and-sell",
            "--launch-salt",
            &"11".repeat(32),
            "--name",
            "E2E token",
            "--uri",
            "https://example.invalid/e2e-token.json",
            "--sale-reserve",
            "800",
            "--creator-allocation",
            "50",
            "--virtual-token-reserve",
            "2000",
            "--virtual-collateral-reserve",
            "100",
            "--collateral-definition",
            "11111111111111111111111111111111",
            "--creator",
            "Public/11111111111111111111111111111111",
            "--factory-program-path",
            "factory.bin",
            "--factory-program-account",
            "11111111111111111111111111111111",
            "--curve-program-path",
            "curve.bin",
            "--curve-program-account",
            "11111111111111111111111111111111",
        ])
        .expect("create sale command should parse");
        assert!(matches!(cli.command, Command::CreateSale(_)));
    }

    #[test]
    fn create_sale_saves_optional_hook_program_and_hex_configuration() {
        let salt = "11".repeat(32);
        let program = lee::AccountId::new([1; 32]).to_string();
        let mut arguments = vec![
            "launchpad",
            "create-sale",
            "--trade-mode",
            "buy-only",
            "--launch-salt",
            &salt,
            "--name",
            "Launch",
            "--uri",
            "https://example.invalid/token",
            "--sale-reserve",
            "800",
            "--creator-allocation",
            "100",
            "--virtual-token-reserve",
            "2000",
            "--virtual-collateral-reserve",
            "100",
            "--collateral-definition",
            "11111111111111111111111111111111",
            "--creator",
            "Public/11111111111111111111111111111111",
            "--factory-program-path",
            "factory.bin",
            "--factory-program-account",
            "11111111111111111111111111111111",
            "--curve-program-path",
            "curve.bin",
            "--curve-program-account",
            "11111111111111111111111111111111",
        ];
        let without_hook = Cli::try_parse_from(&arguments).unwrap();
        let Command::CreateSale(args) = without_hook.command else {
            panic!("create sale")
        };
        assert!(args.sale_request().graduation_hook.is_none());
        assert_eq!(args.sale_request().sell_fee_bps, 0);
        for rate in ["0", "125", "9999"] {
            let mut selected = arguments.clone();
            selected.extend(["--sell-fee-bps", rate]);
            let Command::CreateSale(args) = Cli::try_parse_from(selected).unwrap().command else {
                panic!("create sale")
            };
            assert_eq!(
                args.sale_request().sell_fee_bps,
                rate.parse::<u16>().unwrap()
            );
        }
        for rate in ["10000", "65535", "65536", "-1"] {
            let mut selected = arguments.clone();
            selected.extend(["--sell-fee-bps", rate]);
            assert!(Cli::try_parse_from(selected).is_err());
        }

        assert_eq!(
            args.sale_request().ending_policy,
            launchpad_client::EndingPolicy::Depletion
        );
        assert_eq!(
            args.sale_request().pause_policy,
            launchpad_client::PausePolicy::Disabled
        );
        for (options, expected) in [
            (vec![], launchpad_client::EndingPolicy::Depletion),
            (
                vec!["--end-timestamp", "100"],
                launchpad_client::EndingPolicy::Either,
            ),
            (
                vec!["--ending-policy", "depletion"],
                launchpad_client::EndingPolicy::Depletion,
            ),
            (
                vec!["--ending-policy", "scheduled", "--end-timestamp", "100"],
                launchpad_client::EndingPolicy::Scheduled,
            ),
            (
                vec!["--ending-policy", "either", "--end-timestamp", "100"],
                launchpad_client::EndingPolicy::Either,
            ),
        ] {
            let mut selected = arguments.clone();
            selected.extend(options);
            let cli = Cli::try_parse_from(selected).unwrap();
            cli.validate_route().unwrap();
            let Command::CreateSale(args) = cli.command else {
                panic!("create sale")
            };
            assert_eq!(args.sale_request().ending_policy, expected);
        }
        for ending in ["scheduled", "either", "manual", "unknown"] {
            let mut missing_end = arguments.clone();
            missing_end.extend(["--ending-policy", ending]);
            assert!(Cli::try_parse_from(missing_end).is_err());
        }
        let mut depletion_with_end = arguments.clone();
        depletion_with_end.extend(["--ending-policy", "depletion", "--end-timestamp", "100"]);
        assert!(
            Cli::try_parse_from(depletion_with_end)
                .unwrap()
                .validate_route()
                .is_err()
        );
        let mut pausable = arguments.clone();
        pausable.push("--pausable");
        let Command::CreateSale(args) = Cli::try_parse_from(pausable).unwrap().command else {
            panic!("create sale")
        };
        assert_eq!(
            args.sale_request().pause_policy,
            launchpad_client::PausePolicy::AuthorityControlled
        );
        for flag in ["--graduation-launch-bps", "--graduation-collateral-bps"] {
            for value in ["0", "10000"] {
                let mut selected = arguments.clone();
                selected.extend([flag, value]);
                let cli = Cli::try_parse_from(selected).unwrap();
                assert_eq!(cli.validate_route().is_ok(), value == "0");
            }
            for value in ["10001", "65536", "-1"] {
                let mut selected = arguments.clone();
                selected.extend([flag, value]);
                assert!(Cli::try_parse_from(selected).is_err());
            }
        }
        let mut missing_split = arguments.clone();
        missing_split.extend(["--graduation-hook-program-id", &program]);
        assert!(Cli::try_parse_from(&missing_split).is_err());
        for flag in ["--graduation-launch-bps", "--graduation-collateral-bps"] {
            let mut selected = missing_split.clone();
            selected.extend([flag, "0"]);
            assert!(Cli::try_parse_from(selected).is_err());
        }
        let mut configuration_without_program = arguments.clone();
        configuration_without_program.extend(["--graduation-hook-configuration", "070b"]);
        assert!(Cli::try_parse_from(configuration_without_program).is_err());
        for invalid_program in ["00", "zz", &"00".repeat(33)] {
            let mut invalid = arguments.clone();
            invalid.extend(["--graduation-hook-program-id", invalid_program]);
            assert!(Cli::try_parse_from(invalid).is_err());
        }
        let mut invalid_configuration = arguments.clone();
        invalid_configuration.extend([
            "--graduation-hook-program-id",
            &program,
            "--graduation-hook-configuration",
            "07z",
        ]);
        assert!(Cli::try_parse_from(invalid_configuration).is_err());
        let mut empty_configuration = arguments.clone();
        empty_configuration.extend([
            "--graduation-hook-program-id",
            &program,
            "--graduation-launch-bps",
            "0",
            "--graduation-collateral-bps",
            "10000",
        ]);
        let Command::CreateSale(args) = Cli::try_parse_from(empty_configuration).unwrap().command
        else {
            panic!("create sale")
        };
        assert_eq!(
            args.sale_request().graduation_hook,
            Some(launchpad_client::GraduationHook {
                program_id: lee::AccountId::new([1; 32]),
                configuration: vec![],
            })
        );
        arguments.extend([
            "--graduation-hook-program-id",
            &program,
            "--graduation-hook-configuration",
            "070b",
            "--graduation-launch-bps",
            "2500",
            "--graduation-collateral-bps",
            "7500",
        ]);
        let cli = Cli::try_parse_from(arguments).unwrap();
        let Command::CreateSale(args) = cli.command else {
            panic!("create sale")
        };
        assert_eq!(
            args.sale_request().graduation_hook,
            Some(launchpad_client::GraduationHook {
                program_id: lee::AccountId::new([1; 32]),
                configuration: vec![7, 11],
            })
        );
    }

    #[test]
    fn close_command_is_not_available() {
        assert!(Cli::try_parse_from(["launchpad", "close"]).is_err());
    }

    #[test]
    fn price_accepts_an_exact_output_buy_quote_with_live_program_context() {
        let cli = Cli::try_parse_from([
            "launchpad",
            "price",
            "--launch-salt",
            &"11".repeat(32),
            "--tokens",
            "25",
            "--collateral-definition",
            "11111111111111111111111111111111",
            "--factory-program-path",
            "factory.bin",
            "--factory-program-account",
            "11111111111111111111111111111111",
            "--curve-program-path",
            "curve.bin",
            "--curve-program-account",
            "11111111111111111111111111111111",
        ])
        .expect("price command should parse");
        assert!(matches!(cli.command, Command::Price(_)));
    }

    #[test]
    fn configure_accepts_the_curve_admin_and_treasury_context() {
        let cli = Cli::try_parse_from([
            "launchpad",
            "--json",
            "configure",
            "--admin",
            "Public/11111111111111111111111111111111",
            "--treasury",
            "Public/11111111111111111111111111111111",
            "--curve-program-path",
            "curve.bin",
            "--curve-program-account",
            "11111111111111111111111111111111",
        ])
        .expect("configure command should parse");
        assert!(matches!(cli.command, Command::Configure(_)));
    }

    #[test]
    fn quote_json_values_stay_stable() {
        for (kind, expected) in [(QuoteMode::Buy, "buy"), (QuoteMode::Sell, "sell")] {
            let quote = PriceQuote {
                namespace: "namespace".to_owned(),
                protocol_fee_bps: 5,
                treasury: "treasury".to_owned(),
                kind,
                amount_in: 10,
                raw_amount_out: 9,
                amount_out: 8,
                protocol_fee: 1,
            };
            assert_eq!(serde_json::to_value(quote).unwrap()["kind"], expected);
        }
    }

    #[test]
    fn json_error_categories_are_stable() {
        for (category, expected) in [
            (ErrorCategory::SlippageFloor, "slippage_floor"),
            (
                ErrorCategory::SaleReserveOvershoot,
                "sale_reserve_overshoot",
            ),
            (
                ErrorCategory::CollateralReserveOvershoot,
                "collateral_reserve_overshoot",
            ),
            (ErrorCategory::General, "general"),
        ] {
            let label: &'static str = category.into();
            assert_eq!(label, expected);
            assert_eq!(category.to_string(), expected);
            assert_eq!(error_category(&eyre::Report::new(category)), category);
        }
    }

    #[test]
    fn quote_error_variants_select_categories() {
        use launchpad_client::ClientError;
        use pool::SwapError;
        for (error, category) in [
            (
                ClientError::QuoteBuy(SwapError::SlippageExceeded),
                ErrorCategory::SlippageFloor,
            ),
            (
                ClientError::QuoteSell(SwapError::SlippageExceeded),
                ErrorCategory::SlippageFloor,
            ),
            (
                ClientError::QuoteBuyWithCollateral(SwapError::SlippageExceeded),
                ErrorCategory::SlippageFloor,
            ),
            (
                ClientError::QuoteBuy(SwapError::InsufficientRealOutputReserve),
                ErrorCategory::SaleReserveOvershoot,
            ),
            (
                ClientError::QuoteBuyWithCollateral(SwapError::InsufficientRealOutputReserve),
                ErrorCategory::SaleReserveOvershoot,
            ),
            (
                ClientError::QuoteSell(SwapError::InsufficientRealOutputReserve),
                ErrorCategory::CollateralReserveOvershoot,
            ),
            (
                ClientError::QuoteBuy(SwapError::Closed),
                ErrorCategory::General,
            ),
        ] {
            let report = eyre::Report::new(error).wrap_err("reading a quote");
            assert_eq!(error_category(&report), category);
        }
    }

    #[test]
    fn error_text_cannot_select_a_category() {
        for message in [
            "slippage",
            "max_amount_in",
            "sale reserve",
            "real_reserve0",
            "collateral reserve",
            "real_reserve1",
        ] {
            assert_eq!(
                error_category(&eyre::eyre!(message)),
                ErrorCategory::General
            );
            let error = launchpad_client::ClientError::ReadFactoryPool(Box::new(
                std::io::Error::other(message),
            ));
            assert_eq!(
                error_category(&eyre::Report::new(error)),
                ErrorCategory::General
            );
        }
        let error = launchpad_client::ClientError::SubmitPrivateBuy(Box::new(
            wallet::ExecutionFailureKind::InsufficientFundsError,
        ));
        assert_eq!(
            error_category(&eyre::Report::new(error)),
            ErrorCategory::General
        );
    }

    #[test]
    fn launch_salt_must_be_exactly_32_bytes() {
        let account = "11111111111111111111111111111111";
        let error = Cli::try_parse_from([
            "launchpad",
            "sale-info",
            "--engine",
            "bonding-curve",
            "--launch-salt",
            "00",
            "--collateral-definition",
            account,
            "--factory-program-account",
            account,
            "--engine-program-account",
            account,
        ])
        .expect_err("short launch salts must be rejected");
        assert!(error.to_string().contains("exactly 32 bytes"));
    }
}

#[cfg(test)]
mod distribution_tests {
    use super::*;

    fn creation_arguments(file: &Path) -> Vec<String> {
        let mut args: Vec<String> = [
            "launchpad",
            "create-sale",
            "--trade-mode",
            "buy-only",
            "--name",
            "Launch",
            "--uri",
            "https://example.invalid/token",
            "--sale-reserve",
            "800",
            "--creator-allocation",
            "100",
            "--virtual-token-reserve",
            "2000",
            "--virtual-collateral-reserve",
            "100",
            "--collateral-definition",
            "11111111111111111111111111111111",
            "--creator",
            "Public/11111111111111111111111111111111",
            "--factory-program-path",
            "missing-factory.bin",
            "--curve-program-path",
            "missing-curve.bin",
            "--curve-program-account",
            "11111111111111111111111111111111",
            "--graduation-launch-bps",
            "5000",
            "--graduation-collateral-bps",
            "5000",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        args.extend([
            "--launch-salt".into(),
            hex::encode([3; 32]),
            "--factory-program-account".into(),
            lee::AccountId::new([7; 32]).to_string(),
            "--graduation-hook-program-id".into(),
            lee::AccountId::new([9; 32]).to_string(),
            "--distribution-configuration".into(),
            file.to_str().unwrap().into(),
        ]);
        args
    }

    fn configuration(destination: lee::AccountId) -> launchpad_client::distribution::Configuration {
        use launchpad_client::distribution::{Configuration, Rule, VERSION};
        Configuration {
            version: VERSION,
            launch_rules: vec![Rule {
                destination,
                bps: 10000,
            }],
            collateral_rules: vec![Rule {
                destination,
                bps: 10000,
            }],
        }
    }

    #[tokio::test]
    async fn reserve_input_is_saved_and_supply_overflow_fails_before_io() {
        let mut arguments = creation_arguments(Path::new("missing-rules.json"));
        arguments.extend(["--graduation-reserve".into(), "200".into()]);
        let Command::CreateSale(mut args) = Cli::try_parse_from(arguments).unwrap().command else {
            panic!("create sale")
        };
        let request = args.sale_request();
        assert_eq!(request.graduation_reserve, 200);
        assert_eq!(request.total_supply().unwrap(), 1100);
        args.graduation_reserve = u128::MAX;
        let error = create_sale(lee::AccountId::new([40; 32]), args, None, open_wallet)
            .await
            .unwrap_err();
        assert!(matches!(
            error.downcast_ref::<launchpad_client::ClientError>(),
            Some(launchpad_client::ClientError::FactoryParameters(_))
        ));
    }

    #[tokio::test]
    async fn creation_rejects_custody_before_loading_programs_or_wallet() {
        use launchpad_client::distribution::{DistributionError, GraduationIntentKind, initialize};
        let namespace = lee::AccountId::new([40; 32]);
        let custody = initialize(
            lee::AccountId::new([9; 32]),
            lee::AccountId::new([7; 32]),
            namespace,
            [3; 32],
            GraduationIntentKind::Curve,
        )
        .destination
        .account_id;
        for side in 0..2 {
            let mut config = configuration(lee::AccountId::new([80; 32]));
            if side == 0 {
                config.launch_rules[0].destination = custody;
            } else {
                config.collateral_rules[0].destination = custody;
            }
            let file = std::env::temp_dir().join(format!(
                "launchpad-invalid-distribution-{}-{side}.json",
                std::process::id()
            ));
            std::fs::write(&file, serde_json::to_vec(&config).unwrap()).unwrap();
            let cli = Cli::try_parse_from(creation_arguments(&file)).unwrap();
            cli.validate_route().unwrap();
            let Command::CreateSale(args) = cli.command else {
                panic!("create sale")
            };
            let result = create_sale(namespace, args, None, open_wallet).await;
            std::fs::remove_file(file).unwrap();
            assert!(matches!(
                result
                    .unwrap_err()
                    .downcast_ref::<launchpad_client::ClientError>(),
                Some(launchpad_client::ClientError::Distribution(
                    DistributionError::InvalidConfiguration
                ))
            ));
        }
    }

    #[test]
    fn creation_saves_valid_rules_and_requires_one_configuration_source() {
        let config = configuration(lee::AccountId::new([80; 32]));
        let file = std::env::temp_dir().join(format!(
            "launchpad-valid-distribution-{}.json",
            std::process::id()
        ));
        std::fs::write(&file, serde_json::to_vec(&config).unwrap()).unwrap();
        let arguments = creation_arguments(&file);
        let cli = Cli::try_parse_from(&arguments).unwrap();
        let Command::CreateSale(args) = cli.command else {
            panic!("create sale")
        };
        let request = args.prepared_sale_request(args.sale(lee::AccountId::new([40; 32])));
        std::fs::remove_file(file).unwrap();
        let saved = request.unwrap().graduation_hook.unwrap();
        assert_eq!(saved.program_id, lee::AccountId::new([9; 32]));
        assert_eq!(
            launchpad_client::distribution::Configuration::decode(&saved.configuration).unwrap(),
            config
        );

        let mut conflicting = arguments.clone();
        conflicting.extend(["--graduation-hook-configuration".into(), "00".into()]);
        assert!(Cli::try_parse_from(conflicting).is_err());
        let mut missing_program = arguments;
        let index = missing_program
            .iter()
            .position(|arg| arg == "--graduation-hook-program-id")
            .unwrap();
        missing_program.drain(index..index + 2);
        assert!(Cli::try_parse_from(missing_program).is_err());
    }

    #[test]
    fn offline_distribution_commands_parse_without_wallet_or_namespace() {
        for args in [
            vec![
                "launchpad",
                "distribution",
                "encode",
                "--file",
                "rules.json",
            ],
            vec![
                "launchpad",
                "distribution",
                "preview",
                "--file",
                "rules.json",
                "--launch-balance",
                "101",
                "--collateral-balance",
                "0",
            ],
        ] {
            let cli = Cli::try_parse_from(args).unwrap();
            assert!(cli.namespace.is_none());
            cli.validate_route().unwrap();
        }
    }
}
