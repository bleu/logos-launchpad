use factory_core::{LbpCreationStage, LbpLaunchState, LbpSetupMode};
use lbp_core::{CreatorAllocationRelease, FixedPrice, SettlementProgress};
use wallet::WalletSequencerClient;

use super::{
    AccountId, Actor, BuyPolicy, ClientError, CreationStage, EndingPolicy, FactorySale,
    FactoryState, GraduationHook, GraduationPolicy, LbpSale, PausePolicy, Result, SettlementStage,
    TradeMode, compute_factory_pda, validate_factory_pool, validate_factory_state,
    validate_lbp_factory_state,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaleSelection {
    BondingCurve(FactorySale),
    FixedPrice(LbpSale),
    Weighted(LbpSale),
}

#[derive(Debug)]
pub enum SaleInfo {
    BondingCurve(CurveSaleInfo),
    FixedPrice(FixedPriceSaleInfo),
    Weighted(WeightedSaleInfo),
}

#[derive(Debug)]
pub struct SavedSaleTerms {
    pub namespace: AccountId,
    pub factory: AccountId,
    pub launch_salt: [u8; 32],
    pub pool: AccountId,
    pub launch_token: AccountId,
    pub name: String,
    pub uri: String,
    pub collateral: AccountId,
    pub creator_role: AccountId,
    pub sale_inventory: u128,
    pub graduation_reserve: u128,
    pub creator_allocation: u128,
    pub total_supply: u128,
    pub buy_policy: BuyPolicy,
    pub pause_policy: PausePolicy,
    pub ending_policy: EndingPolicy,
    pub trade_mode: TradeMode,
    pub sell_fee_bps: u16,
    pub per_block_token_limit: Option<u128>,
    pub graduation_hook: Option<GraduationHook>,
    pub graduation_policy: GraduationPolicy,
    pub end_timestamp: Option<u64>,
}

#[derive(Debug)]
pub struct CurveSaleInfo {
    pub terms: SavedSaleTerms,
    pub setup_stage: CreationStage,
    pub settlement_stage: SettlementStage,
    pub creator_allocation_claimed: bool,
    pub initial_virtual_token_reserve: u128,
    pub initial_virtual_collateral_reserve: u128,
    pub pool: Option<Box<CurvePoolObservation>>,
}

#[derive(Debug)]
pub struct CurvePoolObservation {
    pub state: pool::Pool,
    pub funded: bool,
    /// Net tokens sold, when the pool still holds its reserves.
    pub tokens_sold: Option<u128>,
}

#[derive(Debug)]
pub struct LbpSaleInfo {
    pub terms: SavedSaleTerms,
    pub setup_stage: LbpCreationStage,
    pub settlement_progress: SettlementProgress,
    pub start_timestamp: u64,
    pub allocation_release: CreatorAllocationRelease,
    pub creator_allocation_released: bool,
}

#[derive(Debug)]
pub struct FixedPriceSaleInfo {
    pub sale: LbpSaleInfo,
    pub price: FixedPrice,
}

#[derive(Debug)]
pub struct WeightedSaleInfo {
    pub sale: LbpSaleInfo,
    pub collateral_seed: u128,
    pub token_weight_start: u128,
    pub token_weight_end: u128,
}

/// Read saved sale terms and setup progress. This does not test whether trading is open.
pub async fn load_sale_info(
    transport: &impl WalletSequencerClient,
    selection: SaleSelection,
) -> Result<SaleInfo> {
    let (namespace, deployment, salt) = match selection {
        SaleSelection::BondingCurve(sale) => {
            (sale.namespace, sale.factory_program_id, sale.launch_salt)
        }
        SaleSelection::FixedPrice(sale) | SaleSelection::Weighted(sale) => {
            (sale.namespace, sale.factory_program_id, sale.launch_salt)
        }
    };
    let factory = compute_factory_pda(namespace, deployment, salt);
    let actor = Actor::new(factory, deployment);
    let account = transport
        .get_account_view(actor)
        .await
        .map_err(|error| ClientError::ReadFactoryState(Box::new(error)))?
        .ok_or(ClientError::FactoryLaunchMissing)?;
    let bytes = account.data.actor_state(deployment);
    if bytes.is_empty() {
        return Err(ClientError::FactoryLaunchMissing);
    }
    match selection {
        SaleSelection::BondingCurve(sale) => {
            curve_sale_info(transport, sale, factory, bytes.as_ref()).await
        }
        SaleSelection::FixedPrice(sale) | SaleSelection::Weighted(sale) => {
            let state = LbpLaunchState::decode(bytes.as_ref()).map_err(|error| {
                if FactoryState::decode(bytes.as_ref()).is_ok() {
                    ClientError::SaleEngineMismatch
                } else {
                    ClientError::DecodeFactoryState(error)
                }
            })?;
            validate_lbp_factory_state(sale, &state)?;
            match (selection, state.mode) {
                (SaleSelection::FixedPrice(_), LbpSetupMode::FixedPrice) => {
                    Ok(SaleInfo::FixedPrice(FixedPriceSaleInfo {
                        price: state.price,
                        sale: lbp_sale_info(factory, state),
                    }))
                }
                (
                    SaleSelection::Weighted(_),
                    LbpSetupMode::Weighted {
                        collateral_seed,
                        token_weight_start,
                        token_weight_end,
                    },
                ) => Ok(SaleInfo::Weighted(WeightedSaleInfo {
                    sale: lbp_sale_info(factory, state),
                    collateral_seed,
                    token_weight_start,
                    token_weight_end,
                })),
                _ => Err(ClientError::SaleEngineMismatch),
            }
        }
    }
}

async fn curve_sale_info(
    transport: &impl WalletSequencerClient,
    sale: FactorySale,
    factory: AccountId,
    bytes: &[u8],
) -> Result<SaleInfo> {
    let state = FactoryState::decode(bytes).map_err(|error| {
        if LbpLaunchState::decode(bytes).is_ok() {
            ClientError::SaleEngineMismatch
        } else {
            ClientError::DecodeFactoryState(error)
        }
    })?;
    validate_factory_state(sale, &state)?;
    let pool = if state.creation_stage == CreationStage::Active {
        let account = transport
            .get_account_view(Actor::new(state.pool_id, sale.curve_program_id))
            .await
            .map_err(|error| ClientError::ReadFactoryPool(Box::new(error)))?
            .ok_or_else(|| {
                ClientError::ReadFactoryPool(Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "actor state is missing",
                )))
            })?;
        let saved = curve_core::PoolAccount::decode(
            account.data.actor_state(sale.curve_program_id).as_ref(),
        )
        .map_err(ClientError::DecodeFactoryPool)?;
        validate_factory_pool(sale, &state, &saved)?;
        let tokens_sold = if saved.pool.lifecycle == pool::PoolLifecycle::Withdrawn {
            None
        } else {
            Some(
                state
                    .sale_reserve
                    .checked_sub(saved.pool.real_reserve0)
                    .ok_or(ClientError::FactorySaleMismatch)?,
            )
        };
        Some(Box::new(CurvePoolObservation {
            state: saved.pool,
            funded: saved.funded,
            tokens_sold,
        }))
    } else {
        None
    };
    Ok(SaleInfo::BondingCurve(CurveSaleInfo {
        terms: SavedSaleTerms {
            namespace: state.namespace,
            factory,
            launch_salt: state.launch_salt,
            pool: state.pool_id,
            launch_token: state.token_definition_id,
            name: state.name,
            uri: state.uri,
            collateral: state.collateral_definition_id,
            creator_role: state.creator_role_id,
            sale_inventory: state.sale_reserve,
            graduation_reserve: state.graduation_reserve,
            creator_allocation: state.creator_allocation,
            total_supply: state.total_supply,
            buy_policy: state.buy_policy,
            pause_policy: state.pause_policy,
            ending_policy: state.ending_policy,
            trade_mode: state.trade_mode,
            sell_fee_bps: state.sell_fee_bps,
            per_block_token_limit: state.per_block_token_limit,
            graduation_hook: state.graduation_hook,
            graduation_policy: state.graduation_policy,
            end_timestamp: state.end_timestamp,
        },
        setup_stage: state.creation_stage,
        settlement_stage: state.settlement_stage,
        creator_allocation_claimed: state.creator_allocation_claimed,
        initial_virtual_token_reserve: state.virtual_token_reserve,
        initial_virtual_collateral_reserve: state.virtual_collateral_reserve,
        pool,
    }))
}

fn lbp_sale_info(factory: AccountId, state: LbpLaunchState) -> LbpSaleInfo {
    LbpSaleInfo {
        terms: SavedSaleTerms {
            namespace: state.namespace,
            factory,
            launch_salt: state.launch_salt,
            pool: state.pool_id,
            launch_token: state.token_definition_id,
            name: state.name,
            uri: state.uri,
            collateral: state.collateral_definition_id,
            creator_role: state.creator_role_id,
            sale_inventory: state.sale_inventory,
            graduation_reserve: state.graduation_reserve,
            creator_allocation: state.creator_allocation,
            total_supply: state.total_supply,
            buy_policy: state.buy_policy,
            pause_policy: state.pause_policy,
            ending_policy: state.ending_policy,
            trade_mode: state.trade_mode,
            sell_fee_bps: state.sell_fee_bps,
            per_block_token_limit: state.per_block_token_limit,
            graduation_hook: state.graduation_hook,
            graduation_policy: state.graduation_policy,
            end_timestamp: state.end_timestamp,
        },
        setup_stage: state.creation_stage,
        settlement_progress: state.settlement_progress,
        start_timestamp: state.start_timestamp,
        allocation_release: state.allocation_release,
        creator_allocation_released: state.creator_allocation_released,
    }
}
