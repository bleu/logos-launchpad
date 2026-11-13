//! Bounded curve actor and namespace administration.

use borsh::{BorshDeserialize, BorshSerialize};
pub use eligibility_policy::{AllowlistPolicy, BuyPolicy, MembershipPath};
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, ReceiveInput, Response, SyncMessage},
};
pub use sale_policy::events;
pub use sale_policy::{
    EndingDecision, EndingError, EndingPolicy, EndingReason, PausePolicy, SettlementAmounts,
    SettlementError, SettlementProgress, SettlementRoute, TimeBounds, TradeControl, TradeMode,
};

include!(concat!(env!("OUT_DIR"), "/emergency_authority.rs"));

pub const MAX_FEE_BPS: u16 = 10_000;
pub const TOKEN_PROGRAM_ACCOUNT_ID: AccountId = AccountId::new([
    227, 49, 42, 31, 136, 159, 220, 198, 195, 186, 3, 242, 23, 1, 47, 213, 204, 150, 77, 104, 203,
    183, 207, 146, 116, 205, 6, 145, 114, 161, 89, 113,
]);
pub const AUTHORITY_PROGRAM_ACCOUNT_ID: AccountId = AccountId::new([
    94, 144, 155, 106, 14, 146, 62, 120, 135, 16, 55, 144, 76, 89, 251, 81, 15, 154, 235, 51, 72,
    22, 124, 218, 6, 52, 198, 24, 253, 158, 192, 243,
]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidMessage,
    InvalidState,
    Unauthorized,
    WrongAccount,
    AlreadyInitialized,
    InvalidFee,
    InvalidTreasury,
    InvalidPool,
    InvalidToken,
    Underfunded,
    PendingFunding,
    Frozen,
    Arithmetic,
    Restricted,
    Closed,
    PauseDisabled,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Message {
    ReadState,
    WithdrawReserves {
        now: u64,
    },
    ClosePool {
        now: u64,
    },
    CollectFees,
    SwapExactOutput {
        amount_out: u128,
        max_amount_in: u128,
        token_in: AccountId,
        membership: Option<MembershipPath>,
        now: u64,
    },
    SwapExactInput {
        amount_in: u128,
        min_amount_out: u128,
        token_in: AccountId,
        membership: Option<MembershipPath>,
        now: u64,
    },
    ActivatePool {
        now: u64,
    },
    Execute {
        pool: AccountId,
        instruction: Box<Message>,
    },
    CreatePool {
        buy_policy: BuyPolicy,
        pause_policy: PausePolicy,
        ending_policy: EndingPolicy,
        namespace: AccountId,
        token0_definition_id: AccountId,
        token1_definition_id: AccountId,
        allowed_input_token: Option<u8>,
        defer_funding: bool,
        token0_amount: u128,
        token1_amount: u128,
        virtual_reserve0: u128,
        virtual_reserve1: u128,
        close_timestamp: Option<u64>,
        close_on_depletion: Option<u8>,
        owner: AccountId,
        owner_program_id: Option<AccountId>,
        owner_program_seed: Option<[u8; 32]>,
        curve_program_id: AccountId,
        now: u64,
    },
    TransferEmergencyAuthority {
        namespace: AccountId,
        new_authority: AccountId,
    },
    SetFrozen {
        namespace: AccountId,
        frozen: bool,
    },
    RenounceAdmin {
        namespace: AccountId,
    },
    ReadNamespace {
        namespace: AccountId,
    },
    TransferAdmin {
        namespace: AccountId,
        new_admin: AccountId,
        candidate: AdminCandidate,
    },
    UpdateTreasury {
        namespace: AccountId,
        treasury: AccountId,
    },
    UpdateFee {
        namespace: AccountId,
        protocol_fee_bps: u32,
    },
    InitializeNamespace {
        namespace: AccountId,
        protocol_fee_bps: u32,
        treasury: AccountId,
    },
    SetPaused {
        paused: bool,
    },
}
pub type Instruction = Message;
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AdminCandidate {
    Signer,
    Pda {
        program_id: AccountId,
        seed: [u8; 32],
    },
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FreezeConfig {
    holder: AccountId,
    pub is_frozen: bool,
}
impl FreezeConfig {
    pub fn authority(&self) -> AccountId {
        self.holder
    }
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Config {
    format: [u8; 8],
    version: u16,
    pub namespace: AccountId,
    pub protocol_fee_bps: u16,
    pub treasury: AccountId,
    pub emergency: FreezeConfig,
}
impl Config {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let state: Self = borsh::from_slice(bytes).map_err(|_| Error::InvalidState)?;
        if state.format != *b"LZNSCONF" || state.version != 3 {
            return Err(Error::InvalidState);
        }
        Ok(state)
    }
    pub fn format_version(&self) -> u16 {
        self.version
    }
}

/// Convert public Unix seconds to LEZ milliseconds.
pub fn timestamp_millis(seconds: u64) -> Result<u64, Error> {
    seconds.checked_mul(1000).ok_or(Error::Arithmetic)
}

/// Restrict inclusion time without replacing an existing response window.
pub fn with_time_bounds(
    mut response: Response,
    at_least: Option<u64>,
    before: Option<u64>,
) -> Result<Response, Error> {
    let bounds = (
        at_least.map(timestamp_millis).transpose()?,
        before.map(timestamp_millis).transpose()?,
    );
    let window = bounds.try_into().map_err(|_| Error::Closed)?;
    response.validity.timestamps = response
        .validity
        .timestamps
        .intersect(window)
        .map_err(|_| Error::Closed)?;
    Ok(response)
}

/// Check a saved-clock interval for weighted-sale pricing.
pub fn clock_assertion(at_least: u64, before: Option<u64>) -> Result<clock_core::Message, Error> {
    let at_least = timestamp_millis(at_least)?;
    let at_most = match before {
        Some(end) => timestamp_millis(end)?
            .checked_sub(1)
            .ok_or(Error::Arithmetic)?,
        None => u64::MAX,
    };
    if at_least > at_most {
        return Err(Error::Closed);
    }
    Ok(clock_core::Message::AssertTimestamp { at_least, at_most })
}

/// Intersect inclusion-time bounds in LEZ milliseconds with the response window.
pub fn with_inclusion_bounds(
    mut response: Response,
    at_least: Option<u64>,
    before: Option<u64>,
) -> Result<Response, Error> {
    let window = (at_least, before).try_into().map_err(|_| Error::Closed)?;
    response.validity.timestamps = response
        .validity
        .timestamps
        .intersect(window)
        .map_err(|_| Error::Closed)?;
    Ok(response)
}

pub fn compute_config_pda_seed(namespace: AccountId) -> PdaSeed {
    PdaSeed::new(*namespace.value())
}
pub fn compute_config_pda(namespace: AccountId, program: AccountId) -> AccountId {
    AccountId::for_public_pda(&program, &compute_config_pda_seed(namespace))
}
pub fn compute_admin_role_seed(namespace: AccountId) -> PdaSeed {
    let domain = AccountId::from_builtin_program_name(b"launchpad/namespace-admin/v1");
    PdaSeed::new(*AccountId::for_public_pda(&domain, &compute_config_pda_seed(namespace)).value())
}
pub fn compute_admin_role(namespace: AccountId, program: AccountId) -> AccountId {
    AccountId::for_public_pda(&program, &compute_admin_role_seed(namespace))
}

pub fn compute_pool_owner_role_seed(namespace: AccountId, pool: AccountId) -> PdaSeed {
    let domain = AccountId::from_builtin_program_name(b"launchpad/curve-pool-owner/v1");
    let namespace_domain = AccountId::for_public_pda(&domain, &PdaSeed::new(*namespace.value()));
    PdaSeed::new(
        *AccountId::for_public_pda(&namespace_domain, &PdaSeed::new(*pool.value())).value(),
    )
}
pub fn compute_pool_owner_role(
    namespace: AccountId,
    pool: AccountId,
    program: AccountId,
) -> AccountId {
    AccountId::for_public_pda(&program, &compute_pool_owner_role_seed(namespace, pool))
}

mod authority;
mod dispatch;
mod emergency;
mod program_events;
pub use program_events::sale_event;
mod pool_create;
mod pool_lifecycle;
mod pool_swap;
mod update_config;

pub fn handle(input: &ReceiveInput) -> Result<Response, Error> {
    if let Some(result) = update_config::resume(input) {
        return result;
    }
    if let Some(result) = dispatch::resume(input) {
        return result;
    }
    let message: Message = borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    let program = input.receiver.program_account_id;
    match message {
        Message::ReadState => {
            let state = PoolAccount::decode(&input.pre_state)?;
            state.validate_address(input.receiver)?;
            Ok(Response::keep_state().send_sync(
                input.from.ok_or(Error::Unauthorized)?,
                &lee_core::program::StateReply::from(input),
            ))
        }
        Message::Execute { pool, instruction } => {
            if !input.is_authorized
                || !matches!(
                    *instruction,
                    Message::CreatePool { .. }
                        | Message::ActivatePool { .. }
                        | Message::WithdrawReserves { .. }
                        | Message::ClosePool { .. }
                        | Message::SetPaused { .. }
                        | Message::SwapExactInput { .. }
                        | Message::SwapExactOutput { .. }
                )
            {
                return Err(Error::Unauthorized);
            }
            let mut call = SyncMessage::new(Actor::new(pool, program), &*instruction);
            if let Message::CreatePool {
                namespace,
                token0_definition_id,
                token1_definition_id,
                owner,
                ..
            } = *instruction
            {
                call = call.with_pda_seeds([compute_pool_pda_seed(
                    namespace,
                    token0_definition_id,
                    token1_definition_id,
                    owner,
                )]);
            }
            Ok(Response::keep_state().send(call))
        }
        Message::WithdrawReserves { .. }
        | Message::ClosePool { .. }
        | Message::CollectFees
        | Message::SetPaused { .. } => pool_lifecycle::begin(input, message),
        Message::SwapExactInput { .. } | Message::SwapExactOutput { .. } => {
            pool_swap::begin(input, message)
        }
        Message::ActivatePool { .. } => pool_create::activate(input, message),
        Message::CreatePool { .. } => pool_create::begin(input, message),
        Message::TransferEmergencyAuthority { .. } | Message::SetFrozen { .. } => {
            emergency::handle(input, message)
        }
        Message::ReadNamespace { namespace } => {
            if input.receiver.account_id != compute_config_pda(namespace, program) {
                return Err(Error::WrongAccount);
            }
            let config = Config::decode(&input.pre_state)?;
            if config.namespace != namespace {
                return Err(Error::WrongAccount);
            }
            Ok(Response::keep_state().send_sync(
                input.from.ok_or(Error::Unauthorized)?,
                &lee_core::program::StateReply::from(input),
            ))
        }
        Message::RenounceAdmin { .. }
        | Message::TransferAdmin { .. }
        | Message::UpdateFee { .. }
        | Message::UpdateTreasury { .. } => update_config::begin(input, message),
        Message::InitializeNamespace {
            namespace,
            protocol_fee_bps,
            treasury,
        } => {
            if namespace == AccountId::default() {
                return Err(Error::Unauthorized);
            }
            if protocol_fee_bps >= u32::from(MAX_FEE_BPS) {
                return Err(Error::InvalidFee);
            }
            if treasury == AccountId::default() {
                return Err(Error::InvalidTreasury);
            }
            let config = Actor::new(compute_config_pda(namespace, program), program);
            if input.receiver.account_id == namespace {
                if !input.is_authorized {
                    return Err(Error::Unauthorized);
                }
                return Ok(Response::keep_state().send(
                    SyncMessage::new(config, &message)
                        .with_pda_seeds([compute_config_pda_seed(namespace)]),
                ));
            }
            if input.receiver != config {
                return Err(Error::WrongAccount);
            }
            if input.from != Some(Actor::new(namespace, program)) {
                return Err(Error::Unauthorized);
            }
            if !input.pre_state.is_empty() {
                return Err(Error::AlreadyInitialized);
            }
            let state = Config {
                format: *b"LZNSCONF",
                version: 3,
                namespace,
                protocol_fee_bps: protocol_fee_bps as u16,
                treasury,
                emergency: FreezeConfig {
                    holder: INITIAL_EMERGENCY_AUTHORITY,
                    is_frozen: false,
                },
            };
            Ok(Response::set_state(borsh::to_vec(&state).unwrap()).send(
                SyncMessage::new(
                    Actor::new(
                        compute_admin_role(namespace, program),
                        AUTHORITY_PROGRAM_ACCOUNT_ID,
                    ),
                    &lez_compat_core::authority::Message::Bootstrap {
                        authority: namespace,
                    },
                )
                .with_pda_seeds([compute_admin_role_seed(namespace)]),
            ))
        }
    }
}

pub fn compute_pool_pda_seed(
    namespace: AccountId,
    token0: AccountId,
    token1: AccountId,
    owner: AccountId,
) -> PdaSeed {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let mut bytes = [0; 128];
    bytes[..32].copy_from_slice(token0.value());
    bytes[32..64].copy_from_slice(token1.value());
    bytes[64..96].copy_from_slice(owner.value());
    bytes[96..].copy_from_slice(namespace.value());
    PdaSeed::new(Impl::hash_bytes(&bytes).as_bytes().try_into().unwrap())
}
pub fn compute_pool_pda(
    namespace: AccountId,
    program: AccountId,
    token0: AccountId,
    token1: AccountId,
    owner: AccountId,
) -> AccountId {
    AccountId::for_public_pda(
        &program,
        &compute_pool_pda_seed(namespace, token0, token1, owner),
    )
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PoolAccount {
    pub buy_policy: BuyPolicy,
    pub trade_control: TradeControl,
    pub allowed_input_token: Option<pool::TokenSide>,
    pub funded: bool,
    pub closure_observed: bool,
    pub namespace: AccountId,
    pub token0_definition_id: AccountId,
    pub token1_definition_id: AccountId,
    pub owner: AccountId,
    pub owner_program: Option<(AccountId, [u8; 32])>,
    pub pool: pool::Pool,
}
impl PoolAccount {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        borsh::from_slice(bytes).map_err(|_| Error::InvalidState)
    }
    pub fn validate_address(&self, actor: Actor) -> Result<(), Error> {
        if actor.account_id
            != compute_pool_pda(
                self.namespace,
                actor.program_account_id,
                self.token0_definition_id,
                self.token1_definition_id,
                self.owner,
            )
        {
            return Err(Error::WrongAccount);
        }
        self.pool
            .validate_policy()
            .map_err(|_| Error::InvalidPool)?;
        self.buy_policy
            .validate_binding(self.namespace.value(), actor.account_id.value())
            .map_err(|_| Error::InvalidPool)?;
        Ok(())
    }
}
