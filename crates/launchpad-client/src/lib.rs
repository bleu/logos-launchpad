//! Transaction builders, quotes and resumable launch workflows for LEZ actors.
//! Public timestamps use Unix seconds; native clock conversion belongs to program handlers.
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod error;
mod lifecycle;
mod types;
pub use curve_core::{
    AllowlistPolicy, BuyPolicy, EndingPolicy, MembershipPath, PausePolicy,
    TOKEN_PROGRAM_ACCOUNT_ID, TradeControl,
};
pub use error::{ClientError, Result};
pub use factory_core::{CreationStage, GraduationHook, SettlementStage, TradeMode};
pub use factory_core::{GraduationPolicy, SettlementPayouts};
pub use lbp_core::{QuoteRequest as LbpQuoteRequest, TradeQuote as LbpTradeQuote};
pub use lifecycle::{FactorySession, LifecycleReceipt, next_creation_invocation};
pub use types::{
    AccountLocation, ExactInputTradeTerms, ExactOutputTradeTerms, FactorySale, LbpSale,
    PoolFeeTarget, PrivateAccountId,
};

#[cfg(test)]
mod tests;

pub mod distribution;

use borsh::BorshSerialize;
use common::HashType;
use curve_core::{Config, Message as CurveInstruction, compute_config_pda, compute_pool_pda};
use factory_core::{
    FactoryState, Message as FactoryInstruction, compute_definition_pda, compute_escrow_pda,
    compute_factory_pda,
};
use lee::{AccountId, program::Program};
use lee_core::{account::Actor, program::ProgramId};
use lez_compat_core::token;
use program_loader_core::LoaderActorState as _;
use std::path::Path;
use wallet::AccountIdentity;

/// The upstream wallet with the RPC client used by the launchpad SDK and CLI.
pub type LaunchpadWallet = wallet::WalletCore<sequencer_service_rpc::SequencerClient>;

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

/// All creation terms are immutable. `now` is a clock lower bound, not a saved sale term.
#[derive(Debug, Clone)]
pub struct CreateSaleRequest {
    pub buy_policy: BuyPolicy,
    pub pause_policy: PausePolicy,
    pub ending_policy: EndingPolicy,
    pub trade_mode: TradeMode,
    pub sell_fee_bps: u16,
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

#[derive(Debug, Clone, Copy)]
pub struct PrivateBuyRequest {
    pub amount_out: u128,
    pub max_collateral_in: u128,
    pub from_private: PrivateAccountId,
    pub to_private: PrivateAccountId,
    pub gas_reserve: u128,
}

pub fn validate_private_buy_request(request: PrivateBuyRequest) -> Result<()> {
    if request.gas_reserve == 0 {
        return Err(ClientError::ZeroPrivateBuyGasReserve);
    }
    if request.amount_out == 0 || request.max_collateral_in == 0 {
        return Err(ClientError::ZeroPrivateBuyAmount);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct PrivateBuyReceipt {
    pub transaction_hash: HashType,
    pub transient_public_account: AccountId,
    pub private_destination: AccountId,
}
pub struct PrivateBuyRoute<'a> {
    pub private_buy_program: &'a Program,
    pub curve_program: &'a Program,
}

/// Canonical public token holdings do not implement the full private route.
pub async fn submit_private_buy(
    _wallet: &mut LaunchpadWallet,
    _sale: FactorySale,
    _route: PrivateBuyRoute<'_>,
    request: PrivateBuyRequest,
) -> Result<PrivateBuyReceipt> {
    validate_private_buy_request(request)?;
    Err(ClientError::UnsupportedPrivateRoute)
}

/// Typed message and declared actor states. Actor order does not encode instruction arguments.
#[derive(Debug, Clone)]
pub struct PublicInvocation<I> {
    pub destination: Actor,
    pub actors: Vec<Actor>,
    pub signer_accounts: Vec<AccountId>,
    pub instruction: I,
    pub fee_payer: Option<AccountId>,
    pub pda_accounts: Vec<(AccountId, lee_core::program::PdaSeed)>,
}
impl<I> PublicInvocation<I> {
    /// Wallet mentions retain admission evidence without granting PDA authorization.
    pub fn account_mentions(&self) -> Vec<wallet::AccountMention> {
        self.actors
            .iter()
            .map(|actor| {
                let signs = self.signer_accounts.contains(&actor.account_id);
                let identity = if signs {
                    AccountIdentity::Public(actor.account_id)
                } else if let Some((program, seed)) =
                    self.pda_accounts.iter().find(|(program, seed)| {
                        AccountId::for_public_pda(program, seed) == actor.account_id
                    })
                {
                    AccountIdentity::PublicPda {
                        program: *program,
                        seed: *seed,
                    }
                } else {
                    AccountIdentity::PublicNoSign(actor.account_id)
                };
                let mention = identity.select_program_actor_state(actor.program_account_id);
                if signs {
                    mention
                } else {
                    mention.without_authorization()
                }
            })
            .collect()
    }
    fn with_pda(mut self, program: AccountId, seed: lee_core::program::PdaSeed) -> Self {
        if !self.pda_accounts.contains(&(program, seed)) {
            self.pda_accounts.push((program, seed));
        }
        self
    }
    fn with_holding(self, owner: AccountId, definition: AccountId) -> Self {
        self.with_pda(
            TOKEN_PROGRAM_ACCOUNT_ID,
            token::holding_seed(owner, definition),
        )
    }
}
impl<I: BorshSerialize> PublicInvocation<I> {
    pub fn message_bytes(&self) -> Result<Vec<u8>> {
        borsh::to_vec(&self.instruction).map_err(ClientError::EncodeMessage)
    }
}

fn invocation<I>(
    destination: Actor,
    mut actors: Vec<Actor>,
    signers: Vec<AccountId>,
    instruction: I,
) -> PublicInvocation<I> {
    actors.insert(0, destination);
    let mut unique = Vec::new();
    for actor in actors {
        if !unique.contains(&actor) {
            unique.push(actor);
        }
    }
    PublicInvocation {
        destination,
        actors: unique,
        signer_accounts: signers,
        instruction,
        fee_payer: None,
        pda_accounts: vec![],
    }
}

fn namespace_actors(namespace: AccountId, curve: AccountId, signer: AccountId) -> Vec<Actor> {
    vec![
        Actor::new(signer, curve),
        Actor::new(compute_config_pda(namespace, curve), curve),
        Actor::new(
            curve_core::compute_admin_role(namespace, curve),
            curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
        ),
        Actor::new(signer, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
    ]
}

pub fn build_initialize_namespace_invocation(
    namespace: AccountId,
    curve: AccountId,
    fee: u16,
    treasury: AccountId,
) -> PublicInvocation<CurveInstruction> {
    invocation(
        Actor::new(namespace, curve),
        namespace_actors(namespace, curve, namespace),
        vec![namespace],
        CurveInstruction::InitializeNamespace {
            namespace,
            protocol_fee_bps: fee.into(),
            treasury,
        },
    )
    .with_pda(curve, curve_core::compute_config_pda_seed(namespace))
    .with_pda(curve, curve_core::compute_admin_role_seed(namespace))
}
pub fn build_update_fee_invocation(
    namespace: AccountId,
    curve: AccountId,
    admin: AccountId,
    fee: u16,
) -> PublicInvocation<CurveInstruction> {
    invocation(
        Actor::new(admin, curve),
        namespace_actors(namespace, curve, admin),
        vec![admin],
        CurveInstruction::UpdateFee {
            namespace,
            protocol_fee_bps: fee.into(),
        },
    )
}
pub fn build_update_treasury_invocation(
    namespace: AccountId,
    curve: AccountId,
    admin: AccountId,
    treasury: AccountId,
) -> PublicInvocation<CurveInstruction> {
    invocation(
        Actor::new(admin, curve),
        namespace_actors(namespace, curve, admin),
        vec![admin],
        CurveInstruction::UpdateTreasury {
            namespace,
            treasury,
        },
    )
}
pub fn build_renounce_admin_invocation(
    namespace: AccountId,
    curve: AccountId,
    admin: AccountId,
) -> PublicInvocation<CurveInstruction> {
    invocation(
        Actor::new(admin, curve),
        namespace_actors(namespace, curve, admin),
        vec![admin],
        CurveInstruction::RenounceAdmin { namespace },
    )
}
pub fn build_transfer_admin_invocation(
    namespace: AccountId,
    curve: AccountId,
    admin: AccountId,
    new_admin: AccountId,
    candidate: curve_core::AdminCandidate,
) -> PublicInvocation<CurveInstruction> {
    let mut actors = namespace_actors(namespace, curve, admin);
    actors.push(Actor::new(
        new_admin,
        curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
    ));
    let signers = match candidate {
        curve_core::AdminCandidate::Signer => vec![admin, new_admin],
        curve_core::AdminCandidate::Pda { program_id, .. } => {
            actors.push(Actor::new(new_admin, program_id));
            vec![admin]
        }
    };
    let successor_proof = match &candidate {
        curve_core::AdminCandidate::Pda { program_id, seed } => {
            let seed = lee_core::program::PdaSeed::new(*seed);
            (AccountId::for_public_pda(program_id, &seed) == new_admin)
                .then_some((*program_id, seed))
        }
        curve_core::AdminCandidate::Signer => None,
    };
    let call = invocation(
        Actor::new(admin, curve),
        actors,
        signers,
        CurveInstruction::TransferAdmin {
            namespace,
            new_admin,
            candidate,
        },
    );
    if let Some((program, seed)) = successor_proof {
        return call.with_pda(program, seed);
    }
    call
}

#[derive(Debug, Clone, Copy)]
struct FactoryPoolAddresses {
    factory: AccountId,
    token_definition: AccountId,
    creator_escrow: AccountId,
    pool: AccountId,
}
fn factory_pool_addresses(sale: FactorySale) -> FactoryPoolAddresses {
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
fn associated_token_account(owner: AccountId, definition: AccountId) -> AccountId {
    token::holding_address(curve_core::TOKEN_PROGRAM_ACCOUNT_ID, owner, definition)
}
fn token_actor(account: AccountId) -> Actor {
    Actor::new(account, curve_core::TOKEN_PROGRAM_ACCOUNT_ID)
}
fn clock_actor() -> Actor {
    clock_core::clock_actor()
}
fn factory_actors(
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

fn admit_factory_accounts<I>(
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

pub fn build_create_sale_invocation(
    sale: FactorySale,
    creator: AccountId,
    request: CreateSaleRequest,
) -> Result<PublicInvocation<FactoryInstruction>> {
    request.total_supply()?;
    factory_core::validate_curve_parameters(
        request.sale_reserve,
        request.virtual_token_reserve,
        request.virtual_collateral_reserve,
    )?;
    factory_core::validate_curve_policies(request.ending_policy, request.end_timestamp)?;
    curve_core::SellFeeRate::new(request.sell_fee_bps)
        .map_err(|_| ClientError::FactoryPolicy(factory_core::Error::InvalidTerms))?;
    request
        .graduation_policy
        .validate(request.graduation_hook.is_some())
        .map_err(|_| ClientError::FactoryPolicy(factory_core::Error::InvalidTerms))?;
    let a = factory_pool_addresses(sale);
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

/// Persisted terms select the adapter. A creator hint only admits the resolved payout accounts.
#[derive(Debug, Clone, Copy)]
pub enum GraduationTarget<'a> {
    Curve {
        sale: FactorySale,
        state: &'a FactoryState,
        creator_hint: AccountId,
    },
    Lbp {
        sale: LbpSale,
        state: &'a factory_core::LbpLaunchState,
        creator_hint: AccountId,
    },
}

/// The callback has a fixed typed context and no arbitrary account extension.
pub fn build_graduation_invocation(
    target: GraduationTarget<'_>,
    treasury: AccountId,
) -> Result<PublicInvocation<FactoryInstruction>> {
    let (mut call, factory, hook, definition, collateral) = match target {
        GraduationTarget::Curve {
            sale,
            state,
            creator_hint,
        } => {
            validate_factory_state(sale, state)?;
            let hook = state
                .graduation_hook
                .as_ref()
                .ok_or(ClientError::GraduationNotConfigured)?;
            let a = factory_pool_addresses(sale);
            let call = admit_factory_accounts(
                invocation(
                    Actor::new(a.factory, sale.factory_program_id),
                    factory_actors(sale, creator_hint, Some(treasury)),
                    vec![],
                    FactoryInstruction::ExecuteGraduationHook {
                        creator_hint: Some(creator_hint),
                    },
                ),
                sale,
            )
            .with_holding(creator_hint, a.token_definition)
            .with_holding(creator_hint, sale.collateral_definition);
            (
                call,
                a.factory,
                hook,
                a.token_definition,
                sale.collateral_definition,
            )
        }
        GraduationTarget::Lbp {
            sale,
            state,
            creator_hint,
        } => {
            validate_lbp_factory_state(sale, state)?;
            let hook = state
                .graduation_hook
                .as_ref()
                .ok_or(ClientError::GraduationNotConfigured)?;
            let a = lbp_addresses(sale);
            let mut call = lbp_setup_invocation(
                sale,
                creator_hint,
                FactoryInstruction::ExecuteGraduationHook {
                    creator_hint: Some(creator_hint),
                },
            );
            call.signer_accounts.clear();
            (
                call,
                a.factory,
                hook,
                a.token_definition,
                sale.collateral_definition,
            )
        }
    };
    let custody = factory_core::compute_graduation_pda(hook.program_id, factory);
    call.actors.push(Actor::new(custody, hook.program_id));
    call.actors.push(token_actor(custody));
    for owner in [custody, treasury] {
        for token in [definition, collateral] {
            call.actors
                .push(token_actor(associated_token_account(owner, token)));
            call = call.with_holding(owner, token);
        }
    }
    let mut seen = std::collections::HashSet::new();
    call.actors.retain(|actor| seen.insert(*actor));
    Ok(call.with_pda(
        hook.program_id,
        factory_core::compute_graduation_seed(factory),
    ))
}

fn validate_lbp_factory_state(sale: LbpSale, state: &factory_core::LbpLaunchState) -> Result<()> {
    let a = lbp_addresses(sale);
    if state.namespace != sale.namespace
        || state.launch_salt != sale.launch_salt
        || state.namespace_program_id != sale.namespace_program_id
        || state.lbp_program_id != sale.lbp_program_id
        || state.collateral_definition_id != sale.collateral_definition
        || state.token_definition_id != a.token_definition
        || state.creator_escrow_id != a.creator_escrow
        || state.pool_id != a.pool
        || state.creator_role_id
            != factory_core::compute_creator_role_pda(
                sale.namespace,
                sale.factory_program_id,
                sale.launch_salt,
            )
    {
        return Err(ClientError::FactorySaleMismatch);
    }
    Ok(())
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
pub fn prepare_buy_invocation(
    sale: FactorySale,
    participant: AccountId,
    pool: &curve_core::PoolAccount,
    config: &Config,
    terms: ExactOutputTradeTerms,
    membership: Option<MembershipPath>,
    now: u64,
) -> Result<PublicInvocation<CurveInstruction>> {
    let quote = quote_buy(pool, config, terms.amount_out, now)?;
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
pub fn prepare_buy_with_collateral_invocation(
    sale: FactorySale,
    participant: AccountId,
    pool: &curve_core::PoolAccount,
    config: &Config,
    terms: ExactInputTradeTerms,
    membership: Option<MembershipPath>,
    now: u64,
) -> Result<PublicInvocation<CurveInstruction>> {
    let quote = quote_buy_with_collateral(pool, config, terms.amount_in, now)?;
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

fn lbp_addresses(sale: LbpSale) -> FactoryPoolAddresses {
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
        pool: lbp_core::compute_pool_pda(
            sale.namespace,
            sale.lbp_program_id,
            token_definition,
            sale.collateral_definition,
            factory,
        ),
    }
}

fn lbp_setup_invocation(
    sale: LbpSale,
    creator: AccountId,
    instruction: FactoryInstruction,
) -> PublicInvocation<FactoryInstruction> {
    let a = lbp_addresses(sale);
    let mint =
        factory_core::compute_mint_pda(sale.namespace, sale.factory_program_id, sale.launch_salt);
    let role = factory_core::compute_creator_role_pda(
        sale.namespace,
        sale.factory_program_id,
        sale.launch_salt,
    );
    let actors = vec![
        Actor::new(a.factory, sale.factory_program_id),
        Actor::new(creator, sale.factory_program_id),
        Actor::new(role, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
        Actor::new(creator, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
        Actor::new(a.pool, sale.lbp_program_id),
        Actor::new(a.factory, sale.lbp_program_id),
        Actor::new(
            compute_config_pda(sale.namespace, sale.namespace_program_id),
            sale.namespace_program_id,
        ),
        token_actor(mint),
        token_actor(a.token_definition),
        token_actor(sale.collateral_definition),
        token_actor(a.factory),
        token_actor(a.pool),
        token_actor(a.creator_escrow),
        token_actor(creator),
        clock_actor(),
    ];
    let mut call = invocation(
        Actor::new(a.factory, sale.factory_program_id),
        actors,
        vec![creator],
        instruction,
    );
    for seed in [
        factory_core::compute_factory_seed(sale.namespace, sale.launch_salt),
        factory_core::compute_creator_role_seed(sale.namespace, sale.launch_salt),
        factory_core::compute_definition_seed(sale.namespace, sale.launch_salt),
        factory_core::compute_mint_seed(sale.namespace, sale.launch_salt),
        factory_core::compute_escrow_seed(sale.namespace, sale.launch_salt),
    ] {
        call = call.with_pda(sale.factory_program_id, seed);
    }
    call = call.with_pda(
        sale.lbp_program_id,
        lbp_core::compute_pool_pda_seed(
            sale.namespace,
            a.token_definition,
            sale.collateral_definition,
            a.factory,
        ),
    );
    for owner in [mint, a.factory, a.pool, a.creator_escrow, creator] {
        for definition in [a.token_definition, sale.collateral_definition] {
            call.actors
                .push(token_actor(associated_token_account(owner, definition)));
            call = call.with_holding(owner, definition);
        }
    }
    call
}

pub fn build_set_lbp_paused_invocation(
    sale: LbpSale,
    creator: AccountId,
    paused: bool,
) -> PublicInvocation<FactoryInstruction> {
    lbp_setup_invocation(
        sale,
        creator,
        FactoryInstruction::SetPaused { creator, paused },
    )
}

pub fn build_create_fixed_price_invocation(
    factory_program_id: AccountId,
    inputs: factory_core::FixedPriceInputs,
) -> PublicInvocation<FactoryInstruction> {
    let sale = LbpSale {
        namespace: inputs.namespace,
        namespace_program_id: inputs.namespace_program_id,
        factory_program_id,
        lbp_program_id: inputs.lbp_program_id,
        launch_salt: inputs.launch_salt,
        collateral_definition: inputs.collateral_definition_id,
    };
    lbp_setup_invocation(
        sale,
        inputs.creator,
        FactoryInstruction::CreateFixedPriceLbp { inputs },
    )
}

pub fn build_create_weighted_invocation(
    factory_program_id: AccountId,
    inputs: factory_core::WeightedInputs,
) -> PublicInvocation<FactoryInstruction> {
    let sale = LbpSale {
        namespace: inputs.namespace,
        namespace_program_id: inputs.namespace_program_id,
        factory_program_id,
        lbp_program_id: inputs.lbp_program_id,
        launch_salt: inputs.launch_salt,
        collateral_definition: inputs.collateral_definition_id,
    };
    lbp_setup_invocation(
        sale,
        inputs.creator,
        FactoryInstruction::CreateWeightedLbp { inputs },
    )
}

pub fn build_continue_fixed_price_invocation(
    sale: LbpSale,
    creator: AccountId,
    expected_stage: factory_core::LbpCreationStage,
    now: u64,
) -> PublicInvocation<FactoryInstruction> {
    lbp_setup_invocation(
        sale,
        creator,
        FactoryInstruction::ContinueFixedPriceCreation {
            expected_stage,
            creator,
            now,
        },
    )
}

pub fn build_continue_weighted_invocation(
    sale: LbpSale,
    creator: AccountId,
    expected_stage: factory_core::LbpCreationStage,
) -> PublicInvocation<FactoryInstruction> {
    lbp_setup_invocation(
        sale,
        creator,
        FactoryInstruction::ContinueWeightedCreation {
            expected_stage,
            creator,
        },
    )
}

fn lbp_trade_invocation(
    sale: LbpSale,
    participant: AccountId,
    instruction: lbp_core::Message,
) -> PublicInvocation<lbp_core::Message> {
    let a = lbp_addresses(sale);
    let mut actors = vec![
        Actor::new(a.pool, sale.lbp_program_id),
        Actor::new(
            compute_config_pda(sale.namespace, sale.namespace_program_id),
            sale.namespace_program_id,
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
        Actor::new(participant, sale.lbp_program_id),
        actors,
        vec![participant],
        lbp_core::Message::Execute {
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

pub fn build_weighted_buy_with_collateral_invocation(
    sale: LbpSale,
    participant: AccountId,
    terms: ExactInputTradeTerms,
    membership: Option<MembershipPath>,
) -> PublicInvocation<lbp_core::Message> {
    lbp_trade_invocation(
        sale,
        participant,
        lbp_core::Message::BuyWeightedExactInput {
            membership,
            amount_in: terms.amount_in,
            min_amount_out: terms.min_amount_out,
        },
    )
}

pub fn build_weighted_buy_invocation(
    sale: LbpSale,
    participant: AccountId,
    terms: ExactOutputTradeTerms,
    membership: Option<MembershipPath>,
) -> PublicInvocation<lbp_core::Message> {
    lbp_trade_invocation(
        sale,
        participant,
        lbp_core::Message::BuyWeightedExactOutput {
            membership,
            amount_out: terms.amount_out,
            max_amount_in: terms.max_amount_in,
        },
    )
}

pub fn build_fixed_price_buy_with_collateral_invocation(
    sale: LbpSale,
    participant: AccountId,
    terms: ExactInputTradeTerms,
    membership: Option<MembershipPath>,
    now: u64,
) -> PublicInvocation<lbp_core::Message> {
    lbp_trade_invocation(
        sale,
        participant,
        lbp_core::Message::BuyExactInput {
            membership,
            amount_in: terms.amount_in,
            min_amount_out: terms.min_amount_out,
            now,
        },
    )
}

pub fn build_fixed_price_buy_invocation(
    sale: LbpSale,
    participant: AccountId,
    terms: ExactOutputTradeTerms,
    membership: Option<MembershipPath>,
    now: u64,
) -> PublicInvocation<lbp_core::Message> {
    lbp_trade_invocation(
        sale,
        participant,
        lbp_core::Message::BuyExactOutput {
            membership,
            amount_out: terms.amount_out,
            max_amount_in: terms.max_amount_in,
            now,
        },
    )
}

pub fn build_fixed_price_sell_invocation(
    sale: LbpSale,
    participant: AccountId,
    terms: ExactInputTradeTerms,
    now: u64,
) -> PublicInvocation<lbp_core::Message> {
    lbp_trade_invocation(
        sale,
        participant,
        lbp_core::Message::SellExactInput {
            amount_in: terms.amount_in,
            min_amount_out: terms.min_amount_out,
            now,
        },
    )
}

pub fn build_weighted_sell_invocation(
    sale: LbpSale,
    participant: AccountId,
    terms: ExactInputTradeTerms,
) -> PublicInvocation<lbp_core::Message> {
    lbp_trade_invocation(
        sale,
        participant,
        lbp_core::Message::SellWeightedExactInput {
            amount_in: terms.amount_in,
            min_amount_out: terms.min_amount_out,
        },
    )
}

/// Legacy NFT helper. This does not create a launch role.
pub fn build_create_authority_invocation(
    definition: AccountId,
    owner: AccountId,
    name: String,
    uri: String,
) -> PublicInvocation<token::Message> {
    invocation(
        token_actor(definition),
        vec![token_actor(associated_token_account(owner, definition))],
        vec![definition],
        token::Message::Create {
            name,
            kind: token::TokenKind::NftMaster,
            supply: 1,
            recipient: owner,
            metadata: Some(token::Metadata {
                standard: token::MetadataStandard::Simple,
                uri,
                creators: String::new(),
            }),
        },
    )
    .with_holding(owner, definition)
}
/// Legacy NFT helper. This does not transfer a launch role.
pub fn build_transfer_authority_invocation(
    owner: AccountId,
    recipient: AccountId,
    definition: AccountId,
) -> PublicInvocation<token::Message> {
    invocation(
        token_actor(owner),
        vec![
            token_actor(associated_token_account(owner, definition)),
            token_actor(associated_token_account(recipient, definition)),
        ],
        vec![owner],
        token::Message::Transfer {
            definition,
            recipient,
            amount: 1,
        },
    )
    .with_holding(owner, definition)
    .with_holding(recipient, definition)
}

pub trait PrivateInstruction: BorshSerialize + Clone {}
impl PrivateInstruction for CurveInstruction {}
impl PrivateInstruction for FactoryInstruction {}

pub fn build_private_authority_instruction<I: PrivateInstruction>(
    _action: &PublicInvocation<I>,
    _holder: AccountId,
) -> Result<()> {
    Err(ClientError::UnsupportedPrivateAuthority)
}
pub async fn submit_private_authority<I: PrivateInstruction>(
    _wallet: &mut LaunchpadWallet,
    _router: &ProgramHandle,
    _action_program: &ProgramHandle,
    _extra_dependencies: Vec<ProgramHandle>,
    _source: AccountId,
    _build: impl FnOnce(AccountId) -> Result<PublicInvocation<I>>,
) -> Result<PrivateBuyReceipt> {
    Err(ClientError::UnsupportedPrivateAuthority)
}

async fn read_actor(
    wallet: &LaunchpadWallet,
    actor: Actor,
) -> std::result::Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let account = wallet
        .get_account_view(actor)
        .await
        .map_err(|e| e.into_boxed_dyn_error())?
        .ok_or_else(|| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "actor state is missing",
            )) as Box<dyn std::error::Error + Send + Sync>
        })?;
    Ok(account
        .data
        .actor_state(actor.program_account_id)
        .as_ref()
        .to_vec())
}
pub async fn load_curve_config(
    namespace: AccountId,
    wallet: &LaunchpadWallet,
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
    wallet: &LaunchpadWallet,
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
    wallet: &LaunchpadWallet,
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

/// The upstream wallet selects fees, reads nonces, signs and submits.
pub async fn submit_public_invocation<I: BorshSerialize>(
    wallet: &LaunchpadWallet,
    program: &ProgramHandle,
    call: PublicInvocation<I>,
) -> Result<HashType> {
    if call.destination.program_account_id != program.account_id {
        return Err(ClientError::InvocationProgramMismatch);
    }
    let header_actor = Actor::new(
        program.account_id,
        lee_core::program::PROGRAM_LOADER_ACCOUNT_ID,
    );
    let header_bytes = read_actor(wallet, header_actor)
        .await
        .map_err(ClientError::ReadProgramHeader)?;
    let header = lee_core::program::ProgramHeader::from_loader_actor_state(&header_bytes)
        .ok_or(ClientError::DecodeProgramHeader)?;
    if header.image_id != program.image_id() {
        return Err(ClientError::InvocationImageMismatch);
    }
    let message = call.message_bytes()?;
    let root = call
        .actors
        .iter()
        .position(|a| *a == call.destination)
        .ok_or(ClientError::InvocationDestinationMissing)?;
    let actors = call.account_mentions();
    let hash = wallet
        .send_pub_tx_paid_by(actors, root, message, call.fee_payer)
        .await
        .map_err(|e| ClientError::SubmitPublicTransaction(Box::new(e)))?;
    wait_for_confirmation(wallet, hash).await?;
    Ok(hash)
}
async fn wait_for_confirmation(wallet: &LaunchpadWallet, hash: HashType) -> Result<()> {
    tokio::time::timeout(
        std::time::Duration::from_secs(60),
        wallet.poll_transaction(hash),
    )
    .await
    .map_err(|source| ClientError::ConfirmationTimeout { hash, source })?
    .map_err(|e| ClientError::ConfirmTransaction(e.into_boxed_dyn_error()))?;
    Ok(())
}

/// Reads a legacy NFT holding, not a launch role authority.
pub async fn load_authority_identity(
    wallet: &LaunchpadWallet,
    location: AccountLocation,
    definition: AccountId,
) -> Result<AccountId> {
    if location.is_private() {
        return Err(ClientError::UnsupportedPrivateAuthority);
    }
    let owner = location.account_id();
    let data = read_actor(
        wallet,
        token_actor(associated_token_account(owner, definition)),
    )
    .await
    .map_err(ClientError::ReadAuthorityHolding)?;
    match token::TokenState::decode(&data).map_err(ClientError::DecodeAuthorityHolding)? {
        token::TokenState::Holding {
            definition: actual,
            owner: holder,
            kind: token::TokenKind::NftMaster,
            balance: 1,
        } if actual == definition && holder == owner => Ok(definition),
        _ => Err(ClientError::InvalidAuthorityHolding),
    }
}
/// Transfers a legacy NFT, not a launch role authority.
pub async fn transfer_authority(
    wallet: &mut LaunchpadWallet,
    token_program: &ProgramHandle,
    source: AccountLocation,
    destination: AccountLocation,
    definition: AccountId,
) -> Result<HashType> {
    if source.is_private() || destination.is_private() {
        return Err(ClientError::UnsupportedPrivateAuthority);
    }
    if source == destination {
        return Err(ClientError::SameAuthoritySourceAndDestination);
    }
    load_authority_identity(wallet, source, definition).await?;
    submit_public_invocation(
        wallet,
        token_program,
        build_transfer_authority_invocation(
            source.account_id(),
            destination.account_id(),
            definition,
        ),
    )
    .await
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
    wallet: &LaunchpadWallet,
    namespace: AccountId,
    curve: AccountId,
    _collateral: AccountId,
) -> Result<AccountId> {
    // The token actor creates the canonical destination during its synchronous credit.
    Ok(load_curve_config(namespace, wallet, curve).await?.treasury)
}
pub async fn collect_pool_fees(
    wallet: &LaunchpadWallet,
    namespace: AccountId,
    curve: &ProgramHandle,
    targets: &[PoolFeeTarget],
    payer: AccountId,
) -> Result<Vec<HashType>> {
    let mut hashes = Vec::new();
    for target in targets {
        let data = read_actor(wallet, Actor::new(target.pool, curve.account_id))
            .await
            .map_err(ClientError::ReadCollectionPool)?;
        let pool =
            curve_core::PoolAccount::decode(&data).map_err(ClientError::DecodeCollectionPool)?;
        if pool.namespace != namespace || pool.token1_definition_id != target.collateral_definition
        {
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
        hashes.push(submit_public_invocation(wallet, curve, call).await?);
    }
    Ok(hashes)
}

/// Reads the block ID that a transaction in the next block observes from the canonical clock.
pub async fn load_clock_block_id(wallet: &LaunchpadWallet) -> Result<u64> {
    let data = read_actor(wallet, clock_actor())
        .await
        .map_err(ClientError::ReadClock)?;
    borsh::from_slice::<clock_core::ClockAccountData>(&data)
        .map(|clock| clock.block_id)
        .map_err(ClientError::DecodeClock)
}

/// Quotes use the saved sale rate and live namespace freeze state. Weighted pricing time is not inclusion proof.
/// Capped buys need `observed_block` from [`load_clock_block_id`]. The allowance check holds only
/// for inclusion in the next block and does not reserve allocation.
pub fn quote_lbp_trade(
    sale: LbpSale,
    pool_account: &lbp_core::PoolAccount,
    config: &Config,
    request: LbpQuoteRequest,
    pricing_time: u64,
    observed_block: Option<u64>,
) -> Result<LbpTradeQuote> {
    let addresses = lbp_addresses(sale);
    if config.namespace != sale.namespace {
        return Err(ClientError::PoolSelectionMismatch);
    }
    if pool_account.namespace != sale.namespace
        || pool_account.namespace_program_id != sale.namespace_program_id
        || pool_account.factory_owner != addresses.factory
        || pool_account.factory_program_id != sale.factory_program_id
        || pool_account.factory_seed
            != *factory_core::compute_factory_seed(sale.namespace, sale.launch_salt).as_bytes()
        || pool_account.token_definition_id != addresses.token_definition
        || pool_account.collateral_definition_id != sale.collateral_definition
    {
        return Err(ClientError::FactorySaleMismatch);
    }
    if config.emergency.is_frozen {
        return Err(ClientError::NamespaceFrozen);
    }
    let quote = lbp_core::quote_trade(pool_account, request, pricing_time)
        .map_err(ClientError::QuoteLbpTrade)?;
    if !request.is_sell() && pool_account.block_allocation.limit.is_some() {
        let block = observed_block.ok_or(ClientError::BlockIdRequired)?;
        if pool_account
            .block_allocation
            .remaining(block)
            .is_some_and(|remaining| quote.amount_out > remaining)
        {
            return Err(ClientError::QuoteLbpTrade(
                lbp_core::Error::BlockAllocationExceeded,
            ));
        }
    }
    Ok(quote)
}

/// Quotes an exact-output purchase against a snapshot of the current pool and fee config.
/// The participant supplies the cap in the purchase message.
pub fn quote_buy(
    pool_account: &curve_core::PoolAccount,
    config: &Config,
    amount_out: u128,
    now: u64,
) -> Result<pool::SwapOutcome> {
    if config.emergency.is_frozen {
        return Err(ClientError::NamespaceFrozen);
    }
    if !pool_account.funded {
        return Err(ClientError::PoolFundingPending);
    }
    if pool_account.trade_control.is_paused() {
        return Err(ClientError::PoolPaused);
    }
    if pool_account
        .allowed_input_token
        .is_some_and(|side| side != pool::TokenSide::Token1)
    {
        return Err(ClientError::InputTokenRestricted);
    }
    pool_account
        .pool
        .validate_policy()
        .map_err(|_| ClientError::QuoteBuy(pool::SwapError::InvalidPolicy))?;
    let mut pool = pool_account.pool.clone();
    pool.swap_exact_output(pool::ExactOutputSwap {
        token_in: pool::TokenSide::Token1,
        amount_out,
        max_amount_in: u128::MAX,
        pool_fee_bps: 0,
        protocol_fee_bps: config.protocol_fee_bps,
        now,
    })
    .map_err(ClientError::QuoteBuy)
}

/// Quotes an exact-input sale against a snapshot of the current pool and fee config.
/// The participant supplies the floor in the sale message.
pub fn quote_sell(
    pool_account: &curve_core::PoolAccount,
    config: &Config,
    amount_in: u128,
    now: u64,
) -> Result<pool::SwapOutcome> {
    if config.emergency.is_frozen {
        return Err(ClientError::NamespaceFrozen);
    }
    if !pool_account.funded {
        return Err(ClientError::PoolFundingPending);
    }
    if pool_account.trade_control.is_paused() {
        return Err(ClientError::PoolPaused);
    }
    if pool_account
        .allowed_input_token
        .is_some_and(|side| side != pool::TokenSide::Token0)
    {
        return Err(ClientError::InputTokenRestricted);
    }
    pool_account
        .pool
        .validate_policy()
        .map_err(|_| ClientError::QuoteSell(pool::SwapError::InvalidPolicy))?;
    let mut pool = pool_account.pool.clone();
    pool.swap_exact_input(pool::ExactInputSwap {
        token_in: pool::TokenSide::Token0,
        amount_in,
        min_amount_out: 0,
        pool_fee_bps: 0,
        protocol_fee_bps: config.protocol_fee_bps,
        now,
    })
    .map_err(ClientError::QuoteSell)
}

/// Quotes an exact-input purchase spending collateral against a pool snapshot.
pub fn quote_buy_with_collateral(
    pool_account: &curve_core::PoolAccount,
    config: &Config,
    collateral_in: u128,
    now: u64,
) -> Result<pool::SwapOutcome> {
    if config.emergency.is_frozen {
        return Err(ClientError::NamespaceFrozen);
    }
    if !pool_account.funded {
        return Err(ClientError::PoolFundingPending);
    }
    if pool_account.trade_control.is_paused() {
        return Err(ClientError::PoolPaused);
    }
    if pool_account
        .allowed_input_token
        .is_some_and(|side| side != pool::TokenSide::Token1)
    {
        return Err(ClientError::InputTokenRestricted);
    }
    pool_account
        .pool
        .validate_policy()
        .map_err(|_| ClientError::QuoteBuyWithCollateral(pool::SwapError::InvalidPolicy))?;
    let mut pool = pool_account.pool.clone();
    pool.swap_exact_input(pool::ExactInputSwap {
        token_in: pool::TokenSide::Token1,
        amount_in: collateral_in,
        min_amount_out: 0,
        pool_fee_bps: 0,
        protocol_fee_bps: config.protocol_fee_bps,
        now,
    })
    .map_err(ClientError::QuoteBuyWithCollateral)
}
