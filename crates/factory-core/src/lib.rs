//! Launch setup and mechanism-specific settlement over synchronous actor calls.
use borsh::{BorshDeserialize, BorshSerialize};
pub use curve_core::events;
use eligibility_policy::BuyPolicy;
use events::*;
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, ReceiveInput, Response, StateReply, SyncMessage},
};
use lez_compat_core::{
    authority,
    token::{self, TokenKind, TokenState},
};
use serde::{Deserialize, Serialize};

mod hook;
mod lbp_setup;
pub use curve_core::{
    AUTHORITY_PROGRAM_ACCOUNT_ID, EndingPolicy, PausePolicy, SettlementAmounts, SettlementError,
    SettlementProgress, SettlementRoute, TOKEN_PROGRAM_ACCOUNT_ID,
};
pub use hook::{
    AdapterMessage, AdapterState, GraduationIntent, GraduationIntentKind, GraduationRequest,
    GraduationResult, handle_adapter,
};
pub use lbp_core::{SettlementReceipt, SettlementReply};
pub use lbp_setup::{
    CreatorAllocationRelease, FixedPrice, FixedPriceInputs, LbpCreationStage, LbpLaunchState,
    LbpSetupMode, SaleStart, WeightedInputs,
};
pub use lbp_setup::{
    LbpCreationStage as FixedPriceCreationStage, LbpLaunchState as FixedPriceLaunchState,
};

use curve_core::SellFeeRate;
pub use curve_core::TradeMode;
pub use lbp_core::{GraduationPolicy, SettlementPayouts};
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum CreationStage {
    Minted,
    Allocated,
    PoolPrepared,
    Active,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum SettlementStage {
    Unstarted,
    Ready,
    Withdrawn { unsold: u128, collateral_due: u128 },
    HookReady { unsold: u128, collateral_due: u128 },
    Complete,
    Graduated,
}
impl SettlementStage {
    pub fn progress(self) -> SettlementProgress {
        match self {
            Self::Unstarted | Self::Ready | Self::Withdrawn { .. } | Self::HookReady { .. } => {
                SettlementProgress::Unselected
            }
            Self::Complete => SettlementProgress::ManualComplete,
            Self::Graduated => SettlementProgress::Graduated,
        }
    }
    pub fn is_manual_settlement_complete(self) -> bool {
        self.progress().is_complete()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct GraduationHook {
    pub program_id: AccountId,
    pub configuration: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FactoryState {
    pub buy_policy: BuyPolicy,
    pub pause_policy: PausePolicy,
    pub ending_policy: EndingPolicy,
    pub trade_mode: TradeMode,
    pub sell_fee_bps: u16,
    pub per_block_token_limit: Option<u128>,
    pub graduation_hook: Option<GraduationHook>,
    pub creation_stage: CreationStage,
    pub settlement_stage: SettlementStage,
    pub end_timestamp: Option<u64>,
    pub namespace: AccountId,
    pub launch_salt: [u8; 32],
    pub token_definition_id: AccountId,
    pub collateral_definition_id: AccountId,
    pub sale_reserve: u128,
    pub graduation_reserve: u128,
    pub graduation_policy: GraduationPolicy,
    pub creator_allocation: u128,
    pub total_supply: u128,
    pub virtual_token_reserve: u128,
    pub virtual_collateral_reserve: u128,
    pub curve_program_id: AccountId,
    pub creator_role_id: AccountId,
    pub name: String,
    pub uri: String,
    pub creator_escrow_id: AccountId,
    pub pool_id: AccountId,
    pub creator_allocation_claimed: bool,
    pending: Option<Pending>,
}
fn launch_event(
    namespace: AccountId,
    factory: AccountId,
    pool: AccountId,
    engine: PricingEngineV1,
    kind: SaleEventKindV1,
) -> lee_core::program::ProgramEvent {
    curve_core::sale_event(SaleEventV1::new(
        *namespace.value(),
        engine,
        EventSubjectV1::Launch {
            factory: *factory.value(),
            pool: *pool.value(),
        },
        kind,
    ))
}

impl FactoryState {
    fn event(&self, factory: AccountId, kind: SaleEventKindV1) -> lee_core::program::ProgramEvent {
        launch_event(
            self.namespace,
            factory,
            self.pool_id,
            PricingEngineV1::Curve,
            kind,
        )
    }
    fn allocation_event(
        &self,
        response: Response,
        factory: AccountId,
        creator: AccountId,
    ) -> Response {
        if self.creator_allocation == 0 || creator == self.creator_escrow_id {
            return response;
        }
        response.event(self.event(
            factory,
            SaleEventKindV1::AllocationReleased {
                definition: *self.token_definition_id.value(),
                recipient: *creator.value(),
                amount: self.creator_allocation,
                release: AllocationReleaseV1::AfterEnd,
            },
        ))
    }
    fn response(&self) -> Response {
        Response::set_state(borsh::to_vec(self).unwrap())
    }
    fn next(&self) -> Response {
        let pending = self.pending.as_ref().unwrap();
        let read = &pending.reads[pending.values.len()];
        let mut call = SyncMessage::new(read.actor, &());
        call.message = read.message.clone();
        self.response().send(call)
    }
    fn validate_terms(&self) -> Result<(), Error> {
        if total_supply(
            self.sale_reserve,
            self.graduation_reserve,
            self.creator_allocation,
        )
        .map_err(|_| Error::InvalidState)?
            != self.total_supply
        {
            return Err(Error::InvalidState);
        }
        SellFeeRate::new(self.sell_fee_bps).map_err(|_| Error::InvalidState)?;
        curve_core::BlockAllocation::new(self.per_block_token_limit)
            .map_err(|_| Error::InvalidState)?;
        self.graduation_policy
            .validate(self.graduation_hook.is_some())
            .map_err(|_| Error::InvalidState)?;
        validate_curve_policies(self.ending_policy, self.end_timestamp)
            .map_err(|_| Error::InvalidState)
    }
    /// Check canonical factory identity and stored setup terms.
    pub fn validate(&self, actor: Actor) -> Result<(), Error> {
        self.validate_terms()?;
        if actor.account_id
            != compute_factory_pda(self.namespace, actor.program_account_id, self.launch_salt)
            || self.token_definition_id
                != compute_definition_pda(
                    self.namespace,
                    actor.program_account_id,
                    self.launch_salt,
                )
            || self.creator_role_id
                != compute_curve_creator_role_pda(
                    self.namespace,
                    actor.program_account_id,
                    self.launch_salt,
                )
            || self.creator_escrow_id
                != compute_escrow_pda(self.namespace, actor.program_account_id, self.launch_salt)
            || self.pool_id
                != curve_core::compute_pool_pda(
                    self.namespace,
                    self.curve_program_id,
                    self.token_definition_id,
                    self.collateral_definition_id,
                    actor.account_id,
                )
        {
            return Err(Error::WrongAccount);
        }
        Ok(())
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let state: Self = borsh::from_slice(bytes).map_err(|_| Error::InvalidState)?;
        state.validate_terms()?;
        Ok(state)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Message {
    CreateFactoryPool {
        buy_policy: BuyPolicy,
        pause_policy: PausePolicy,
        ending_policy: EndingPolicy,
        trade_mode: u8,
        sell_fee_bps: u16,
        per_block_token_limit: Option<u128>,
        graduation_hook_program_id: Option<AccountId>,
        graduation_hook_configuration: Vec<u8>,
        namespace: AccountId,
        launch_salt: [u8; 32],
        name: String,
        uri: String,
        sale_reserve: u128,
        graduation_reserve: u128,
        graduation_policy: GraduationPolicy,
        creator_allocation: u128,
        virtual_token_reserve: u128,
        virtual_collateral_reserve: u128,
        end_timestamp: Option<u64>,
        curve_program_id: AccountId,
        collateral_definition_id: AccountId,
        creator: AccountId,
        now: u64,
    },
    ContinueCreation {
        creator: AccountId,
        now: u64,
    },
    ClaimCreatorAllocation {
        creator: AccountId,
        now: u64,
    },
    WithdrawFactoryProceeds {
        creator: AccountId,
        now: u64,
    },
    ExecuteGraduationHook {
        creator_hint: Option<AccountId>,
    },
    CreateFixedPriceLbp {
        inputs: FixedPriceInputs,
    },
    ContinueFixedPriceCreation {
        expected_stage: LbpCreationStage,
        creator: AccountId,
        now: u64,
    },
    ReadState,
    CreateWeightedLbp {
        inputs: WeightedInputs,
    },
    ContinueWeightedCreation {
        expected_stage: LbpCreationStage,
        creator: AccountId,
    },
    SetWeightedPaused {
        creator: AccountId,
        paused: bool,
    },
    WithdrawWeightedProceeds {
        creator: AccountId,
    },
    ClaimWeightedAllocation {
        creator: AccountId,
    },
    SetPaused {
        creator: AccountId,
        paused: bool,
    },
}
pub type Instruction = Message;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateError {
    InvalidTradeMode,
    SaleReserveZero,
    VirtualTokenReserveNotAboveSaleReserve,
    VirtualReserveZero,
    VirtualReserveAboveBound,
    SupplyOverflow,
    EmptyName,
    EmptyUri,
}
impl std::fmt::Display for CreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CreateError {}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidMessage,
    InvalidState,
    Unauthorized,
    WrongAccount,
    AlreadyInitialized,
    InvalidTerms,
    InvalidDefinition,
    InvalidCustody,
    StaleStage,
    CreationPending,
    NotClosed,
    AlreadyClaimed,
    AlreadySettled,
    HookNotConfigured,
    ManualPayoutStarted,
    Overflow,
    AdapterIntentMismatch,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl From<SettlementError> for Error {
    fn from(error: SettlementError) -> Self {
        match error {
            SettlementError::AlreadySettled => Self::AlreadySettled,
            SettlementError::ManualPayoutStarted => Self::ManualPayoutStarted,
            SettlementError::InvalidGraduationPolicy
            | SettlementError::AdapterRequired
            | SettlementError::RouteMismatch => Self::InvalidTerms,
            SettlementError::Arithmetic => Self::Overflow,
        }
    }
}
impl From<lez_compat_core::Error> for Error {
    fn from(_: lez_compat_core::Error) -> Self {
        Self::Unauthorized
    }
}

pub fn total_supply(sale: u128, reserve: u128, allocation: u128) -> Result<u128, CreateError> {
    if sale == 0 {
        return Err(CreateError::SaleReserveZero);
    }
    sale.checked_add(reserve)
        .and_then(|amount| amount.checked_add(allocation))
        .ok_or(CreateError::SupplyOverflow)
}
pub fn validate_curve_parameters(
    sale: u128,
    token: u128,
    collateral: u128,
) -> Result<(), CreateError> {
    if token <= sale {
        return Err(CreateError::VirtualTokenReserveNotAboveSaleReserve);
    }
    if token == 0 || collateral == 0 {
        return Err(CreateError::VirtualReserveZero);
    }
    if token >= pool::VIRTUAL_RESERVE_BOUND || collateral >= pool::VIRTUAL_RESERVE_BOUND {
        return Err(CreateError::VirtualReserveAboveBound);
    }
    Ok(())
}
fn seed(namespace: AccountId, tag: &[u8], salt: [u8; 32]) -> PdaSeed {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let mut bytes = [0; 96];
    bytes[..tag.len()].copy_from_slice(tag);
    bytes[32..64].copy_from_slice(&salt);
    bytes[64..].copy_from_slice(namespace.value());
    PdaSeed::new(Impl::hash_bytes(&bytes).as_bytes().try_into().unwrap())
}
pub fn compute_factory_seed(namespace: AccountId, salt: [u8; 32]) -> PdaSeed {
    seed(namespace, b"factory", salt)
}
pub fn compute_definition_seed(namespace: AccountId, salt: [u8; 32]) -> PdaSeed {
    seed(namespace, b"definition", salt)
}
pub fn compute_mint_seed(namespace: AccountId, salt: [u8; 32]) -> PdaSeed {
    seed(namespace, b"mint", salt)
}
pub fn compute_metadata_seed(namespace: AccountId, salt: [u8; 32]) -> PdaSeed {
    seed(namespace, b"metadata", salt)
}
pub fn compute_escrow_seed(namespace: AccountId, salt: [u8; 32]) -> PdaSeed {
    seed(namespace, b"escrow", salt)
}
pub fn compute_creator_role_seed(namespace: AccountId, salt: [u8; 32]) -> PdaSeed {
    seed(namespace, b"creator-role", salt)
}
pub fn compute_factory_pda(namespace: AccountId, program: AccountId, salt: [u8; 32]) -> AccountId {
    AccountId::for_public_pda(&program, &compute_factory_seed(namespace, salt))
}
pub fn compute_definition_pda(
    namespace: AccountId,
    program: AccountId,
    salt: [u8; 32],
) -> AccountId {
    AccountId::for_public_pda(&program, &compute_definition_seed(namespace, salt))
}
pub fn compute_mint_pda(namespace: AccountId, program: AccountId, salt: [u8; 32]) -> AccountId {
    AccountId::for_public_pda(&program, &compute_mint_seed(namespace, salt))
}
pub fn compute_metadata_pda(namespace: AccountId, program: AccountId, salt: [u8; 32]) -> AccountId {
    AccountId::for_public_pda(&program, &compute_metadata_seed(namespace, salt))
}
pub fn compute_escrow_pda(namespace: AccountId, program: AccountId, salt: [u8; 32]) -> AccountId {
    AccountId::for_public_pda(&program, &compute_escrow_seed(namespace, salt))
}
pub fn compute_creator_role_pda(
    namespace: AccountId,
    program: AccountId,
    salt: [u8; 32],
) -> AccountId {
    AccountId::for_public_pda(&program, &compute_creator_role_seed(namespace, salt))
}
pub fn compute_graduation_seed(factory: AccountId) -> PdaSeed {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let mut bytes = b"launchpad-graduation-v1".to_vec();
    bytes.extend_from_slice(factory.value());
    PdaSeed::new(Impl::hash_bytes(&bytes).as_bytes().try_into().unwrap())
}
pub fn compute_graduation_pda(adapter: AccountId, factory: AccountId) -> AccountId {
    AccountId::for_public_pda(&adapter, &compute_graduation_seed(factory))
}
pub fn compute_curve_creator_role_seed(namespace: AccountId, salt: [u8; 32]) -> PdaSeed {
    seed(namespace, b"curve-creator-role", salt)
}
pub fn compute_curve_creator_role_pda(
    namespace: AccountId,
    program: AccountId,
    salt: [u8; 32],
) -> AccountId {
    AccountId::for_public_pda(&program, &compute_curve_creator_role_seed(namespace, salt))
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct Pending {
    command: Message,
    proof: bool,
    reads: Vec<Read>,
    values: Vec<Vec<u8>>,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct Read {
    actor: Actor,
    message: Vec<u8>,
}
impl Read {
    fn new<T: BorshSerialize>(actor: Actor, message: &T) -> Self {
        Self {
            actor,
            message: borsh::to_vec(message).unwrap(),
        }
    }
}

fn holding_read(owner: AccountId, definition: AccountId) -> Read {
    Read::new(
        Actor::new(
            token::holding_address(TOKEN_PROGRAM_ACCOUNT_ID, owner, definition),
            TOKEN_PROGRAM_ACCOUNT_ID,
        ),
        &token::Message::ReadState,
    )
}
fn balance(bytes: &[u8], owner: AccountId, definition: AccountId) -> Result<u128, Error> {
    match TokenState::decode(bytes)? {
        TokenState::Holding {
            owner: stored,
            definition: stored_definition,
            kind: TokenKind::Fungible,
            balance,
        } if stored == owner && stored_definition == definition => Ok(balance),
        _ => Err(Error::InvalidCustody),
    }
}
fn transfer(
    response: Response,
    owner: AccountId,
    seed: PdaSeed,
    definition: AccountId,
    recipient: AccountId,
    amount: u128,
) -> Response {
    if amount == 0 {
        return response;
    }
    response.send(
        SyncMessage::new(
            Actor::new(owner, TOKEN_PROGRAM_ACCOUNT_ID),
            &token::Message::Transfer {
                definition,
                recipient,
                amount,
            },
        )
        .with_pda_seeds([seed]),
    )
}
fn curve_payouts(
    state: &FactoryState,
    factory: Actor,
    mut response: Response,
    creator: AccountId,
    adapter: Option<AccountId>,
    payouts: SettlementPayouts,
) -> Response {
    let mut effects = vec![];
    response = reserve_payout(
        response,
        factory,
        state.namespace,
        state.launch_salt,
        state.token_definition_id,
        adapter.unwrap_or(creator),
        state.graduation_reserve,
        &mut effects,
    );
    for (recipient, amounts, launch_purpose, collateral_purpose) in [
        (
            creator,
            payouts.creator,
            PayoutPurposeV1::CreatorLaunch,
            PayoutPurposeV1::CreatorProceeds,
        ),
        (
            adapter.unwrap_or(creator),
            payouts.adapter,
            PayoutPurposeV1::AdapterLaunch,
            PayoutPurposeV1::AdapterCollateral,
        ),
    ] {
        for (definition, amount, purpose) in [
            (
                state.token_definition_id,
                amounts.launch_tokens,
                launch_purpose,
            ),
            (
                state.collateral_definition_id,
                amounts.collateral,
                collateral_purpose,
            ),
        ] {
            if amount != 0 {
                response = transfer(
                    response,
                    factory.account_id,
                    compute_factory_seed(state.namespace, state.launch_salt),
                    definition,
                    recipient,
                    amount,
                );
                effects.push(SettlementEffectV1::Payout {
                    definition: *definition.value(),
                    recipient: *recipient.value(),
                    purpose,
                    liability_discharged: amount,
                    tokens_transferred: if recipient == factory.account_id {
                        0
                    } else {
                        amount
                    },
                });
            }
        }
    }
    response.event(state.event(
        factory.account_id,
        SaleEventKindV1::Settlement {
            step: SettlementStepV1::Payout,
            route: Some(if adapter.is_some() {
                SettlementRouteV1::Graduation
            } else {
                SettlementRouteV1::Manual
            }),
            effects,
        },
    ))
}

#[allow(clippy::too_many_arguments)]
fn reserve_payout(
    response: Response,
    factory: Actor,
    namespace: AccountId,
    salt: [u8; 32],
    definition: AccountId,
    recipient: AccountId,
    amount: u128,
    effects: &mut Vec<SettlementEffectV1>,
) -> Response {
    if amount == 0 {
        return response;
    }
    let custody = compute_mint_pda(namespace, factory.program_account_id, salt);
    effects.push(SettlementEffectV1::Payout {
        definition: *definition.value(),
        recipient: *recipient.value(),
        purpose: PayoutPurposeV1::GraduationReserve,
        liability_discharged: amount,
        tokens_transferred: if recipient == custody { 0 } else { amount },
    });
    transfer(
        response,
        custody,
        compute_mint_seed(namespace, salt),
        definition,
        recipient,
        amount,
    )
}
fn validate_window(
    ending: EndingPolicy,
    start: Option<u64>,
    end: Option<u64>,
) -> Result<(), Error> {
    ending
        .validate(start, end)
        .map_err(|_| Error::InvalidTerms)?;
    let _ = curve_core::with_time_bounds(Response::keep_state(), start, end)
        .map_err(|_| Error::InvalidTerms)?;
    Ok(())
}
pub fn validate_curve_policies(ending: EndingPolicy, end: Option<u64>) -> Result<(), Error> {
    if ending == EndingPolicy::Manual {
        return Err(Error::InvalidTerms);
    }
    validate_window(ending, None, end)
}
fn create_curve(input: &ReceiveInput, command: Message) -> Result<Response, Error> {
    let Message::CreateFactoryPool {
        buy_policy,
        pause_policy,
        ending_policy,
        trade_mode,
        sell_fee_bps,
        per_block_token_limit,
        graduation_hook_program_id,
        graduation_hook_configuration,
        namespace,
        launch_salt,
        name,
        uri,
        sale_reserve,
        graduation_reserve,
        graduation_policy,
        creator_allocation,
        virtual_token_reserve,
        virtual_collateral_reserve,
        end_timestamp,
        curve_program_id,
        collateral_definition_id,
        creator,
        now: _,
    } = command.clone()
    else {
        return Err(Error::InvalidMessage);
    };
    if input.receiver.account_id
        != compute_factory_pda(namespace, input.receiver.program_account_id, launch_salt)
    {
        return Err(Error::WrongAccount);
    }
    if !input.pre_state.is_empty() {
        return Err(Error::AlreadyInitialized);
    }
    if namespace == AccountId::default()
        || creator == AccountId::default()
        || name.is_empty()
        || uri.is_empty()
        || graduation_hook_program_id.is_none() && !graduation_hook_configuration.is_empty()
    {
        return Err(Error::InvalidTerms);
    }
    SellFeeRate::new(sell_fee_bps).map_err(|_| Error::InvalidTerms)?;
    curve_core::BlockAllocation::new(per_block_token_limit).map_err(|_| Error::InvalidTerms)?;
    validate_curve_policies(ending_policy, end_timestamp)?;
    graduation_policy.validate(graduation_hook_program_id.is_some())?;
    let total_supply = total_supply(sale_reserve, graduation_reserve, creator_allocation)
        .map_err(|_| Error::InvalidTerms)?;
    validate_curve_parameters(
        sale_reserve,
        virtual_token_reserve,
        virtual_collateral_reserve,
    )
    .map_err(|_| Error::InvalidTerms)?;
    let token_definition_id =
        compute_definition_pda(namespace, input.receiver.program_account_id, launch_salt);
    if collateral_definition_id == token_definition_id {
        return Err(Error::InvalidTerms);
    }
    let state = FactoryState {
        buy_policy,
        pause_policy,
        ending_policy,
        trade_mode: TradeMode::try_from(trade_mode).map_err(|_| Error::InvalidTerms)?,
        sell_fee_bps,
        per_block_token_limit,
        graduation_hook: graduation_hook_program_id.map(|program_id| GraduationHook {
            program_id,
            configuration: graduation_hook_configuration,
        }),
        creation_stage: CreationStage::Minted,
        settlement_stage: SettlementStage::Unstarted,
        end_timestamp,
        namespace,
        launch_salt,
        token_definition_id,
        collateral_definition_id,
        sale_reserve,
        graduation_reserve,
        graduation_policy,
        creator_allocation,
        total_supply,
        virtual_token_reserve,
        virtual_collateral_reserve,
        curve_program_id,
        creator_role_id: compute_curve_creator_role_pda(
            namespace,
            input.receiver.program_account_id,
            launch_salt,
        ),
        name,
        uri,
        creator_escrow_id: compute_escrow_pda(
            namespace,
            input.receiver.program_account_id,
            launch_salt,
        ),
        pool_id: curve_core::compute_pool_pda(
            namespace,
            curve_program_id,
            token_definition_id,
            collateral_definition_id,
            input.receiver.account_id,
        ),
        creator_allocation_claimed: false,
        pending: Some(Pending {
            command,
            proof: false,
            reads: vec![
                Read::new(
                    Actor::new(
                        curve_core::compute_config_pda(namespace, curve_program_id),
                        curve_program_id,
                    ),
                    &curve_core::Message::ReadNamespace { namespace },
                ),
                Read::new(
                    Actor::new(collateral_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                    &token::Message::ReadState,
                ),
            ],
            values: vec![],
        }),
    };
    state
        .buy_policy
        .validate_binding(state.namespace.value(), state.pool_id.value())
        .map_err(|_| Error::InvalidTerms)?;
    Ok(state.next().event(state.event(
        input.receiver.account_id,
        SaleEventKindV1::Created {
            creator: *creator.value(),
            creator_role: *state.creator_role_id.value(),
            token_definition: *state.token_definition_id.value(),
            collateral_definition: *state.collateral_definition_id.value(),
            graduation_reserve: state.graduation_reserve,
            sale_inventory: state.sale_reserve,
            creator_allocation: state.creator_allocation,
            graduation_policy: state.graduation_policy,
            collateral_seed: 0,
            start_timestamp: None,
            end_timestamp: state.end_timestamp,
            policies: EventPoliciesV1 {
                ending: state.ending_policy,
                pause: state.pause_policy,
                sell_fee_bps: state.sell_fee_bps,
            },
        },
    )))
}
fn validate_pool(
    state: &FactoryState,
    actor: Actor,
    saved: &curve_core::PoolAccount,
) -> Result<(), Error> {
    if saved.pool.ending_policy != state.ending_policy
        || saved.pool.sell_fee_bps != state.sell_fee_bps
        || saved.block_allocation.limit != state.per_block_token_limit
    {
        return Err(Error::InvalidCustody);
    }
    saved
        .pool
        .validate_policy()
        .map_err(|_| Error::InvalidState)?;
    if saved.buy_policy != state.buy_policy
        || saved.trade_control.policy() != state.pause_policy
        || saved.namespace != state.namespace
        || saved.owner != actor.account_id
        || saved.owner_program
            != Some((
                actor.program_account_id,
                *compute_factory_seed(state.namespace, state.launch_salt).as_bytes(),
            ))
        || saved.token0_definition_id != state.token_definition_id
        || saved.token1_definition_id != state.collateral_definition_id
        || saved.pool.initial_k
            != state
                .virtual_token_reserve
                .checked_mul(state.virtual_collateral_reserve)
                .ok_or(Error::Overflow)?
        || saved.pool.close_timestamp != state.end_timestamp
        || saved.pool.close_on_depletion
            != match state.ending_policy {
                EndingPolicy::Scheduled => None,
                _ => Some(pool::TokenSide::Token0),
            }
        || saved.allowed_input_token
            != match state.trade_mode {
                TradeMode::BuyOnly => Some(pool::TokenSide::Token1),
                TradeMode::BuyAndSell => None,
            }
    {
        return Err(Error::InvalidState);
    }
    Ok(())
}
fn begin_curve(input: &ReceiveInput, command: Message) -> Result<Response, Error> {
    let mut state = FactoryState::decode(&input.pre_state)?;
    state.validate(input.receiver)?;
    let Message::ContinueCreation { creator, .. } = command else {
        return Err(Error::InvalidMessage);
    };
    if state.creation_stage == CreationStage::Active {
        return Err(Error::StaleStage);
    }
    let mint = compute_mint_pda(
        state.namespace,
        input.receiver.program_account_id,
        state.launch_salt,
    );
    state.pending = Some(Pending {
        command,
        proof: true,
        reads: vec![
            Read::new(
                Actor::new(state.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                &token::Message::ReadState,
            ),
            holding_read(mint, state.token_definition_id),
            holding_read(input.receiver.account_id, state.token_definition_id),
            holding_read(state.creator_escrow_id, state.token_definition_id),
        ],
        values: vec![],
    });
    if state.creation_stage == CreationStage::PoolPrepared {
        state.pending.as_mut().unwrap().reads.extend([
            Read::new(
                Actor::new(state.pool_id, state.curve_program_id),
                &curve_core::Message::ReadState,
            ),
            holding_read(state.pool_id, state.token_definition_id),
        ]);
    }
    Ok(state.response().send_sync(
        Actor::new(creator, AUTHORITY_PROGRAM_ACCOUNT_ID),
        &authority::Message::Check {
            role: state.creator_role_id,
            callback: input.receiver,
        },
    ))
}
fn begin_pause(input: &ReceiveInput, command: Message) -> Result<Response, Error> {
    let mut state = FactoryState::decode(&input.pre_state)?;
    state.validate(input.receiver)?;
    if state.creation_stage != CreationStage::Active {
        return Err(Error::CreationPending);
    }
    let Message::SetPaused { creator, .. } = command else {
        return Err(Error::InvalidMessage);
    };
    state.pending = Some(Pending {
        command,
        proof: true,
        reads: vec![],
        values: vec![],
    });
    Ok(state.response().send_sync(
        Actor::new(creator, AUTHORITY_PROGRAM_ACCOUNT_ID),
        &authority::Message::Check {
            role: state.creator_role_id,
            callback: input.receiver,
        },
    ))
}
fn begin_claim(input: &ReceiveInput, command: Message) -> Result<Response, Error> {
    let mut state = FactoryState::decode(&input.pre_state)?;
    state.validate(input.receiver)?;
    if state.creation_stage != CreationStage::Active {
        return Err(Error::CreationPending);
    }
    if state.creator_allocation_claimed {
        return Err(Error::AlreadyClaimed);
    }
    let Message::ClaimCreatorAllocation { creator, .. } = command else {
        return Err(Error::InvalidMessage);
    };
    let mut reads = vec![Read::new(
        Actor::new(state.pool_id, state.curve_program_id),
        &curve_core::Message::ReadState,
    )];
    if state.creator_allocation != 0 {
        reads.push(holding_read(
            state.creator_escrow_id,
            state.token_definition_id,
        ))
    }
    state.pending = Some(Pending {
        command,
        proof: true,
        reads,
        values: vec![],
    });
    Ok(state.response().send_sync(
        Actor::new(creator, AUTHORITY_PROGRAM_ACCOUNT_ID),
        &authority::Message::Check {
            role: state.creator_role_id,
            callback: input.receiver,
        },
    ))
}
fn begin_settlement(input: &ReceiveInput, command: Message) -> Result<Response, Error> {
    let mut state = FactoryState::decode(&input.pre_state)?;
    state.validate(input.receiver)?;
    if state.creation_stage != CreationStage::Active {
        return Err(Error::CreationPending);
    }
    let (creator, is_hook) = match command {
        Message::WithdrawFactoryProceeds { creator, .. } => (creator, false),
        Message::ExecuteGraduationHook { creator_hint } => {
            (creator_hint.ok_or(Error::Unauthorized)?, true)
        }
        _ => return Err(Error::InvalidMessage),
    };
    if creator == AccountId::default() {
        return Err(Error::Unauthorized);
    }
    let route = if is_hook {
        SettlementRoute::Graduation
    } else {
        SettlementRoute::Manual
    };
    if is_hook && state.graduation_hook.is_none() {
        return Err(Error::HookNotConfigured);
    }
    state
        .graduation_policy
        .validate_route(route, state.graduation_hook.is_some())?;
    state.settlement_stage.progress().ensure_available(route)?;
    state.pending = Some(Pending {
        command,
        proof: true,
        reads: vec![
            Read::new(
                Actor::new(state.pool_id, state.curve_program_id),
                &curve_core::Message::ReadState,
            ),
            holding_read(input.receiver.account_id, state.token_definition_id),
            holding_read(input.receiver.account_id, state.collateral_definition_id),
        ],
        values: vec![],
    });
    if is_hook
        && matches!(
            state.settlement_stage,
            SettlementStage::Withdrawn { .. } | SettlementStage::HookReady { .. }
        )
    {
        let program = state.graduation_hook.as_ref().unwrap().program_id;
        state.pending.as_mut().unwrap().reads.push(Read::new(
            Actor::new(
                compute_graduation_pda(program, input.receiver.account_id),
                program,
            ),
            &AdapterMessage::ReadState,
        ));
    }
    let response = state
        .response()
        .send_sync(
            Actor::new(state.collateral_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
            &token::Message::Initialize {
                owner: input.receiver.account_id,
            },
        )
        .send_sync(
            Actor::new(state.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
            &token::Message::Initialize { owner: creator },
        )
        .send_sync(
            Actor::new(state.collateral_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
            &token::Message::Initialize { owner: creator },
        );
    if is_hook {
        Ok(response.send_sync(
            Actor::new(state.creator_role_id, AUTHORITY_PROGRAM_ACCOUNT_ID),
            &authority::Message::Resolve,
        ))
    } else {
        Ok(response.send_sync(
            Actor::new(creator, AUTHORITY_PROGRAM_ACCOUNT_ID),
            &authority::Message::Check {
                role: state.creator_role_id,
                callback: input.receiver,
            },
        ))
    }
}
fn curve_resume(input: &ReceiveInput) -> Option<Result<Response, Error>> {
    let mut state: FactoryState = borsh::from_slice(&input.pre_state).ok()?;
    let mut pending = state.pending.take()?;
    Some((|| {
        state.validate(input.receiver)?;
        if matches!(pending.command, Message::ExecuteGraduationHook { .. })
            && pending.reads.is_empty()
        {
            let program = state
                .graduation_hook
                .as_ref()
                .ok_or(Error::HookNotConfigured)?
                .program_id;
            if input.from
                != Some(Actor::new(
                    compute_graduation_pda(program, input.receiver.account_id),
                    program,
                ))
            {
                return Err(Error::Unauthorized);
            }
            let result: GraduationResult =
                borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
            if result.factory != input.receiver.account_id
                || pending.values.as_slice() != [result.request_hash.to_vec()]
                || !matches!(state.settlement_stage, SettlementStage::HookReady { .. })
            {
                return Err(Error::InvalidState);
            }
            let mut progress = state.settlement_stage.progress();
            progress.finish_graduation()?;
            let SettlementStage::HookReady {
                unsold,
                collateral_due,
            } = state.settlement_stage
            else {
                return Err(Error::InvalidState);
            };
            let amounts = state
                .graduation_policy
                .payouts(
                    SettlementAmounts {
                        launch_tokens: unsold,
                        collateral: collateral_due,
                    },
                    SettlementRoute::Graduation,
                    true,
                )?
                .adapter;
            state.settlement_stage = SettlementStage::Graduated;
            return Ok(state
                .response()
                .event(state.event(
                    input.receiver.account_id,
                    SaleEventKindV1::Settlement {
                        step: SettlementStepV1::Completed,
                        route: Some(SettlementRouteV1::Graduation),
                        effects: vec![],
                    },
                ))
                .event(
                    state.event(
                        input.receiver.account_id,
                        SaleEventKindV1::Graduated {
                            adapter_program: *program.value(),
                            adapter_custody: *compute_graduation_pda(
                                program,
                                input.receiver.account_id,
                            )
                            .value(),
                            launch_tokens: amounts
                                .launch_tokens
                                .checked_add(state.graduation_reserve)
                                .ok_or(Error::Overflow)?,
                            collateral: amounts.collateral,
                        },
                    ),
                ));
        }
        if pending.proof {
            let creator = match pending.command {
                Message::CreateFactoryPool { creator, .. }
                | Message::ContinueCreation { creator, .. }
                | Message::ClaimCreatorAllocation { creator, .. }
                | Message::WithdrawFactoryProceeds { creator, .. }
                | Message::SetPaused { creator, .. } => creator,
                Message::ExecuteGraduationHook { creator_hint } => {
                    creator_hint.ok_or(Error::Unauthorized)?
                }
                _ => return Err(Error::InvalidState),
            };
            let role = Actor::new(state.creator_role_id, AUTHORITY_PROGRAM_ACCOUNT_ID);
            if matches!(pending.command, Message::ExecuteGraduationHook { .. }) {
                if authority::verify_resolved(input, role)? != creator {
                    return Err(Error::Unauthorized);
                }
            } else {
                authority::verify(input, role, creator)?;
            }
            if let Message::SetPaused { paused, .. } = pending.command {
                return Ok(state.response().send(
                    SyncMessage::new(
                        Actor::new(state.pool_id, state.curve_program_id),
                        &curve_core::Message::SetPaused { paused },
                    )
                    .with_pda_seeds([compute_factory_seed(state.namespace, state.launch_salt)]),
                ));
            }
            pending.proof = false;
            state.pending = Some(pending);
            let mut response = state.response();
            if matches!(
                state.pending.as_ref().unwrap().command,
                Message::ContinueCreation { .. }
            ) {
                response = response
                    .send_sync(
                        Actor::new(state.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                        &token::Message::Initialize {
                            owner: input.receiver.account_id,
                        },
                    )
                    .send_sync(
                        Actor::new(state.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                        &token::Message::Initialize {
                            owner: state.creator_escrow_id,
                        },
                    );
            }
            response.sync_messages.extend(state.next().sync_messages);
            return Ok(response);
        }
        let expected = pending
            .reads
            .get(pending.values.len())
            .ok_or(Error::InvalidState)?;
        if input.from != Some(expected.actor) {
            return Err(Error::Unauthorized);
        }
        let reply: StateReply =
            borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
        pending.values.push(reply.state.to_vec());
        if pending.values.len() < pending.reads.len() {
            state.pending = Some(pending);
            return Ok(state.next());
        }
        match pending.command {
            Message::ClaimCreatorAllocation { creator, .. } => {
                let saved = curve_core::PoolAccount::decode(&pending.values[0])
                    .map_err(|_| Error::InvalidState)?;
                validate_pool(&state, input.receiver, &saved)?;
                if !saved.funded {
                    return Err(Error::CreationPending);
                }
                let expiry = saved
                    .pool
                    .ending_decision()
                    .map_err(|_| Error::NotClosed)?
                    .inclusion_bounds()
                    .at_least;
                if state.creator_allocation != 0
                    && balance(
                        &pending.values[1],
                        state.creator_escrow_id,
                        state.token_definition_id,
                    )? < state.creator_allocation
                {
                    return Err(Error::InvalidCustody);
                }
                state.creator_allocation_claimed = true;
                Ok(state.allocation_event(
                    transfer(
                        curve_core::with_time_bounds(state.response(), expiry, None)
                            .map_err(|_| Error::InvalidTerms)?,
                        state.creator_escrow_id,
                        compute_escrow_seed(state.namespace, state.launch_salt),
                        state.token_definition_id,
                        creator,
                        state.creator_allocation,
                    ),
                    input.receiver.account_id,
                    creator,
                ))
            }
            command @ (Message::WithdrawFactoryProceeds { .. }
            | Message::ExecuteGraduationHook { .. }) => {
                let (now, creator, route) = match command {
                    Message::WithdrawFactoryProceeds { now, creator } => {
                        (now, creator, SettlementRoute::Manual)
                    }
                    Message::ExecuteGraduationHook { creator_hint } => (
                        0,
                        creator_hint.ok_or(Error::Unauthorized)?,
                        SettlementRoute::Graduation,
                    ),
                    _ => unreachable!(),
                };
                state
                    .graduation_policy
                    .validate_route(route, state.graduation_hook.is_some())?;
                let saved = curve_core::PoolAccount::decode(&pending.values[0])
                    .map_err(|_| Error::InvalidState)?;
                validate_pool(&state, input.receiver, &saved)?;
                if !saved.funded {
                    return Err(Error::CreationPending);
                }
                let expiry = saved
                    .pool
                    .ending_decision()
                    .map_err(|_| Error::NotClosed)?
                    .inclusion_bounds()
                    .at_least;
                let launch_balance = balance(
                    &pending.values[1],
                    input.receiver.account_id,
                    state.token_definition_id,
                )?;
                let collateral_balance = balance(
                    &pending.values[2],
                    input.receiver.account_id,
                    state.collateral_definition_id,
                )?;
                let (unsold, collateral_due) = match state.settlement_stage {
                    SettlementStage::Unstarted => {
                        state.settlement_stage = SettlementStage::Ready;
                        return Ok(curve_core::with_time_bounds(state.response(), expiry, None)
                            .map_err(|_| Error::InvalidTerms)?
                            .event(state.event(
                                input.receiver.account_id,
                                SaleEventKindV1::Settlement {
                                    step: SettlementStepV1::Prepared,
                                    route: None,
                                    effects: vec![],
                                },
                            )));
                    }
                    SettlementStage::Ready => {
                        state.settlement_stage = SettlementStage::Withdrawn {
                            unsold: saved.pool.real_reserve0,
                            collateral_due: saved.pool.real_reserve1,
                        };
                        return Ok(curve_core::with_time_bounds(state.response(), expiry, None)
                            .map_err(|_| Error::InvalidTerms)?
                            .send_sync(
                                Actor::new(state.pool_id, state.curve_program_id),
                                &curve_core::Message::WithdrawReserves { now },
                            ));
                    }
                    SettlementStage::Withdrawn {
                        unsold,
                        collateral_due,
                    }
                    | SettlementStage::HookReady {
                        unsold,
                        collateral_due,
                    } => (unsold, collateral_due),
                    _ => return Err(Error::AlreadySettled),
                };
                if saved.pool.real_reserve0 != 0
                    || saved.pool.real_reserve1 != 0
                    || launch_balance < unsold
                    || collateral_balance < collateral_due
                {
                    return Err(Error::InvalidCustody);
                }
                let eligible = SettlementAmounts {
                    launch_tokens: unsold,
                    collateral: collateral_due,
                };
                let payouts = state.graduation_policy.payouts(
                    eligible,
                    route,
                    state.graduation_hook.is_some(),
                )?;
                if route == SettlementRoute::Manual {
                    state.settlement_stage = SettlementStage::Complete;
                    let response = curve_core::with_time_bounds(state.response(), expiry, None)
                        .map_err(|_| Error::InvalidTerms)?;
                    return Ok(curve_payouts(
                        &state,
                        input.receiver,
                        response,
                        creator,
                        None,
                        payouts,
                    )
                    .event(state.event(
                        input.receiver.account_id,
                        SaleEventKindV1::Settlement {
                            step: SettlementStepV1::Completed,
                            route: Some(SettlementRouteV1::Manual),
                            effects: vec![],
                        },
                    )));
                }
                let hook = state
                    .graduation_hook
                    .as_ref()
                    .ok_or(Error::HookNotConfigured)?;
                let program = hook.program_id;
                let custody = compute_graduation_pda(program, input.receiver.account_id);
                if [
                    input.receiver.account_id,
                    state.pool_id,
                    state.creator_escrow_id,
                    state.token_definition_id,
                    state.collateral_definition_id,
                    creator,
                ]
                .contains(&custody)
                {
                    return Err(Error::InvalidCustody);
                }
                let adapter = AdapterState::decode(&pending.values[3])?;
                if adapter.factory != input.receiver || adapter.receipt.is_some() {
                    return Err(Error::InvalidCustody);
                }
                if adapter.accepted_intent != GraduationIntentKind::Curve {
                    return Err(Error::AdapterIntentMismatch);
                }
                if matches!(state.settlement_stage, SettlementStage::Withdrawn { .. }) {
                    state.settlement_stage = SettlementStage::HookReady {
                        unsold,
                        collateral_due,
                    };
                    return Ok(curve_core::with_time_bounds(state.response(), expiry, None)
                        .map_err(|_| Error::InvalidTerms)?
                        .event(state.event(
                            input.receiver.account_id,
                            SaleEventKindV1::Settlement {
                                step: SettlementStepV1::Prepared,
                                route: None,
                                effects: vec![],
                            },
                        ))
                        .send_sync(
                            Actor::new(state.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                            &token::Message::Initialize { owner: custody },
                        )
                        .send_sync(
                            Actor::new(state.collateral_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                            &token::Message::Initialize { owner: custody },
                        ));
                }
                let request = GraduationRequest {
                    factory_program_id: input.receiver.program_account_id,
                    namespace: state.namespace,
                    launch_salt: state.launch_salt,
                    token_definition_id: state.token_definition_id,
                    collateral_definition_id: state.collateral_definition_id,
                    intent: GraduationIntent::Curve {
                        launch_tokens: payouts
                            .adapter
                            .launch_tokens
                            .checked_add(state.graduation_reserve)
                            .ok_or(Error::Overflow)?,
                        collateral: payouts.adapter.collateral,
                    },
                    reserve_tokens: state.graduation_reserve,
                    unsold_tokens: payouts.adapter.launch_tokens,
                    configuration: hook.configuration.clone(),
                };
                state.pending = Some(Pending {
                    command: Message::ExecuteGraduationHook {
                        creator_hint: Some(creator),
                    },
                    proof: false,
                    reads: vec![],
                    values: vec![request.hash().to_vec()],
                });
                let response = curve_core::with_time_bounds(state.response(), expiry, None)
                    .map_err(|_| Error::InvalidTerms)?;
                // The adapter receives no factory PDA grant.
                Ok(curve_payouts(
                    &state,
                    input.receiver,
                    response,
                    creator,
                    Some(custody),
                    payouts,
                )
                .send_sync(
                    Actor::new(custody, program),
                    &AdapterMessage::Graduate { request },
                ))
            }
            Message::ContinueCreation { now, .. } => {
                if TokenState::decode(&pending.values[0])?
                    != (TokenState::Definition {
                        name: state.name.clone(),
                        kind: TokenKind::Fungible,
                        supply: state.total_supply,
                        metadata: Some(token::Metadata {
                            standard: token::MetadataStandard::Simple,
                            uri: state.uri.clone(),
                            creators: String::new(),
                        }),
                    })
                {
                    return Err(Error::InvalidDefinition);
                }
                let mint = compute_mint_pda(
                    state.namespace,
                    input.receiver.program_account_id,
                    state.launch_salt,
                );
                if state.creation_stage == CreationStage::PoolPrepared {
                    if balance(&pending.values[1], mint, state.token_definition_id)?
                        != state.graduation_reserve
                        || balance(
                            &pending.values[2],
                            input.receiver.account_id,
                            state.token_definition_id,
                        )? != state.sale_reserve
                        || balance(
                            &pending.values[3],
                            state.creator_escrow_id,
                            state.token_definition_id,
                        )? != state.creator_allocation
                    {
                        return Err(Error::InvalidCustody);
                    }
                    let saved = curve_core::PoolAccount::decode(&pending.values[4])
                        .map_err(|_| Error::InvalidState)?;
                    validate_pool(&state, input.receiver, &saved)?;
                    if saved.funded
                        || saved.pool.k != saved.pool.initial_k
                        || saved.pool.virtual_reserve0 != state.virtual_token_reserve
                        || saved.pool.virtual_reserve1 != state.virtual_collateral_reserve
                        || saved.pool.real_reserve0 != state.sale_reserve
                        || saved.pool.real_reserve1 != 0
                        || balance(&pending.values[5], state.pool_id, state.token_definition_id)?
                            != 0
                    {
                        return Err(Error::InvalidCustody);
                    }
                    state.creation_stage = CreationStage::Active;
                    return Ok(transfer(
                        state.response(),
                        input.receiver.account_id,
                        compute_factory_seed(state.namespace, state.launch_salt),
                        state.token_definition_id,
                        state.pool_id,
                        state.sale_reserve,
                    )
                    .send_sync(
                        Actor::new(state.pool_id, state.curve_program_id),
                        &curve_core::Message::ActivatePool { now },
                    ));
                }
                if state.creation_stage == CreationStage::Allocated {
                    if balance(&pending.values[1], mint, state.token_definition_id)?
                        != state.graduation_reserve
                        || balance(
                            &pending.values[2],
                            input.receiver.account_id,
                            state.token_definition_id,
                        )? != state.sale_reserve
                        || balance(
                            &pending.values[3],
                            state.creator_escrow_id,
                            state.token_definition_id,
                        )? != state.creator_allocation
                    {
                        return Err(Error::InvalidCustody);
                    }
                    state.creation_stage = CreationStage::PoolPrepared;
                    return Ok(state.response().send(
                        SyncMessage::new(
                            Actor::new(input.receiver.account_id, state.curve_program_id),
                            &curve_core::Message::CreatePool {
                                buy_policy: state.buy_policy.clone(),
                                pause_policy: state.pause_policy,
                                ending_policy: state.ending_policy,
                                sell_fee_bps: state.sell_fee_bps,
                                per_block_token_limit: state.per_block_token_limit,
                                namespace: state.namespace,
                                token0_definition_id: state.token_definition_id,
                                token1_definition_id: state.collateral_definition_id,
                                allowed_input_token: match state.trade_mode {
                                    TradeMode::BuyOnly => Some(1),
                                    TradeMode::BuyAndSell => None,
                                },
                                defer_funding: true,
                                token0_amount: state.sale_reserve,
                                token1_amount: 0,
                                virtual_reserve0: state.virtual_token_reserve,
                                virtual_reserve1: state.virtual_collateral_reserve,
                                close_timestamp: state.end_timestamp,
                                close_on_depletion: match state.ending_policy {
                                    EndingPolicy::Scheduled => None,
                                    _ => Some(0),
                                },
                                owner: input.receiver.account_id,
                                owner_program_id: Some(input.receiver.program_account_id),
                                owner_program_seed: Some(
                                    *compute_factory_seed(state.namespace, state.launch_salt)
                                        .as_bytes(),
                                ),
                                curve_program_id: state.curve_program_id,
                                now,
                            },
                        )
                        .with_pda_seeds([compute_factory_seed(state.namespace, state.launch_salt)]),
                    ));
                }
                if state.creation_stage != CreationStage::Minted {
                    return Err(Error::StaleStage);
                }
                if balance(&pending.values[1], mint, state.token_definition_id)?
                    != state.total_supply
                    || balance(
                        &pending.values[2],
                        input.receiver.account_id,
                        state.token_definition_id,
                    )? != 0
                    || balance(
                        &pending.values[3],
                        state.creator_escrow_id,
                        state.token_definition_id,
                    )? != 0
                {
                    return Err(Error::InvalidCustody);
                }
                state.creation_stage = CreationStage::Allocated;
                let response = transfer(
                    state.response(),
                    mint,
                    compute_mint_seed(state.namespace, state.launch_salt),
                    state.token_definition_id,
                    input.receiver.account_id,
                    state.sale_reserve,
                );
                Ok(transfer(
                    response,
                    mint,
                    compute_mint_seed(state.namespace, state.launch_salt),
                    state.token_definition_id,
                    state.creator_escrow_id,
                    state.creator_allocation,
                ))
            }
            Message::CreateFactoryPool { creator, .. } => {
                let config = curve_core::Config::decode(&pending.values[0])
                    .map_err(|_| Error::InvalidState)?;
                if config.namespace != state.namespace {
                    return Err(Error::WrongAccount);
                }
                if !matches!(
                    TokenState::decode(&pending.values[1])?,
                    TokenState::Definition {
                        kind: TokenKind::Fungible,
                        ..
                    }
                ) {
                    return Err(Error::InvalidDefinition);
                }
                let mint = token::Message::Create {
                    name: state.name.clone(),
                    kind: TokenKind::Fungible,
                    supply: state.total_supply,
                    recipient: compute_mint_pda(
                        state.namespace,
                        input.receiver.program_account_id,
                        state.launch_salt,
                    ),
                    metadata: Some(token::Metadata {
                        standard: token::MetadataStandard::Simple,
                        uri: state.uri.clone(),
                        creators: String::new(),
                    }),
                };
                let response =
                    curve_core::with_time_bounds(state.response(), None, state.end_timestamp)
                        .map_err(|_| Error::InvalidTerms)?;
                Ok(response
                    .send(
                        SyncMessage::new(
                            Actor::new(state.creator_role_id, AUTHORITY_PROGRAM_ACCOUNT_ID),
                            &authority::Message::BootstrapTransferOnly { authority: creator },
                        )
                        .with_pda_seeds([compute_curve_creator_role_seed(
                            state.namespace,
                            state.launch_salt,
                        )]),
                    )
                    .send(
                        SyncMessage::new(
                            Actor::new(state.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                            &mint,
                        )
                        .with_pda_seeds([compute_definition_seed(
                            state.namespace,
                            state.launch_salt,
                        )]),
                    ))
            }
            _ => Err(Error::InvalidMessage),
        }
    })())
}
/// Operational floor for configured curve expiry. Private-path fairness still needs measurements.
pub const MIN_CURVE_SALE_DURATION_SECONDS: u64 = 300;

pub fn validate_curve_duration(end: Option<u64>, now: u64) -> Result<(), Error> {
    if end.is_some_and(|end| {
        end.checked_sub(now)
            .is_none_or(|duration| duration < MIN_CURVE_SALE_DURATION_SECONDS)
    }) {
        return Err(Error::InvalidTerms);
    }
    Ok(())
}

pub fn validate_curve_continuation_duration(end: Option<u64>, now: u64) -> Result<(), Error> {
    if end.is_some_and(|end| now >= end) {
        return Ok(());
    }
    validate_curve_duration(end, now)
}

pub fn handle(input: &ReceiveInput) -> Result<Response, Error> {
    let saved = FactoryState::decode(&input.pre_state).ok();
    let command = saved
        .as_ref()
        .and_then(|state| {
            state
                .pending
                .as_ref()
                .map(|pending| pending.command.clone())
        })
        .or_else(|| borsh::from_slice::<Message>(&input.message).ok());
    let (expiry, recovery) = match command {
        Some(Message::CreateFactoryPool {
            end_timestamp, now, ..
        }) => {
            validate_curve_duration(end_timestamp, now)?;
            (end_timestamp, false)
        }
        Some(Message::ContinueCreation { now, .. }) => {
            let end = saved.as_ref().ok_or(Error::InvalidState)?.end_timestamp;
            validate_curve_continuation_duration(end, now)?;
            (end, end.is_some_and(|end| now >= end))
        }
        _ => (None, false),
    };
    let mut response = handle_inner(input)?;
    if let Some(end) = expiry {
        if recovery {
            let lower = curve_core::timestamp_millis(end).map_err(|_| Error::InvalidTerms)?;
            let lower = response
                .validity
                .timestamps
                .start()
                .map_or(lower, |existing| existing.max(lower));
            response = match response.validity.timestamps.end() {
                Some(upper) => response
                    .try_timestamp_window(lower..upper)
                    .map_err(|_| Error::InvalidTerms)?,
                None => response.timestamp_window(lower..),
            };
            return Ok(response);
        }
        // Inclusion time, not the caller's hint, limits every setup callback and activation.
        let latest = end
            .checked_sub(MIN_CURVE_SALE_DURATION_SECONDS)
            .ok_or(Error::InvalidTerms)?;
        let upper = curve_core::timestamp_millis(latest)
            .map_err(|_| Error::InvalidTerms)?
            .checked_add(1)
            .ok_or(Error::InvalidTerms)?;
        let upper = response
            .validity
            .timestamps
            .end()
            .map_or(upper, |existing| existing.min(upper));
        response = match response.validity.timestamps.start() {
            Some(start) => response
                .try_timestamp_window(start..upper)
                .map_err(|_| Error::InvalidTerms)?,
            None => response.timestamp_window(..upper),
        };
    }
    Ok(response)
}

fn handle_inner(input: &ReceiveInput) -> Result<Response, Error> {
    if let Some(result) = lez_compat_core::clock::resume(input) {
        return result.map_err(|error| match error {
            lez_compat_core::Error::Unauthorized => Error::Unauthorized,
            lez_compat_core::Error::InvalidMessage => Error::InvalidMessage,
            lez_compat_core::Error::OutsideTimeBounds => Error::InvalidTerms,
            _ => Error::InvalidState,
        });
    }

    if let Some(result) = curve_resume(input) {
        return result;
    }
    if let Some(result) = lbp_setup::resume(input) {
        return result;
    }
    let message: Message = borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    match message {
        command @ Message::SetPaused { .. } => {
            if LbpLaunchState::decode(&input.pre_state).is_ok() {
                lbp_setup::begin_payout(input, command)
            } else {
                begin_pause(input, command)
            }
        }
        message @ Message::ExecuteGraduationHook { .. }
            if LbpLaunchState::decode(&input.pre_state).is_ok() =>
        {
            lbp_setup::begin_hook(input, message)
        }
        message @ (Message::ClaimCreatorAllocation { .. }
        | Message::WithdrawFactoryProceeds { .. })
            if FixedPriceLaunchState::decode(&input.pre_state).is_ok() =>
        {
            lbp_setup::begin_payout(input, message)
        }
        message @ Message::ClaimCreatorAllocation { .. } => begin_claim(input, message),
        message @ (Message::WithdrawFactoryProceeds { .. }
        | Message::ExecuteGraduationHook { .. }) => begin_settlement(input, message),
        message @ Message::ContinueCreation { .. } => begin_curve(input, message),
        message @ Message::CreateFactoryPool { .. } => create_curve(input, message),
        Message::ReadState => {
            if let Ok(state) = LbpLaunchState::decode(&input.pre_state) {
                if input.receiver.account_id
                    != compute_factory_pda(
                        state.namespace,
                        input.receiver.program_account_id,
                        state.launch_salt,
                    )
                {
                    return Err(Error::WrongAccount);
                }
            } else {
                FactoryState::decode(&input.pre_state)?.validate(input.receiver)?;
            }
            Ok(Response::keep_state().send_sync(
                input.from.ok_or(Error::Unauthorized)?,
                &StateReply::from(input),
            ))
        }
        Message::ContinueFixedPriceCreation {
            expected_stage,
            creator,
            now,
        } => lbp_setup::begin(input, expected_stage, creator, now, false),
        Message::CreateFixedPriceLbp { inputs } => lbp_setup::create_fixed(input, inputs),
        command @ (Message::SetWeightedPaused { .. }
        | Message::WithdrawWeightedProceeds { .. }
        | Message::ClaimWeightedAllocation { .. }) => lbp_setup::begin_payout(input, command),
        Message::CreateWeightedLbp { inputs } => lbp_setup::create_weighted(input, inputs),
        Message::ContinueWeightedCreation {
            expected_stage,
            creator,
        } => lbp_setup::begin(input, expected_stage, creator, 0, true),
    }
}
