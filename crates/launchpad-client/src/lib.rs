//! Transaction builders, quotes and launch workflows for LEZ actors.
//! Public timestamps use Unix seconds; native clock conversion belongs to program handlers.
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod curve;
mod error;
mod graduation;
mod invocation;
mod lbp;
mod lifecycle;
mod namespace;
mod namespace_builders;
mod quotes;
mod sale_info;
mod types;
mod wallet_ops;
mod write_transport;

#[cfg(test)]
mod tests;

pub mod distribution;

pub use curve::{
    CreateSaleRequest, build_buy_invocation, build_buy_with_collateral_invocation,
    build_claim_creator_allocation_invocation, build_collect_fees_invocation,
    build_create_sale_invocation, build_sell_invocation, build_set_curve_paused_invocation,
    build_set_factory_paused_invocation, build_withdraw_factory_proceeds_invocation,
    collect_pool_fees, curve_buy_policy, load_curve_config, load_factory_pool, load_factory_state,
    prepare_buy_invocation, prepare_buy_with_collateral_invocation, prepare_fee_collection,
    prepare_sell_invocation,
};
pub use curve_core::{
    AllowlistPolicy, BuyPolicy, EndingPolicy, MembershipPath, PausePolicy,
    TOKEN_PROGRAM_ACCOUNT_ID, TradeControl,
};
pub use error::{ClientError, Result};
pub use factory_core::{
    CreationStage, CreatorAllocationRelease, FixedPrice, FixedPriceInputs, GraduationHook,
    GraduationPolicy, LbpCreationStage, SaleStart, SettlementPayouts, SettlementStage, TradeMode,
    WeightedInputs,
};
pub use lbp_core::{
    QuoteRequest as LbpQuoteRequest, SettlementProgress as LbpSettlementProgress,
    TradeQuote as LbpTradeQuote,
};

pub use graduation::{GraduationTarget, build_graduation_invocation};
pub use invocation::PublicInvocation;
pub use lbp::{
    build_create_fixed_price_invocation, build_create_weighted_invocation,
    build_fixed_price_buy_invocation, build_fixed_price_buy_with_collateral_invocation,
    build_fixed_price_sell_invocation, build_set_lbp_paused_invocation,
    build_weighted_buy_invocation, build_weighted_buy_with_collateral_invocation,
    build_weighted_sell_invocation, lbp_buy_policy,
};
pub use lifecycle::{
    FactorySession, LbpCreateRequest, LifecycleReceipt, create_lbp_sale, next_creation_invocation,
};
pub use namespace::{NamespaceAction, NamespaceReceipt, submit_namespace_action};
pub use namespace_builders::{
    build_initialize_namespace_invocation, build_renounce_admin_invocation,
    build_transfer_admin_invocation, build_update_fee_invocation, build_update_treasury_invocation,
};
pub use quotes::{quote_buy, quote_buy_with_collateral, quote_lbp_trade, quote_sell};
pub use sale_info::{
    CurvePoolObservation, CurveSaleInfo, FixedPriceSaleInfo, LbpPoolObservation, LbpSaleInfo,
    SaleInfo, SaleSelection, SavedSaleTerms, WeightedSaleInfo, load_sale_info,
};
pub use types::{
    AccountLocation, ExactInputTradeTerms, ExactOutputTradeTerms, FactorySale, LbpSale,
    PoolFeeTarget, PrivateAccountId,
};

pub use wallet_ops::{
    LaunchpadWallet, PrivateBuyReceipt, PrivateBuyRequest, PrivateBuyRoute, PrivateInstruction,
    build_create_authority_invocation, build_private_authority_instruction,
    build_transfer_authority_invocation, load_authority_identity, load_clock_block_id,
    submit_private_authority, submit_private_buy, submit_public_invocation, transfer_authority,
    validate_private_buy_request,
};
pub use write_transport::{PublicWriteWallet, WriteTransport};

use lee::{AccountId, program::Program};
use lee_core::program::ProgramId;
use std::path::Path;

fn sale_buy_policy(
    namespace: AccountId,
    allowlist_root: Option<[u8; 32]>,
    pool: impl FnOnce() -> AccountId,
) -> BuyPolicy {
    match allowlist_root {
        Some(root) => BuyPolicy::Allowlist(AllowlistPolicy {
            version: 1,
            namespace: *namespace.value(),
            sale_policy_id: *pool().value(),
            root,
        }),
        None => BuyPolicy::Open,
    }
}

/// A deployed program address and its expected binary image. PDAs use `account_id`.
#[derive(Debug, Clone)]
pub struct ProgramHandle {
    pub account_id: AccountId,
    pub image: Program,
}
impl ProgramHandle {
    pub fn image_id(&self) -> ProgramId {
        self.image.id()
    }
}

pub fn parse_account_id(raw: &str) -> Result<AccountId> {
    raw.parse::<AccountLocation>()
        .map(AccountLocation::account_id)
}

pub fn load_program(path: &Path) -> Result<Program> {
    let bytecode = std::fs::read(path).map_err(|source| ClientError::ReadProgram {
        path: path.to_owned(),
        source,
    })?;
    Program::new(bytecode.into()).map_err(|source| ClientError::InvalidProgram {
        path: path.to_owned(),
        source: Box::new(source),
    })
}
