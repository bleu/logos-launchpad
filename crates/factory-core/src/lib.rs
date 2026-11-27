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

mod curve;
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

pub fn validate_curve_expiry(end: Option<u64>, now: u64) -> Result<(), Error> {
    if end.is_some_and(|end| now >= end) {
        return Err(Error::InvalidTerms);
    }
    Ok(())
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
    let expiry = match command {
        Some(Message::CreateFactoryPool {
            end_timestamp, now, ..
        }) => {
            validate_curve_expiry(end_timestamp, now)?;
            end_timestamp
        }
        Some(Message::ContinueCreation { now, .. }) => {
            let end = saved.as_ref().and_then(|state| state.end_timestamp);
            validate_curve_expiry(end, now)?;
            end
        }
        _ => None,
    };
    let mut response = handle_inner(input)?;
    if let Some(end) = expiry {
        let upper = curve_core::timestamp_millis(end).map_err(|_| Error::InvalidTerms)?;
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

    if let Some(result) = curve::resume(input) {
        return result;
    }
    if let Some(result) = lbp_setup::resume(input) {
        return result;
    }
    let message: Message = borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    if matches!(
        message,
        Message::ContinueCreation { .. }
            | Message::ContinueFixedPriceCreation { .. }
            | Message::ContinueWeightedCreation { .. }
    ) && input.from != Some(input.receiver)
    {
        return Err(Error::Unauthorized);
    }
    match message {
        command @ Message::SetPaused { .. } => {
            if LbpLaunchState::decode(&input.pre_state).is_ok() {
                lbp_setup::begin_payout(input, command)
            } else {
                curve::begin_pause(input, command)
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
        message @ Message::ClaimCreatorAllocation { .. } => curve::begin_claim(input, message),
        message @ (Message::WithdrawFactoryProceeds { .. }
        | Message::ExecuteGraduationHook { .. }) => curve::begin_settlement(input, message),
        message @ Message::ContinueCreation { .. } => curve::begin(input, message),
        message @ Message::CreateFactoryPool { .. } => curve::create(input, message),
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
