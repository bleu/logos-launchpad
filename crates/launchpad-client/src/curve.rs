use common::HashType;
use curve_core::{Config, Message as CurveInstruction, compute_config_pda, compute_pool_pda};
use factory_core::{
    FactoryState, Message as FactoryInstruction, compute_definition_pda, compute_escrow_pda,
    compute_factory_pda,
};
use lee::AccountId;
use lee_core::account::Actor;
use wallet::{WalletCore, WalletSequencerClient};

use crate::invocation::{
    FactoryPoolAddresses, associated_token_account, clock_actor, invocation, token_actor,
};
use crate::wallet_ops::read_actor;
use crate::{
    BuyPolicy, ClientError, EndingPolicy, ExactInputTradeTerms, ExactOutputTradeTerms, FactorySale,
    GraduationHook, GraduationPolicy, MembershipPath, PausePolicy, PoolFeeTarget, ProgramHandle,
    PublicInvocation, PublicWriteWallet, Result, TOKEN_PROGRAM_ACCOUNT_ID, TradeMode, quote_buy,
    quote_buy_with_collateral, quote_sell, sale_buy_policy, submit_public_invocation,
};

/// All creation terms are immutable. `now` is a clock lower bound, not a saved sale term.
#[derive(Debug, Clone)]
pub struct CreateSaleRequest {
    pub buy_policy: BuyPolicy,
    pub pause_policy: PausePolicy,
    pub ending_policy: EndingPolicy,
    pub trade_mode: TradeMode,
    pub sell_fee_bps: u16,
    pub per_block_token_limit: Option<u128>,
    pub graduation_hook: Option<GraduationHook>,
    pub name: String,
    pub uri: String,
    pub sale_reserve: u128,
    pub graduation_reserve: u128,
    pub graduation_policy: GraduationPolicy,
    pub creator_allocation: u128,
    pub virtual_token_reserve: u128,
    pub virtual_collateral_reserve: u128,
    /// Optional sale end time in Unix seconds.
    pub end_timestamp: Option<u64>,
    /// Clock lower bound in Unix seconds, preserved as supplied, including zero.
    pub now: u64,
}

impl CreateSaleRequest {
    pub fn total_supply(&self) -> Result<u128> {
        factory_core::total_supply(
            self.sale_reserve,
            self.graduation_reserve,
            self.creator_allocation,
        )
        .map_err(Into::into)
    }
}

pub(super) fn factory_pool_addresses(sale: FactorySale) -> FactoryPoolAddresses {
    let factory = compute_factory_pda(sale.namespace, sale.factory_program_id, sale.launch_salt);
    let token_definition =
        compute_definition_pda(sale.namespace, sale.factory_program_id, sale.launch_salt);
    FactoryPoolAddresses {
        factory,
        token_definition,
        creator_escrow: compute_escrow_pda(
            sale.namespace,
            sale.factory_program_id,
            sale.launch_salt,
        ),
        pool: compute_pool_pda(
            sale.namespace,
            sale.curve_program_id,
            token_definition,
            sale.collateral_definition,
            factory,
        ),
    }
}

pub(super) fn factory_actors(
    sale: FactorySale,
    creator: AccountId,
    treasury: Option<AccountId>,
) -> Vec<Actor> {
    let a = factory_pool_addresses(sale);
    let mint =
        factory_core::compute_mint_pda(sale.namespace, sale.factory_program_id, sale.launch_salt);
    let mut actors = vec![
        Actor::new(
            factory_core::compute_curve_creator_role_pda(
                sale.namespace,
                sale.factory_program_id,
                sale.launch_salt,
            ),
            curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
        ),
        Actor::new(creator, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
        token_actor(mint),
        token_actor(a.creator_escrow),
        Actor::new(a.factory, sale.factory_program_id),
        Actor::new(creator, sale.factory_program_id),
        Actor::new(a.pool, sale.curve_program_id),
        Actor::new(a.factory, sale.curve_program_id),
        Actor::new(
            compute_config_pda(sale.namespace, sale.curve_program_id),
            sale.curve_program_id,
        ),
        clock_actor(),
        token_actor(a.token_definition),
        token_actor(sale.collateral_definition),
        token_actor(a.factory),
        token_actor(a.pool),
        token_actor(creator),
    ];
    for owner in [a.factory, a.pool, a.creator_escrow, mint, creator]
        .into_iter()
        .chain(treasury)
    {
        for definition in [a.token_definition, sale.collateral_definition] {
            actors.push(token_actor(associated_token_account(owner, definition)));
        }
    }
    actors
}

pub(super) fn admit_factory_accounts<I>(
    mut call: PublicInvocation<I>,
    sale: FactorySale,
) -> PublicInvocation<I> {
    let a = factory_pool_addresses(sale);
    for seed in [
        factory_core::compute_factory_seed(sale.namespace, sale.launch_salt),
        factory_core::compute_curve_creator_role_seed(sale.namespace, sale.launch_salt),
        factory_core::compute_definition_seed(sale.namespace, sale.launch_salt),
        factory_core::compute_mint_seed(sale.namespace, sale.launch_salt),
        factory_core::compute_escrow_seed(sale.namespace, sale.launch_salt),
    ] {
        call = call.with_pda(sale.factory_program_id, seed);
    }
    call = call
        .with_pda(
            sale.curve_program_id,
            curve_core::compute_config_pda_seed(sale.namespace),
        )
        .with_pda(
            sale.curve_program_id,
            curve_core::compute_pool_pda_seed(
                sale.namespace,
                a.token_definition,
                sale.collateral_definition,
                a.factory,
            ),
        );
    for actor in call.actors.clone() {
        if actor.program_account_id != TOKEN_PROGRAM_ACCOUNT_ID {
            continue;
        }
        for owner in [
            a.factory,
            a.pool,
            a.creator_escrow,
            factory_core::compute_mint_pda(
                sale.namespace,
                sale.factory_program_id,
                sale.launch_salt,
            ),
        ] {
            for definition in [a.token_definition, sale.collateral_definition] {
                if actor.account_id == associated_token_account(owner, definition) {
                    call = call.with_holding(owner, definition);
                }
            }
        }
    }
    call
}

/// Bind an existing allowlist root to the canonical curve pool for this sale.
pub fn curve_buy_policy(sale: FactorySale, allowlist_root: Option<[u8; 32]>) -> BuyPolicy {
    sale_buy_policy(sale.namespace, allowlist_root, || {
        factory_pool_addresses(sale).pool
    })
}

pub fn build_create_sale_invocation(
    sale: FactorySale,
    creator: AccountId,
    request: CreateSaleRequest,
) -> Result<PublicInvocation<FactoryInstruction>> {
    request.total_supply()?;
    if request.per_block_token_limit == Some(0) {
        return Err(factory_core::Error::InvalidTerms.into());
    }
    factory_core::validate_curve_parameters(
        request.sale_reserve,
        request.virtual_token_reserve,
        request.virtual_collateral_reserve,
    )?;
    factory_core::validate_curve_policies(request.ending_policy, request.end_timestamp)?;
    factory_core::validate_curve_expiry(request.end_timestamp, request.now)?;
    curve_core::SellFeeRate::new(request.sell_fee_bps)
        .map_err(|_| ClientError::FactoryPolicy(factory_core::Error::InvalidTerms))?;
    request
        .graduation_policy
        .validate(request.graduation_hook.is_some())
        .map_err(|_| ClientError::FactoryPolicy(factory_core::Error::InvalidTerms))?;
    let a = factory_pool_addresses(sale);
    request
        .buy_policy
        .validate_binding(sale.namespace.value(), a.pool.value())
        .map_err(|_| ClientError::FactoryPolicy(factory_core::Error::InvalidTerms))?;
    Ok(admit_factory_accounts(
        invocation(
            Actor::new(a.factory, sale.factory_program_id),
            factory_actors(sale, creator, None),
            vec![creator],
            FactoryInstruction::CreateFactoryPool {
                buy_policy: request.buy_policy,
                pause_policy: request.pause_policy,
                ending_policy: request.ending_policy,
                trade_mode: request.trade_mode as u8,
                sell_fee_bps: request.sell_fee_bps,
                per_block_token_limit: request.per_block_token_limit,
                graduation_hook_program_id: request.graduation_hook.as_ref().map(|h| h.program_id),
                graduation_hook_configuration: request
                    .graduation_hook
                    .map_or_else(Vec::new, |h| h.configuration),
                namespace: sale.namespace,
                launch_salt: sale.launch_salt,
                name: request.name,
                uri: request.uri,
                sale_reserve: request.sale_reserve,
                graduation_reserve: request.graduation_reserve,
                graduation_policy: request.graduation_policy,
                creator_allocation: request.creator_allocation,
                virtual_token_reserve: request.virtual_token_reserve,
                virtual_collateral_reserve: request.virtual_collateral_reserve,
                end_timestamp: request.end_timestamp,
                curve_program_id: sale.curve_program_id,
                collateral_definition_id: sale.collateral_definition,
                creator,
                now: request.now,
            },
        ),
        sale,
    )
    .with_holding(creator, a.token_definition)
    .with_holding(creator, sale.collateral_definition))
}

pub fn build_set_factory_paused_invocation(
    sale: FactorySale,
    creator: AccountId,
    paused: bool,
) -> PublicInvocation<FactoryInstruction> {
    let a = factory_pool_addresses(sale);
    admit_factory_accounts(
        invocation(
            Actor::new(a.factory, sale.factory_program_id),
            factory_actors(sale, creator, None),
            vec![creator],
            FactoryInstruction::SetPaused { creator, paused },
        ),
        sale,
    )
}

pub fn build_set_curve_paused_invocation(
    namespace: AccountId,
    curve: AccountId,
    pool: AccountId,
    owner: AccountId,
    paused: bool,
) -> PublicInvocation<CurveInstruction> {
    invocation(
        Actor::new(owner, curve),
        vec![
            Actor::new(pool, curve),
            Actor::new(
                curve_core::compute_pool_owner_role(namespace, pool, curve),
                curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
            ),
            Actor::new(owner, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
            clock_actor(),
        ],
        vec![owner],
        CurveInstruction::Execute {
            pool,
            instruction: Box::new(CurveInstruction::SetPaused { paused }),
        },
    )
    .with_pda(
        curve,
        curve_core::compute_pool_owner_role_seed(namespace, pool),
    )
}

pub fn build_withdraw_factory_proceeds_invocation(
    sale: FactorySale,
    creator: AccountId,
    treasury: AccountId,
    now: u64,
) -> PublicInvocation<FactoryInstruction> {
    let a = factory_pool_addresses(sale);
    admit_factory_accounts(
        invocation(
            Actor::new(a.factory, sale.factory_program_id),
            factory_actors(sale, creator, Some(treasury)),
            vec![creator],
            FactoryInstruction::WithdrawFactoryProceeds { creator, now },
        ),
        sale,
    )
    .with_holding(creator, a.token_definition)
    .with_holding(creator, sale.collateral_definition)
    .with_holding(treasury, a.token_definition)
    .with_holding(treasury, sale.collateral_definition)
}
pub fn build_claim_creator_allocation_invocation(
    sale: FactorySale,
    creator: AccountId,
    now: u64,
) -> PublicInvocation<FactoryInstruction> {
    let a = factory_pool_addresses(sale);
    admit_factory_accounts(
        invocation(
            Actor::new(a.factory, sale.factory_program_id),
            factory_actors(sale, creator, None),
            vec![creator],
            FactoryInstruction::ClaimCreatorAllocation { creator, now },
        ),
        sale,
    )
    .with_holding(creator, a.token_definition)
    .with_holding(creator, sale.collateral_definition)
}

fn trade_invocation(
    sale: FactorySale,
    participant: AccountId,
    instruction: CurveInstruction,
) -> PublicInvocation<CurveInstruction> {
    let a = factory_pool_addresses(sale);
    let mut actors = vec![
        Actor::new(a.pool, sale.curve_program_id),
        Actor::new(
            compute_config_pda(sale.namespace, sale.curve_program_id),
            sale.curve_program_id,
        ),
        token_actor(participant),
        token_actor(a.pool),
        clock_actor(),
    ];
    for owner in [participant, a.pool] {
        for definition in [a.token_definition, sale.collateral_definition] {
            actors.push(token_actor(associated_token_account(owner, definition)));
        }
    }
    let mut call = invocation(
        Actor::new(participant, sale.curve_program_id),
        actors,
        vec![participant],
        CurveInstruction::Execute {
            pool: a.pool,
            instruction: Box::new(instruction),
        },
    );
    for owner in [participant, a.pool] {
        for definition in [a.token_definition, sale.collateral_definition] {
            call = call.with_holding(owner, definition);
        }
    }
    call
}
#[allow(clippy::too_many_arguments)]
pub fn prepare_buy_invocation(
    sale: FactorySale,
    participant: AccountId,
    pool: &curve_core::PoolAccount,
    config: &Config,
    terms: ExactOutputTradeTerms,
    membership: Option<MembershipPath>,
    now: u64,
    observed_block: Option<u64>,
) -> Result<PublicInvocation<CurveInstruction>> {
    let quote = quote_buy(pool, config, terms.amount_out, now, observed_block)?;
    if quote.amount_in > terms.max_amount_in {
        return Err(ClientError::QuoteBuy(pool::SwapError::SlippageExceeded));
    }
    Ok(build_buy_invocation(
        sale,
        participant,
        terms,
        membership,
        now,
    ))
}

pub fn build_buy_invocation(
    sale: FactorySale,
    participant: AccountId,
    terms: ExactOutputTradeTerms,
    membership: Option<MembershipPath>,
    now: u64,
) -> PublicInvocation<CurveInstruction> {
    trade_invocation(
        sale,
        participant,
        CurveInstruction::SwapExactOutput {
            amount_out: terms.amount_out,
            max_amount_in: terms.max_amount_in,
            token_in: sale.collateral_definition,
            membership,
            now,
        },
    )
}
#[allow(clippy::too_many_arguments)]
pub fn prepare_buy_with_collateral_invocation(
    sale: FactorySale,
    participant: AccountId,
    pool: &curve_core::PoolAccount,
    config: &Config,
    terms: ExactInputTradeTerms,
    membership: Option<MembershipPath>,
    now: u64,
    observed_block: Option<u64>,
) -> Result<PublicInvocation<CurveInstruction>> {
    let quote = quote_buy_with_collateral(pool, config, terms.amount_in, now, observed_block)?;
    if quote.amount_out < terms.min_amount_out {
        return Err(ClientError::QuoteBuyWithCollateral(
            pool::SwapError::SlippageExceeded,
        ));
    }
    Ok(build_buy_with_collateral_invocation(
        sale,
        participant,
        terms,
        membership,
        now,
    ))
}

pub fn build_buy_with_collateral_invocation(
    sale: FactorySale,
    participant: AccountId,
    terms: ExactInputTradeTerms,
    membership: Option<MembershipPath>,
    now: u64,
) -> PublicInvocation<CurveInstruction> {
    trade_invocation(
        sale,
        participant,
        CurveInstruction::SwapExactInput {
            amount_in: terms.amount_in,
            min_amount_out: terms.min_amount_out,
            token_in: sale.collateral_definition,
            membership,
            now,
        },
    )
}
pub fn prepare_sell_invocation(
    sale: FactorySale,
    participant: AccountId,
    pool: &curve_core::PoolAccount,
    config: &Config,
    terms: ExactInputTradeTerms,
    now: u64,
) -> Result<PublicInvocation<CurveInstruction>> {
    let quote = quote_sell(pool, config, terms.amount_in, now)?;
    if quote.amount_out < terms.min_amount_out {
        return Err(ClientError::QuoteSell(pool::SwapError::SlippageExceeded));
    }
    Ok(build_sell_invocation(sale, participant, terms, now))
}

pub fn build_sell_invocation(
    sale: FactorySale,
    participant: AccountId,
    terms: ExactInputTradeTerms,
    now: u64,
) -> PublicInvocation<CurveInstruction> {
    trade_invocation(
        sale,
        participant,
        CurveInstruction::SwapExactInput {
            amount_in: terms.amount_in,
            min_amount_out: terms.min_amount_out,
            token_in: factory_pool_addresses(sale).token_definition,
            membership: None,
            now,
        },
    )
}

pub async fn load_curve_config(
    namespace: AccountId,
    wallet: &WalletCore<impl WalletSequencerClient>,
    curve: AccountId,
) -> Result<Config> {
    let data = read_actor(
        wallet,
        Actor::new(compute_config_pda(namespace, curve), curve),
    )
    .await
    .map_err(ClientError::ReadCurveConfig)?;
    let config = Config::decode(&data).map_err(ClientError::DecodeCurveConfig)?;
    if config.namespace != namespace {
        return Err(ClientError::PoolSelectionMismatch);
    }
    Ok(config)
}
pub async fn load_factory_state(
    wallet: &WalletCore<impl WalletSequencerClient>,
    sale: FactorySale,
) -> Result<FactoryState> {
    let a = factory_pool_addresses(sale);
    let data = read_actor(wallet, Actor::new(a.factory, sale.factory_program_id))
        .await
        .map_err(ClientError::ReadFactoryState)?;
    let state = FactoryState::decode(&data).map_err(ClientError::DecodeFactoryState)?;
    validate_factory_state(sale, &state)?;
    Ok(state)
}
pub async fn load_factory_pool(
    wallet: &WalletCore<impl WalletSequencerClient>,
    sale: FactorySale,
) -> Result<curve_core::PoolAccount> {
    let a = factory_pool_addresses(sale);
    let data = read_actor(wallet, Actor::new(a.pool, sale.curve_program_id))
        .await
        .map_err(ClientError::ReadFactoryPool)?;
    let state = curve_core::PoolAccount::decode(&data).map_err(ClientError::DecodeFactoryPool)?;
    let factory = load_factory_state(wallet, sale).await?;
    validate_factory_pool(sale, &factory, &state)?;
    Ok(state)
}

pub(crate) fn validate_factory_pool(
    sale: FactorySale,
    factory: &FactoryState,
    state: &curve_core::PoolAccount,
) -> Result<()> {
    validate_factory_state(sale, factory)?;
    let a = factory_pool_addresses(sale);
    if state.namespace != sale.namespace
        || state.token0_definition_id != a.token_definition
        || state.token1_definition_id != sale.collateral_definition
        || state.owner != a.factory
        || state.owner_program
            != Some((
                sale.factory_program_id,
                *factory_core::compute_factory_seed(sale.namespace, sale.launch_salt).as_bytes(),
            ))
        || state.buy_policy != factory.buy_policy
        || state.trade_control.policy() != factory.pause_policy
        || state.pool.ending_policy != factory.ending_policy
        || state.pool.sell_fee_bps != factory.sell_fee_bps
        || state.block_allocation.limit != factory.per_block_token_limit
        || state.pool.validate_policy().is_err()
        || Some(state.pool.initial_k)
            != factory
                .virtual_token_reserve
                .checked_mul(factory.virtual_collateral_reserve)
        || state.pool.close_timestamp != factory.end_timestamp
        || state.pool.close_on_depletion
            != match factory.ending_policy {
                EndingPolicy::Scheduled => None,
                _ => Some(pool::TokenSide::Token0),
            }
        || state.allowed_input_token
            != match factory.trade_mode {
                TradeMode::BuyOnly => Some(pool::TokenSide::Token1),
                TradeMode::BuyAndSell => None,
            }
    {
        return Err(ClientError::FactorySaleMismatch);
    }
    Ok(())
}
pub(crate) fn validate_factory_state(sale: FactorySale, state: &FactoryState) -> Result<()> {
    curve_core::SellFeeRate::new(state.sell_fee_bps)
        .map_err(|_| ClientError::FactorySaleMismatch)?;
    factory_core::validate_curve_policies(state.ending_policy, state.end_timestamp)
        .map_err(|_| ClientError::FactorySaleMismatch)?;
    let a = factory_pool_addresses(sale);
    if state.namespace != sale.namespace
        || state.launch_salt != sale.launch_salt
        || state.curve_program_id != sale.curve_program_id
        || state.collateral_definition_id != sale.collateral_definition
        || state.token_definition_id != a.token_definition
        || state.creator_escrow_id != a.creator_escrow
        || state.pool_id != a.pool
        || state.creator_role_id
            != factory_core::compute_curve_creator_role_pda(
                sale.namespace,
                sale.factory_program_id,
                sale.launch_salt,
            )
    {
        return Err(ClientError::FactorySaleMismatch);
    }
    Ok(())
}

pub fn build_collect_fees_invocation(
    namespace: AccountId,
    curve: AccountId,
    pool: AccountId,
    collateral: AccountId,
    treasury: AccountId,
) -> PublicInvocation<CurveInstruction> {
    invocation(
        Actor::new(pool, curve),
        vec![
            Actor::new(compute_config_pda(namespace, curve), curve),
            token_actor(pool),
            token_actor(collateral),
            token_actor(associated_token_account(pool, collateral)),
            token_actor(associated_token_account(treasury, collateral)),
        ],
        vec![],
        CurveInstruction::CollectFees,
    )
    .with_holding(pool, collateral)
    .with_holding(treasury, collateral)
}
pub async fn prepare_fee_collection(
    wallet: &WalletCore<impl WalletSequencerClient>,
    namespace: AccountId,
    curve: AccountId,
    _collateral: AccountId,
) -> Result<AccountId> {
    // The token actor creates the canonical destination during its synchronous credit.
    Ok(load_curve_config(namespace, wallet, curve).await?.treasury)
}
pub async fn collect_pool_fees(
    wallet: &PublicWriteWallet<impl WalletSequencerClient>,
    namespace: AccountId,
    curve: &ProgramHandle,
    targets: &[PoolFeeTarget],
    payer: AccountId,
) -> Result<Vec<HashType>> {
    let mut hashes = Vec::new();
    for target in targets {
        match collect_pool_fee(wallet, namespace, curve, target, payer).await {
            Ok(hash) => hashes.push(hash),
            Err(error) => return Err(error.after_execution(hashes)),
        }
    }
    Ok(hashes)
}

async fn collect_pool_fee(
    wallet: &PublicWriteWallet<impl WalletSequencerClient>,
    namespace: AccountId,
    curve: &ProgramHandle,
    target: &PoolFeeTarget,
    payer: AccountId,
) -> Result<HashType> {
    let data = read_actor(wallet, Actor::new(target.pool, curve.account_id))
        .await
        .map_err(ClientError::ReadCollectionPool)?;
    let pool = curve_core::PoolAccount::decode(&data).map_err(ClientError::DecodeCollectionPool)?;
    if pool.namespace != namespace || pool.token1_definition_id != target.collateral_definition {
        return Err(ClientError::PoolSelectionMismatch);
    }
    let treasury = prepare_fee_collection(
        wallet,
        namespace,
        curve.account_id,
        target.collateral_definition,
    )
    .await?;
    let mut call = build_collect_fees_invocation(
        namespace,
        curve.account_id,
        target.pool,
        target.collateral_definition,
        treasury,
    );
    call.fee_payer = Some(payer);
    submit_public_invocation(wallet, curve, call).await
}
