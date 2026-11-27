use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, builder::Styles};
use eyre::Result;
use launchpad_client::{
    AccountLocation, CreateSaleRequest, CreatorAllocationRelease, FixedPrice, FixedPriceInputs,
    LbpSale, PrivateAccountId, SaleStart, WeightedInputs,
};
use serde::Serialize;

use super::sale_info::{self, Engine};

#[cfg(test)]
mod tests;

#[derive(Debug, Parser)]
#[command(name = "launchpad", version, about = "Launchpad on the Logos Execution Zone", styles = Styles::styled())]
pub(super) struct Cli {
    /// Emit stable machine-readable output.
    #[arg(long, global = true)]
    pub(super) json: bool,
    /// Stable namespace AccountId used at initialization.
    #[arg(long, global = true)]
    #[arg(value_parser = parse_account_location)]
    pub(super) namespace: Option<AccountLocation>,
    /// Legacy private authority router path; unsupported on the actor development base.
    #[arg(long, global = true)]
    pub(super) authority_router_path: Option<PathBuf>,
    #[command(subcommand)]
    pub(super) command: Command,
}

impl Cli {
    pub(super) fn validate_route(&self) -> Result<()> {
        if let Command::CreateSale(args) = &self.command {
            args.validate_engine_flags()?;
            let graduation = args.graduation_policy();
            if args.graduation_hook_program_id.is_none()
                && (graduation.launch_bps != 0 || graduation.collateral_bps != 0)
            {
                eyre::bail!("nonzero graduation percentages require --graduation-hook-program-id");
            }
            let start = match args.engine {
                Engine::BondingCurve => None,
                Engine::Weighted => args.start_at,
                Engine::FixedPrice => Some(args.start_now.or(args.start_at).ok_or_else(|| {
                    eyre::eyre!("fixed-price sales require --start-now or --start-at")
                })?),
            };
            args.ending_policy().validate(start, args.end_timestamp)?;
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
pub(super) enum Command {
    /// Create and fund a factory sale.
    #[command(alias = "launch")]
    CreateSale(Box<CreateSaleArgs>),
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
pub(super) enum DistributionCommand {
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
pub(super) struct DistributionIdentity {
    #[arg(long)]
    pub(super) adapter_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) adapter_program_account: lee::AccountId,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) factory_program_account: lee::AccountId,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) fee_payer: lee::AccountId,
    #[command(flatten)]
    pub(super) launch: LaunchArgs,
}
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(super) enum DistributionKind {
    Curve,
    FixedPrice,
    Weighted,
}

#[derive(Debug, Args)]
pub(super) struct CreateAuthorityArgs {
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
pub(super) struct TransferAuthorityArgs {
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
pub(super) struct NamespaceReadArgs {
    #[arg(long)]
    pub(super) curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) curve_program_account: lee::AccountId,
}
#[derive(Debug, Args)]
pub(super) struct NamespaceAdminArgs {
    #[arg(long)]
    pub(super) curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) curve_program_account: lee::AccountId,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) admin: lee::AccountId,
}

#[derive(Debug, Args)]
pub(super) struct UpdateFeeArgs {
    #[command(flatten)]
    pub(super) authority: NamespaceAdminArgs,
    #[arg(long)]
    pub(super) protocol_fee_bps: u16,
}

#[derive(Debug, Args)]
pub(super) struct UpdateTreasuryArgs {
    #[command(flatten)]
    pub(super) authority: NamespaceAdminArgs,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) treasury: lee::AccountId,
}

#[derive(Debug, Args)]
pub(super) struct TransferAdminArgs {
    #[command(flatten)]
    pub(super) authority: NamespaceAdminArgs,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) new_admin: lee::AccountId,
}

#[derive(Debug, Args)]
pub(super) struct RenounceAdminArgs {
    #[command(flatten)]
    pub(super) authority: NamespaceAdminArgs,
    /// Confirm permanent removal of the namespace administrator.
    #[arg(long)]
    pub(super) confirm_renounce: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum TradeModeArg {
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

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum AllocationReleaseArg {
    AfterSetup,
    AfterEnd,
}

impl From<AllocationReleaseArg> for CreatorAllocationRelease {
    fn from(release: AllocationReleaseArg) -> Self {
        match release {
            AllocationReleaseArg::AfterSetup => Self::AfterSetup,
            AllocationReleaseArg::AfterEnd => Self::AfterEnd,
        }
    }
}

#[derive(Debug, Args)]
pub(super) struct CreateSaleArgs {
    #[arg(long, value_enum)]
    pub(super) engine: Engine,
    #[arg(long, value_enum)]
    trade_mode: TradeModeArg,
    /// Optional sale allowlist root (64 hexadecimal characters).
    #[arg(long, value_parser = parse_allowlist_root)]
    pub(super) allowlist_root: Option<[u8; 32]>,
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
    pub(super) graduation_hook_program_id: Option<lee::AccountId>,
    /// Saved opaque adapter configuration as hexadecimal bytes.
    #[arg(long, value_parser = parse_hook_configuration, requires = "graduation_hook_program_id")]
    graduation_hook_configuration: Option<HookConfiguration>,
    /// Distribution rules as JSON. Check recipients for this sale before creation.
    #[arg(
        long,
        requires = "graduation_hook_program_id",
        conflicts_with = "graduation_hook_configuration"
    )]
    pub(super) distribution_configuration: Option<PathBuf>,
    #[command(flatten)]
    pub(super) launch: LaunchArgs,
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
    /// Initial virtual launch tokens (bonding-curve only).
    #[arg(
        long = "virtual-token-reserve",
        alias = "vt",
        required_if_eq("engine", "bonding-curve")
    )]
    virtual_token_reserve: Option<u128>,
    /// Initial virtual collateral (bonding-curve only).
    #[arg(
        long = "virtual-collateral-reserve",
        alias = "vc",
        required_if_eq("engine", "bonding-curve")
    )]
    virtual_collateral_reserve: Option<u128>,
    /// Collateral amount in the fixed-price ratio, in smallest units.
    #[arg(long, required_if_eq("engine", "fixed-price"), value_parser = parse_positive_amount)]
    price_collateral_units: Option<u128>,
    /// Launch token amount in the fixed-price ratio, in smallest units.
    #[arg(long, required_if_eq("engine", "fixed-price"), value_parser = parse_positive_amount)]
    price_token_units: Option<u128>,
    /// Fixed-price start in Unix seconds. Inclusion must be at or after this time.
    #[arg(long, conflicts_with = "start_at")]
    start_now: Option<u64>,
    /// Scheduled start in Unix seconds. Inclusion must be at or before this time.
    #[arg(long, required_if_eq("engine", "weighted"))]
    start_at: Option<u64>,
    /// Creator-funded collateral seed in smallest units (weighted only).
    #[arg(long, required_if_eq("engine", "weighted"), value_parser = parse_positive_amount)]
    collateral_seed: Option<u128>,
    /// Start weight of the launch token on a 10^18 scale (weighted only).
    #[arg(long, required_if_eq("engine", "weighted"), value_parser = parse_positive_amount)]
    token_weight_start: Option<u128>,
    /// End weight of the launch token on a 10^18 scale (weighted only).
    #[arg(long, required_if_eq("engine", "weighted"), value_parser = parse_positive_amount)]
    token_weight_end: Option<u128>,
    /// Creator allocation release for LBP sales. Defaults to after-end.
    #[arg(long, value_enum)]
    allocation_release: Option<AllocationReleaseArg>,
    /// Ending rule; defaults to depletion without an end, or either with an end.
    #[arg(long, value_enum)]
    ending_policy: Option<EndingPolicyArg>,
    /// Sale end time in Unix seconds; required for scheduled or either ending.
    #[arg(long = "end-timestamp", required_if_eq_any = [("engine", "weighted"), ("ending_policy", "scheduled"), ("ending_policy", "either")])]
    end_timestamp: Option<u64>,
    #[arg(long = "collateral-definition", value_parser = parse_account_location)]
    pub(super) collateral_definition: AccountLocation,
    /// Account that authorizes the factory launch.
    #[arg(long, value_parser = parse_account_location)]
    pub(super) creator: AccountLocation,
    /// Clock lower bound in Unix seconds (bonding-curve only; defaults to zero).
    #[arg(long)]
    now: Option<u64>,
    /// Compiled factory guest binary deployed for this walkthrough.
    #[arg(long = "factory-program-path")]
    pub(super) factory_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) factory_program_account: lee::AccountId,
    /// Compiled curve guest binary paired with the factory launch.
    #[arg(long = "curve-program-path", required_if_eq("engine", "bonding-curve"))]
    pub(super) curve_program_path: Option<PathBuf>,
    /// Deployed curve or LBP program account, for the selected engine.
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) engine_program_account: lee::AccountId,
    /// Deployed namespace program account for LBP sales.
    #[arg(long, value_parser = parse_public_account_id, required_if_eq_any([("engine", "fixed-price"), ("engine", "weighted")]))]
    pub(super) namespace_program_account: Option<lee::AccountId>,
}

impl CreateSaleArgs {
    fn validate_engine_flags(&self) -> Result<()> {
        let curve = matches!(self.engine, Engine::BondingCurve);
        let fixed = matches!(self.engine, Engine::FixedPrice);
        let weighted = matches!(self.engine, Engine::Weighted);
        for (flag, present, allowed) in [
            (
                "--curve-program-path",
                self.curve_program_path.is_some(),
                curve,
            ),
            (
                "--virtual-token-reserve",
                self.virtual_token_reserve.is_some(),
                curve,
            ),
            (
                "--virtual-collateral-reserve",
                self.virtual_collateral_reserve.is_some(),
                curve,
            ),
            ("--now", self.now.is_some(), curve),
            (
                "--namespace-program-account",
                self.namespace_program_account.is_some(),
                !curve,
            ),
            (
                "--allocation-release",
                self.allocation_release.is_some(),
                !curve,
            ),
            ("--start-at", self.start_at.is_some(), !curve),
            ("--start-now", self.start_now.is_some(), fixed),
            (
                "--price-collateral-units",
                self.price_collateral_units.is_some(),
                fixed,
            ),
            (
                "--price-token-units",
                self.price_token_units.is_some(),
                fixed,
            ),
            (
                "--collateral-seed",
                self.collateral_seed.is_some(),
                weighted,
            ),
            (
                "--token-weight-start",
                self.token_weight_start.is_some(),
                weighted,
            ),
            (
                "--token-weight-end",
                self.token_weight_end.is_some(),
                weighted,
            ),
        ] {
            if present && !allowed {
                eyre::bail!("{flag} does not apply to the selected engine");
            }
        }
        Ok(())
    }

    pub(super) fn fixed_price_request(
        &self,
        sale: LbpSale,
        creator: lee::AccountId,
    ) -> Result<FixedPriceInputs> {
        let start = match (self.start_now, self.start_at) {
            (Some(timestamp), None) => SaleStart::Now { timestamp },
            (None, Some(timestamp)) => SaleStart::At { timestamp },
            _ => eyre::bail!("fixed-price sales require --start-now or --start-at"),
        };
        Ok(FixedPriceInputs {
            buy_policy: launchpad_client::lbp_buy_policy(sale, self.allowlist_root),
            pause_policy: self.pause_policy(),
            ending_policy: self.ending_policy(),
            trade_mode: self.trade_mode.into(),
            sell_fee_bps: self.sell_fee_bps,
            per_block_token_limit: self.per_block_token_limit,
            graduation_hook: self.prepared_graduation_hook(sale.namespace)?,
            graduation_policy: self.graduation_policy(),
            namespace: sale.namespace,
            launch_salt: sale.launch_salt,
            name: self.name.clone(),
            uri: self.uri.clone(),
            sale_inventory: self.sale_reserve,
            graduation_reserve: self.graduation_reserve,
            creator_allocation: self.creator_allocation,
            price: FixedPrice {
                collateral_units: self
                    .price_collateral_units
                    .expect("required for fixed-price"),
                token_units: self.price_token_units.expect("required for fixed-price"),
            },
            start,
            end_timestamp: self.end_timestamp,
            allocation_release: self.allocation_release(),
            namespace_program_id: sale.namespace_program_id,
            lbp_program_id: sale.lbp_program_id,
            collateral_definition_id: sale.collateral_definition,
            creator,
            now: 0,
        })
    }

    pub(super) fn weighted_request(
        &self,
        sale: LbpSale,
        creator: lee::AccountId,
    ) -> Result<WeightedInputs> {
        Ok(WeightedInputs {
            buy_policy: launchpad_client::lbp_buy_policy(sale, self.allowlist_root),
            pause_policy: self.pause_policy(),
            ending_policy: self.ending_policy(),
            trade_mode: self.trade_mode.into(),
            sell_fee_bps: self.sell_fee_bps,
            per_block_token_limit: self.per_block_token_limit,
            graduation_hook: self.prepared_graduation_hook(sale.namespace)?,
            graduation_policy: self.graduation_policy(),
            namespace: sale.namespace,
            launch_salt: sale.launch_salt,
            name: self.name.clone(),
            uri: self.uri.clone(),
            sale_inventory: self.sale_reserve,
            graduation_reserve: self.graduation_reserve,
            creator_allocation: self.creator_allocation,
            collateral_seed: self.collateral_seed.expect("required for weighted"),
            token_weight_start: self.token_weight_start.expect("required for weighted"),
            token_weight_end: self.token_weight_end.expect("required for weighted"),
            start_timestamp: self.start_at.expect("required for weighted"),
            end_timestamp: self.end_timestamp.expect("required for weighted"),
            allocation_release: self.allocation_release(),
            namespace_program_id: sale.namespace_program_id,
            lbp_program_id: sale.lbp_program_id,
            collateral_definition_id: sale.collateral_definition,
            creator,
        })
    }

    fn allocation_release(&self) -> CreatorAllocationRelease {
        self.allocation_release
            .unwrap_or(AllocationReleaseArg::AfterEnd)
            .into()
    }

    fn pause_policy(&self) -> launchpad_client::PausePolicy {
        if self.pausable {
            launchpad_client::PausePolicy::AuthorityControlled
        } else {
            launchpad_client::PausePolicy::Disabled
        }
    }

    fn ending_policy(&self) -> launchpad_client::EndingPolicy {
        self.ending_policy.map(Into::into).unwrap_or_else(|| {
            if self.end_timestamp.is_some() {
                launchpad_client::EndingPolicy::Either
            } else {
                launchpad_client::EndingPolicy::Depletion
            }
        })
    }

    fn graduation_policy(&self) -> launchpad_client::GraduationPolicy {
        launchpad_client::GraduationPolicy {
            launch_bps: self.graduation_launch_bps.unwrap_or(0),
            collateral_bps: self.graduation_collateral_bps.unwrap_or(0),
        }
    }

    pub(super) fn raw_graduation_hook(&self) -> Option<launchpad_client::GraduationHook> {
        self.graduation_hook_program_id
            .map(|program_id| launchpad_client::GraduationHook {
                program_id,
                configuration: self
                    .graduation_hook_configuration
                    .as_ref()
                    .map_or_else(Vec::new, |configuration| configuration.0.clone()),
            })
    }

    pub(super) fn sale_request(&self) -> CreateSaleRequest {
        CreateSaleRequest {
            buy_policy: launchpad_client::BuyPolicy::Open,
            pause_policy: self.pause_policy(),
            ending_policy: self.ending_policy(),
            trade_mode: self.trade_mode.into(),
            sell_fee_bps: self.sell_fee_bps,
            per_block_token_limit: self.per_block_token_limit,
            graduation_hook: self.raw_graduation_hook(),
            name: self.name.clone(),
            uri: self.uri.clone(),
            sale_reserve: self.sale_reserve,
            graduation_reserve: self.graduation_reserve,
            graduation_policy: self.graduation_policy(),
            creator_allocation: self.creator_allocation,
            virtual_token_reserve: self
                .virtual_token_reserve
                .expect("required for bonding-curve"),
            virtual_collateral_reserve: self
                .virtual_collateral_reserve
                .expect("required for bonding-curve"),
            end_timestamp: self.end_timestamp,
            now: self.now.unwrap_or(0),
        }
    }
}

#[derive(Debug, Clone, Args)]
pub(super) struct LaunchArgs {
    /// Explicit 32-byte hexadecimal sale salt within the selected namespace.
    #[arg(long, value_parser = parse_launch_salt)]
    pub(super) launch_salt: [u8; 32],
}

#[derive(Debug, Args)]
pub(super) struct FactoryTradeArgs {
    #[command(flatten)]
    pub(super) launch: LaunchArgs,
    #[arg(long = "collateral-definition", value_parser = parse_account_location)]
    pub(super) collateral_definition: AccountLocation,
    /// Public account that authorizes the trade.
    #[arg(long, value_parser = parse_account_location)]
    pub(super) participant: AccountLocation,
    #[arg(long = "factory-program-path")]
    pub(super) factory_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) factory_program_account: lee::AccountId,
    #[arg(long = "curve-program-path")]
    pub(super) curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) curve_program_account: lee::AccountId,
}

#[derive(Debug, Args)]
pub(super) struct BuyArgs {
    #[command(flatten)]
    pub(super) trade: FactoryTradeArgs,
    #[arg(long)]
    pub(super) tokens: u128,
    #[arg(long = "max-collateral")]
    pub(super) max_collateral: u128,
}

#[derive(Debug, Args)]
pub(super) struct BuyWithCollateralArgs {
    #[command(flatten)]
    pub(super) trade: FactoryTradeArgs,
    #[arg(long)]
    pub(super) collateral: u128,
    #[arg(long = "min-tokens")]
    pub(super) min_tokens: u128,
}

#[derive(Debug, Args)]
pub(super) struct PrivateBuyArgs {
    #[command(flatten)]
    launch: LaunchArgs,
    #[arg(long = "collateral-definition", value_parser = parse_account_location)]
    collateral_definition: AccountLocation,
    #[arg(long)]
    pub(super) tokens: u128,
    #[arg(long = "max-collateral")]
    pub(super) max_collateral: u128,
    /// Owned private account providing collateral and native gas.
    #[arg(long = "from-private", value_parser = parse_private_account_id)]
    pub(super) from_private: PrivateAccountId,
    /// Owned private account that receives the purchased launch tokens.
    #[arg(long = "to-private", value_parser = parse_private_account_id)]
    pub(super) to_private: PrivateAccountId,
    /// Native units required for the transient public account's transaction fees.
    #[arg(long = "gas-reserve")]
    pub(super) gas_reserve: u128,
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
pub(super) struct FactoryLifecycleArgs {
    #[command(flatten)]
    pub(super) launch: LaunchArgs,
    #[arg(long = "collateral-definition", value_parser = parse_account_location)]
    pub(super) collateral_definition: AccountLocation,
    #[arg(long, value_parser = parse_account_location)]
    pub(super) creator: AccountLocation,
    #[arg(long = "factory-program-path")]
    pub(super) factory_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) factory_program_account: lee::AccountId,
    #[arg(long = "curve-program-path")]
    pub(super) curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) curve_program_account: lee::AccountId,
}

#[derive(Debug, Args)]
pub(super) struct GraduationArgs {
    /// Admit saved distribution destinations and validate its typed configuration.
    #[arg(long)]
    pub(super) distribution: bool,
    /// Current creator role holder. The factory verifies this hint through role authority.
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) creator_hint: lee::AccountId,
    #[command(flatten)]
    pub(super) launch: LaunchArgs,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) collateral_definition: lee::AccountId,
    #[arg(long)]
    pub(super) factory_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) factory_program_account: lee::AccountId,
    #[arg(long)]
    pub(super) curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) curve_program_account: lee::AccountId,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) fee_payer: lee::AccountId,
}

#[derive(Debug, Args)]
pub(super) struct CollectFeesArgs {
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) fee_payer: lee::AccountId,
    #[arg(long, required = true, num_args = 1.., value_parser = parse_account_location)]
    pub(super) pool: Vec<AccountLocation>,
    #[arg(long, value_parser = parse_account_location)]
    pub(super) collateral_definition: AccountLocation,
    #[arg(long)]
    pub(super) curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) curve_program_account: lee::AccountId,
}

#[derive(Debug, Args)]
pub(super) struct LaunchReadArgs {
    #[command(flatten)]
    pub(super) launch: LaunchArgs,
    #[arg(long = "collateral-definition", value_parser = parse_account_location)]
    pub(super) collateral_definition: AccountLocation,
    #[arg(long = "factory-program-path")]
    pub(super) factory_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) factory_program_account: lee::AccountId,
    #[arg(long = "curve-program-path")]
    pub(super) curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) curve_program_account: lee::AccountId,
}

#[derive(Debug, Args)]
pub(super) struct SellArgs {
    #[command(flatten)]
    pub(super) trade: FactoryTradeArgs,
    #[arg(long)]
    pub(super) tokens: u128,
    #[arg(long = "min-collateral")]
    pub(super) min_collateral: u128,
}

#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("price-input").required(true).args(["tokens", "collateral"]))) ]
pub(super) struct PriceArgs {
    /// Quote selling launch tokens for net collateral; requires --tokens.
    #[arg(long, requires = "tokens", conflicts_with = "collateral")]
    pub(super) sell: bool,
    #[command(flatten)]
    pub(super) launch: LaunchReadArgs,
    #[arg(long)]
    pub(super) tokens: Option<u128>,
    /// Collateral to spend for an exact-input purchase quote.
    #[arg(long)]
    pub(super) collateral: Option<u128>,
}

#[derive(Debug, Args)]
pub(super) struct ConfigArgs {
    /// Signing namespace AccountId; must equal --namespace.
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) admin: lee::AccountId,
    #[arg(long, default_value_t = 0)]
    pub(super) protocol_fee_bps: u16,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) treasury: lee::AccountId,
    #[arg(long = "curve-program-path")]
    pub(super) curve_program_path: PathBuf,
    #[arg(long, value_parser = parse_public_account_id)]
    pub(super) curve_program_account: lee::AccountId,
}

fn parse_account_location(raw: &str) -> Result<AccountLocation, String> {
    raw.parse()
        .map_err(|error: launchpad_client::ClientError| error.to_string())
}

pub(super) fn parse_public_account_id(raw: &str) -> Result<lee::AccountId, String> {
    match parse_account_location(raw)? {
        AccountLocation::Public(id) => Ok(id),
        AccountLocation::Private(_) => Err("this command requires public accounts".to_owned()),
    }
}

fn parse_private_account_id(raw: &str) -> Result<PrivateAccountId, String> {
    raw.parse()
        .map_err(|error: launchpad_client::ClientError| error.to_string())
}

#[derive(Debug, Clone)]
struct HookConfiguration(Vec<u8>);

fn parse_hook_configuration(raw: &str) -> Result<HookConfiguration, String> {
    hex::decode(raw)
        .map(HookConfiguration)
        .map_err(|_| "hook configuration must be hexadecimal bytes".to_owned())
}

fn parse_positive_amount(raw: &str) -> Result<u128, String> {
    raw.parse::<std::num::NonZeroU128>()
        .map(std::num::NonZeroU128::get)
        .map_err(|_| "expected a positive integer".into())
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
