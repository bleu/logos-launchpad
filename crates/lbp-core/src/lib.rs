//! Fixed-price and weighted sale setup, trades, fees and mechanism-specific settlement.

pub use curve_core::{AllowlistPolicy, BuyPolicy, MembershipPath};
use sale_policy::{EndingError, TimeBounds};
pub use sale_policy::{
    EndingPolicy, PausePolicy, SettlementAmounts, SettlementProgress, SettlementRoute,
    TradeControl, TradeMode,
};

mod events;
#[path = "settlement.rs"]
mod lbp_settlement;
mod pricing;
mod quote;
pub use quote::{QuoteRequest, TradeQuote, quote_trade};
pub mod pricing_clock;
mod trading;
pub use lbp_settlement::{SettlementReceipt, SettlementReply};
mod weighted_lifecycle;
use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, ReceiveInput, Response, StateReply},
};
use lez_compat_core::token::{self, TokenKind, TokenState};
use serde::{Deserialize, Serialize};

/// Fixed normalization total for weighted-sale endpoints.
pub const WEIGHT_TOTAL: u128 = 1_000_000_000_000_000_000;
pub const MIN_WEIGHT: u128 = WEIGHT_TOTAL / 100;

pub fn valid_weight_endpoints(start: u128, end: u128) -> bool {
    let bounds = MIN_WEIGHT..=WEIGHT_TOTAL - MIN_WEIGHT;
    bounds.contains(&start) && bounds.contains(&end)
}

// ---------------------------------------------------------------------------
// Fixed-price types
// ---------------------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub struct FixedPrice {
    pub collateral_units: u128,
    pub token_units: u128,
}
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum CreatorAllocationRelease {
    AfterSetup,
    AfterEnd,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct FixedPricePoolInputs {
    pub buy_policy: BuyPolicy,
    pub pause_policy: PausePolicy,
    pub ending_policy: EndingPolicy,
    pub trade_mode: TradeMode,
    pub sell_fee_bps: u16,
    pub graduation_adapter: Option<Actor>,
    pub namespace: AccountId,
    pub namespace_program_id: AccountId,
    pub factory_program_id: AccountId,
    pub factory_seed: [u8; 32],
    pub token_definition_id: AccountId,
    pub collateral_definition_id: AccountId,
    pub sale_inventory: u128,
    pub creator_allocation: u128,
    pub price: FixedPrice,
    pub start_timestamp: u64,
    pub end_timestamp: Option<u64>,
    pub creator_allocation_release: CreatorAllocationRelease,
}

// ---------------------------------------------------------------------------
// Weighted-LBP types
// ---------------------------------------------------------------------------

/// Inputs for a Balancer-style weighted LBP where token/collateral weights
/// shift linearly from `*_start` to `*_end` over the sale window.
///
/// Each endpoint pair sums to `WEIGHT_TOTAL`. Each weight is at least 1%.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct WeightedPoolInputs {
    pub buy_policy: BuyPolicy,
    pub pause_policy: PausePolicy,
    pub ending_policy: EndingPolicy,
    pub trade_mode: TradeMode,
    pub sell_fee_bps: u16,
    pub graduation_adapter: Option<Actor>,
    pub namespace: AccountId,
    pub namespace_program_id: AccountId,
    pub factory_program_id: AccountId,
    pub factory_seed: [u8; 32],
    pub token_definition_id: AccountId,
    pub collateral_definition_id: AccountId,
    /// Total tokens offered for sale (initial token reserve).
    pub sale_inventory: u128,
    /// Initial collateral deposited by the creator to seed the pool price.
    /// Must be >= 1.
    pub collateral_seed: u128,
    pub creator_allocation: u128,
    /// Normalized token weight at sale start, e.g. `95 * WEIGHT_TOTAL / 100`.
    pub token_weight_start: u128,
    /// Normalized token weight at sale end.
    pub token_weight_end: u128,
    /// Unix timestamp when the sale opens.
    pub start_timestamp: u64,
    /// Unix timestamp when the sale closes. Required (no open-ended weighted LBP).
    pub end_timestamp: u64,
    pub creator_allocation_release: CreatorAllocationRelease,
}

// ---------------------------------------------------------------------------
// Shared message / state types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Message {
    PrepareFixedPrice {
        inputs: FixedPricePoolInputs,
    },
    ActivateFixedPrice,
    ReadState,
    BuyExactInput {
        membership: Option<MembershipPath>,
        amount_in: u128,
        min_amount_out: u128,
        now: u64,
    },
    BuyExactOutput {
        membership: Option<MembershipPath>,
        amount_out: u128,
        max_amount_in: u128,
        now: u64,
    },
    CollectFees,
    Execute {
        pool: AccountId,
        instruction: Box<Message>,
    },
    SettleFixedPrice {
        creator: AccountId,
        now: u64,
    },
    PrepareWeighted {
        inputs: WeightedPoolInputs,
    },
    ActivateWeighted,
    /// Buy using the timestamp returned by the configured pricing clock.
    BuyWeightedExactInput {
        membership: Option<MembershipPath>,
        amount_in: u128,
        min_amount_out: u128,
    },
    /// Buy using the timestamp returned by the configured pricing clock.
    BuyWeightedExactOutput {
        membership: Option<MembershipPath>,
        amount_out: u128,
        max_amount_in: u128,
    },
    /// Factory-authorized weighted controls. No caller-supplied time.
    SetWeightedPaused {
        paused: bool,
    },
    SettleWeighted {
        creator: AccountId,
    },
    /// Permissionless observer checkpoint. Reads the pricing clock and records
    /// the current token weight without changing the price schedule.
    PokeWeighted,
    /// Factory-authorized controls for either sale mode.
    SetPaused {
        paused: bool,
    },
    Settle {
        route: SettlementRoute,
        creator: AccountId,
        adapter: Option<Actor>,
    },
    FinalizeSettlement {
        receipt_hash: [u8; 32],
    },
    SellExactInput {
        amount_in: u128,
        min_amount_out: u128,
        now: u64,
    },
    SellWeightedExactInput {
        amount_in: u128,
        min_amount_out: u128,
    },
}
pub type Instruction = Message;

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PoolMode {
    FixedPrice,
    /// Collateral weight is the exact complement within `WEIGHT_TOTAL`.
    Weighted {
        token_weight_start: u128,
        token_weight_end: u128,
        /// Immutable creator seed. Surplus does not change this value.
        collateral_seed: u128,
        /// Seed plus net buys minus gross sells plus retained fees. Custody is separate.
        pricing_collateral: u128,
        /// Last observed pricing second. Zero means no checkpoint recorded.
        checkpoint_time: u64,
        /// Token weight at checkpoint_time. Zero when no checkpoint recorded.
        checkpoint_token_weight: u128,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PoolAccount {
    pub buy_policy: BuyPolicy,
    pub trade_control: TradeControl,
    pub ending_policy: EndingPolicy,
    pub trade_mode: TradeMode,
    pub sell_fee_bps: u16,
    pub graduation_adapter: Option<Actor>,
    pub mode: PoolMode,
    pub funded: bool,
    pub closure_observed: bool,
    pub namespace: AccountId,
    pub namespace_program_id: AccountId,
    pub factory_owner: AccountId,
    pub factory_program_id: AccountId,
    pub factory_seed: [u8; 32],
    pub token_definition_id: AccountId,
    pub collateral_definition_id: AccountId,
    pub sale_inventory: u128,
    pub remaining_inventory: u128,
    pub creator_allocation: u128,
    pub price: FixedPrice,
    pub start_timestamp: u64,
    pub end_timestamp: Option<u64>,
    pub creator_allocation_release: CreatorAllocationRelease,
    pub recorded_collateral: u128,
    pub accrued_fees: u128,
    pub settlement_progress: SettlementProgress,
    pub seed_returned: bool,
    pending: Option<Pending>,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct Pending {
    activate: bool,
    step: u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    ClockUnavailable,
    InvalidMessage,
    InvalidState,
    Unauthorized,
    WrongAccount,
    AlreadyInitialized,
    InvalidTerms,
    InvalidDefinition,
    InvalidCustody,
    AlreadyFunded,
    RecordedCollateral,
    PendingFunding,
    Frozen,
    Paused,
    Closed,
    NotClosed,
    InvalidFee,
    InvalidAmount,
    Slippage,
    Arithmetic,
    MathAccuracy,
    PowerUnderflow,
    Underfunded,
    PauseDisabled,
    TradeDirectionUnsupported,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl PoolAccount {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        borsh::from_slice(bytes).map_err(|_| Error::InvalidState)
    }
    pub fn close_time_bound(&self) -> Result<Option<u64>, Error> {
        self.validate_policies()?;
        Ok(self
            .ending_policy
            .settlement(self.end_timestamp, self.remaining_inventory)
            .map_err(ending_error)?
            .inclusion_bounds()
            .at_least)
    }
    fn trade_bounds(&self) -> Result<TimeBounds, Error> {
        self.ending_policy
            .trade_bounds(
                Some(self.start_timestamp),
                self.end_timestamp,
                self.remaining_inventory,
            )
            .map_err(ending_error)
    }
    fn validate_policies(&self) -> Result<(), Error> {
        sale_policy::SellFeeRate::new(self.sell_fee_bps).map_err(|_| Error::InvalidState)?;
        if !supported_ending(
            matches!(self.mode, PoolMode::Weighted { .. }),
            self.ending_policy,
        ) {
            return Err(Error::InvalidState);
        }
        validate_policy_terms(
            matches!(self.mode, PoolMode::Weighted { .. }),
            self.ending_policy,
            self.start_timestamp,
            self.end_timestamp,
        )
    }
    fn validate_address(&self, receiver: Actor) -> Result<(), Error> {
        self.buy_policy
            .validate_binding(self.namespace.value(), receiver.account_id.value())
            .map_err(|_| Error::InvalidTerms)?;
        if receiver.account_id
            != compute_pool_pda(
                self.namespace,
                receiver.program_account_id,
                self.token_definition_id,
                self.collateral_definition_id,
                self.factory_owner,
            )
        {
            return Err(Error::WrongAccount);
        }
        self.validate_policies()
    }
    fn response(&self) -> Response {
        Response::set_state(borsh::to_vec(self).unwrap())
    }
}
fn ending_error(error: EndingError) -> Error {
    match error {
        EndingError::InvalidWindow => Error::InvalidTerms,
        EndingError::Ended => Error::Closed,
        EndingError::NotEnded => Error::NotClosed,
    }
}

fn supported_ending(weighted: bool, ending: EndingPolicy) -> bool {
    matches!(ending, EndingPolicy::Scheduled | EndingPolicy::Either)
        || (!weighted && ending == EndingPolicy::Depletion)
}

fn validate_policy_terms(
    weighted: bool,
    ending: EndingPolicy,
    start: u64,
    end: Option<u64>,
) -> Result<(), Error> {
    if !supported_ending(weighted, ending) {
        return Err(Error::InvalidTerms);
    }
    ending.validate(Some(start), end).map_err(ending_error)?;
    let _ = curve_core::with_time_bounds(Response::keep_state(), Some(start), end)
        .map_err(|_| Error::InvalidTerms)?;
    Ok(())
}

pub fn compute_pool_pda(
    namespace: AccountId,
    program: AccountId,
    token: AccountId,
    collateral: AccountId,
    owner: AccountId,
) -> AccountId {
    curve_core::compute_pool_pda(namespace, program, token, collateral, owner)
}
pub fn compute_pool_pda_seed(
    namespace: AccountId,
    token: AccountId,
    collateral: AccountId,
    owner: AccountId,
) -> PdaSeed {
    curve_core::compute_pool_pda_seed(namespace, token, collateral, owner)
}

// ---------------------------------------------------------------------------
// Shared pending-step state machine for setup handshakes
// ---------------------------------------------------------------------------

/// Resume namespace, token definition and custody validation.
/// Returns `None` when setup is not pending.
fn advance_setup_steps(
    input: &ReceiveInput,
    state: &mut PoolAccount,
) -> Result<Option<Response>, Error> {
    let pending = match state.pending.as_mut() {
        Some(p) => p,
        None => return Ok(None),
    };

    if pending.step > 0 {
        let (owner, definition) = match pending.step {
            1 => (state.token_definition_id, state.token_definition_id),
            2 => (
                state.collateral_definition_id,
                state.collateral_definition_id,
            ),
            3 => (state.factory_owner, state.token_definition_id),
            4 => (input.receiver.account_id, state.token_definition_id),
            5 => (input.receiver.account_id, state.collateral_definition_id),
            _ => return Err(Error::InvalidState),
        };
        let address = if pending.step < 3 {
            definition
        } else {
            token::holding_address(curve_core::TOKEN_PROGRAM_ACCOUNT_ID, owner, definition)
        };
        let value = token::read_reply(
            input,
            Actor::new(address, curve_core::TOKEN_PROGRAM_ACCOUNT_ID),
        )
        .map_err(|_| Error::Unauthorized)?;

        match &state.mode {
            PoolMode::FixedPrice => {
                if pending.activate
                    && matches!(&value, TokenState::Holding { balance, .. }
                        if (pending.step == 3 && *balance != 0)
                        || (pending.step == 4 && *balance != state.sale_inventory))
                {
                    return Err(Error::InvalidCustody);
                }
                if !pending.activate
                    && pending.step >= 4
                    && matches!(&value, TokenState::Holding { balance, .. } if *balance != 0)
                {
                    return Err(Error::InvalidCustody);
                }
            }
            PoolMode::Weighted {
                collateral_seed, ..
            } => {
                if pending.activate {
                    match pending.step {
                        // Factory's source token holding must be 0 after transfer
                        3 => {
                            if matches!(&value, TokenState::Holding { balance, .. } if *balance != 0)
                            {
                                return Err(Error::InvalidCustody);
                            }
                        }
                        // Pool token vault must have exactly sale_inventory
                        4 => {
                            if matches!(&value, TokenState::Holding { balance, .. } if *balance != state.sale_inventory)
                            {
                                return Err(Error::InvalidCustody);
                            }
                        }
                        // Pool collateral vault must cover the seed.
                        5 => {
                            if matches!(&value, TokenState::Holding { balance, .. } if *balance < *collateral_seed)
                            {
                                return Err(Error::InvalidCustody);
                            }
                        }
                        _ => {}
                    }
                } else {
                    // Prepare: pool vaults must be empty
                    if pending.step >= 4
                        && matches!(&value, TokenState::Holding { balance, .. } if *balance != 0)
                    {
                        return Err(Error::InvalidCustody);
                    }
                }
            }
        }

        match value {
            TokenState::Definition {
                kind: TokenKind::Fungible,
                ..
            } if pending.step < 3 => {}
            TokenState::Holding {
                kind: TokenKind::Fungible,
                owner: stored,
                definition: stored_definition,
                ..
            } if pending.step >= 3 && stored == owner && stored_definition == definition => {}
            _ => return Err(Error::InvalidDefinition),
        }
        pending.step += 1;
        if pending.step == 6 {
            if pending.activate {
                state.funded = true;
                if let PoolMode::Weighted {
                    collateral_seed,
                    pricing_collateral,
                    ..
                } = &mut state.mode
                {
                    state.recorded_collateral = *collateral_seed;
                    *pricing_collateral = *collateral_seed;
                }
            }
            let kind = if pending.activate {
                curve_core::events::SaleEventKindV1::Activated {
                    reserve0: state.remaining_inventory,
                    reserve1: state.recorded_collateral,
                    collateral_seed: match state.mode {
                        PoolMode::FixedPrice => 0,
                        PoolMode::Weighted {
                            collateral_seed, ..
                        } => collateral_seed,
                    },
                }
            } else {
                curve_core::events::SaleEventKindV1::Prepared {
                    owner: *state.factory_owner.value(),
                    token0_definition: *state.token_definition_id.value(),
                    token1_definition: *state.collateral_definition_id.value(),
                    reserve0: state.sale_inventory,
                    reserve1: match state.mode {
                        PoolMode::FixedPrice => 0,
                        PoolMode::Weighted {
                            collateral_seed, ..
                        } => collateral_seed,
                    },
                    policies: state.event_policies(),
                }
            };
            state.pending = None;
            let response =
                curve_core::with_time_bounds(state.response(), None, state.end_timestamp)
                    .map_err(|_| Error::InvalidTerms)?
                    .event(state.event(input.receiver, kind));
            let response = if matches!(state.mode, PoolMode::Weighted { .. }) {
                response.send_sync(
                    Actor::new(
                        clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
                        clock_core::clock_account_id(),
                    ),
                    &curve_core::clock_assertion(0, state.end_timestamp)
                        .map_err(|_| Error::InvalidTerms)?,
                )
            } else {
                response
            };
            return Ok(Some(response));
        }
        let step = state.pending.as_ref().unwrap().step;
        let (next_owner, next_definition) = match step {
            2 => (
                state.collateral_definition_id,
                state.collateral_definition_id,
            ),
            3 => (state.factory_owner, state.token_definition_id),
            4 => (input.receiver.account_id, state.token_definition_id),
            _ => (input.receiver.account_id, state.collateral_definition_id),
        };
        let next_address = if step == 2 {
            next_definition
        } else {
            token::holding_address(
                curve_core::TOKEN_PROGRAM_ACCOUNT_ID,
                next_owner,
                next_definition,
            )
        };
        let mut response = state.response();
        if step >= 4 {
            response = response.send_sync(
                Actor::new(next_definition, curve_core::TOKEN_PROGRAM_ACCOUNT_ID),
                &token::Message::Initialize { owner: next_owner },
            )
        }
        return Ok(Some(response.send_sync(
            Actor::new(next_address, curve_core::TOKEN_PROGRAM_ACCOUNT_ID),
            &token::Message::ReadState,
        )));
    }

    if pending.step == 0 {
        let expected = Actor::new(
            curve_core::compute_config_pda(state.namespace, state.namespace_program_id),
            state.namespace_program_id,
        );
        if input.from != Some(expected) {
            return Err(Error::Unauthorized);
        }
        let reply: StateReply =
            borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
        let config = curve_core::Config::decode(&reply.state).map_err(|_| Error::InvalidState)?;
        if config.namespace != state.namespace {
            return Err(Error::WrongAccount);
        }
        state.pending.as_mut().unwrap().step = 1;
        return Ok(Some(state.response().send_sync(
            Actor::new(
                state.token_definition_id,
                curve_core::TOKEN_PROGRAM_ACCOUNT_ID,
            ),
            &token::Message::ReadState,
        )));
    }

    Ok(None)
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn handle(input: &ReceiveInput) -> Result<Response, Error> {
    if let Some(result) = lbp_settlement::resume(input) {
        return result;
    }
    if let Some(result) = weighted_lifecycle::resume_poke(input) {
        return result;
    }
    if let Some(result) = trading::resume(input) {
        return result;
    }
    if let Ok(state) = PoolAccount::decode(&input.pre_state) {
        state.validate_address(input.receiver)?;
    }
    if let Ok(mut state) = PoolAccount::decode(&input.pre_state)
        && state.pending.is_some()
        && let Some(response) = advance_setup_steps(input, &mut state)?
    {
        return Ok(response);
    }
    let message: Message = borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    match message {
        Message::PrepareFixedPrice { inputs } => {
            if !input.pre_state.is_empty() {
                return Err(Error::AlreadyInitialized);
            }
            if inputs.namespace == AccountId::default()
                || inputs.sale_inventory == 0
                || inputs.price.collateral_units == 0
                || inputs.price.token_units == 0
                || inputs
                    .end_timestamp
                    .is_some_and(|end| end <= inputs.start_timestamp)
                || inputs.token_definition_id == inputs.collateral_definition_id
            {
                return Err(Error::InvalidTerms);
            }
            sale_policy::SellFeeRate::new(inputs.sell_fee_bps).map_err(|_| Error::InvalidTerms)?;
            validate_policy_terms(
                false,
                inputs.ending_policy,
                inputs.start_timestamp,
                inputs.end_timestamp,
            )?;
            let factory = AccountId::for_public_pda(
                &inputs.factory_program_id,
                &PdaSeed::new(inputs.factory_seed),
            );
            if input.receiver.account_id
                != compute_pool_pda(
                    inputs.namespace,
                    input.receiver.program_account_id,
                    inputs.token_definition_id,
                    inputs.collateral_definition_id,
                    factory,
                )
            {
                return Err(Error::WrongAccount);
            }
            if input.from != Some(Actor::new(factory, inputs.factory_program_id)) {
                return Err(Error::Unauthorized);
            }
            inputs
                .buy_policy
                .validate_binding(inputs.namespace.value(), input.receiver.account_id.value())
                .map_err(|_| Error::InvalidTerms)?;
            let state = PoolAccount {
                buy_policy: inputs.buy_policy,
                trade_control: TradeControl::new(inputs.pause_policy),
                ending_policy: inputs.ending_policy,
                trade_mode: inputs.trade_mode,
                sell_fee_bps: inputs.sell_fee_bps,
                graduation_adapter: inputs.graduation_adapter,
                mode: PoolMode::FixedPrice,
                funded: false,
                closure_observed: false,
                namespace: inputs.namespace,
                namespace_program_id: inputs.namespace_program_id,
                factory_owner: factory,
                factory_program_id: inputs.factory_program_id,
                factory_seed: inputs.factory_seed,
                token_definition_id: inputs.token_definition_id,
                collateral_definition_id: inputs.collateral_definition_id,
                sale_inventory: inputs.sale_inventory,
                remaining_inventory: inputs.sale_inventory,
                creator_allocation: inputs.creator_allocation,
                price: inputs.price,
                start_timestamp: inputs.start_timestamp,
                end_timestamp: inputs.end_timestamp,
                creator_allocation_release: inputs.creator_allocation_release,
                recorded_collateral: 0,
                accrued_fees: 0,
                settlement_progress: crate::SettlementProgress::Unselected,
                seed_returned: true,
                pending: Some(Pending {
                    activate: false,
                    step: 0,
                }),
            };
            Ok(state.response().send_sync(
                Actor::new(
                    curve_core::compute_config_pda(state.namespace, state.namespace_program_id),
                    state.namespace_program_id,
                ),
                &curve_core::Message::ReadNamespace {
                    namespace: state.namespace,
                },
            ))
        }
        Message::ActivateFixedPrice => {
            let mut state = PoolAccount::decode(&input.pre_state)?;
            if state.mode != PoolMode::FixedPrice {
                return Err(Error::InvalidState);
            }
            if state.funded {
                return Err(Error::AlreadyFunded);
            }
            if state.recorded_collateral != 0 || state.accrued_fees != 0 {
                return Err(Error::RecordedCollateral);
            }
            if input.receiver.account_id
                != compute_pool_pda(
                    state.namespace,
                    input.receiver.program_account_id,
                    state.token_definition_id,
                    state.collateral_definition_id,
                    state.factory_owner,
                )
            {
                return Err(Error::WrongAccount);
            }
            if input.from != Some(Actor::new(state.factory_owner, state.factory_program_id)) {
                return Err(Error::Unauthorized);
            }
            state.pending = Some(Pending {
                activate: true,
                step: 0,
            });
            Ok(state.response().send_sync(
                Actor::new(
                    curve_core::compute_config_pda(state.namespace, state.namespace_program_id),
                    state.namespace_program_id,
                ),
                &curve_core::Message::ReadNamespace {
                    namespace: state.namespace,
                },
            ))
        }
        Message::PrepareWeighted { inputs } => {
            if !input.pre_state.is_empty() {
                return Err(Error::AlreadyInitialized);
            }
            if inputs.namespace == AccountId::default()
                || inputs.sale_inventory == 0
                || inputs.collateral_seed == 0
                || inputs.token_definition_id == inputs.collateral_definition_id
                || inputs.end_timestamp <= inputs.start_timestamp
                || !valid_weight_endpoints(inputs.token_weight_start, inputs.token_weight_end)
            {
                return Err(Error::InvalidTerms);
            }
            sale_policy::SellFeeRate::new(inputs.sell_fee_bps).map_err(|_| Error::InvalidTerms)?;
            validate_policy_terms(
                true,
                inputs.ending_policy,
                inputs.start_timestamp,
                Some(inputs.end_timestamp),
            )?;
            let factory = AccountId::for_public_pda(
                &inputs.factory_program_id,
                &PdaSeed::new(inputs.factory_seed),
            );
            if input.receiver.account_id
                != compute_pool_pda(
                    inputs.namespace,
                    input.receiver.program_account_id,
                    inputs.token_definition_id,
                    inputs.collateral_definition_id,
                    factory,
                )
            {
                return Err(Error::WrongAccount);
            }
            if input.from != Some(Actor::new(factory, inputs.factory_program_id)) {
                return Err(Error::Unauthorized);
            }
            inputs
                .buy_policy
                .validate_binding(inputs.namespace.value(), input.receiver.account_id.value())
                .map_err(|_| Error::InvalidTerms)?;
            let state = PoolAccount {
                buy_policy: inputs.buy_policy,
                trade_control: TradeControl::new(inputs.pause_policy),
                ending_policy: inputs.ending_policy,
                trade_mode: inputs.trade_mode,
                sell_fee_bps: inputs.sell_fee_bps,
                graduation_adapter: inputs.graduation_adapter,
                mode: PoolMode::Weighted {
                    token_weight_start: inputs.token_weight_start,
                    token_weight_end: inputs.token_weight_end,
                    collateral_seed: inputs.collateral_seed,
                    pricing_collateral: 0,
                    checkpoint_time: 0,
                    checkpoint_token_weight: 0,
                },
                funded: false,
                closure_observed: false,
                namespace: inputs.namespace,
                namespace_program_id: inputs.namespace_program_id,
                factory_owner: factory,
                factory_program_id: inputs.factory_program_id,
                factory_seed: inputs.factory_seed,
                token_definition_id: inputs.token_definition_id,
                collateral_definition_id: inputs.collateral_definition_id,
                sale_inventory: inputs.sale_inventory,
                remaining_inventory: inputs.sale_inventory,
                creator_allocation: inputs.creator_allocation,
                // Fixed-price terms are not used by weighted sales.
                price: FixedPrice {
                    collateral_units: 0,
                    token_units: 0,
                },
                start_timestamp: inputs.start_timestamp,
                end_timestamp: Some(inputs.end_timestamp),
                creator_allocation_release: inputs.creator_allocation_release,
                recorded_collateral: 0,
                accrued_fees: 0,
                settlement_progress: crate::SettlementProgress::Unselected,
                seed_returned: false,
                pending: Some(Pending {
                    activate: false,
                    step: 0,
                }),
            };
            Ok(state.response().send_sync(
                Actor::new(
                    curve_core::compute_config_pda(state.namespace, state.namespace_program_id),
                    state.namespace_program_id,
                ),
                &curve_core::Message::ReadNamespace {
                    namespace: state.namespace,
                },
            ))
        }
        Message::ActivateWeighted => {
            let mut state = PoolAccount::decode(&input.pre_state)?;
            if !matches!(state.mode, PoolMode::Weighted { .. }) {
                return Err(Error::InvalidState);
            }
            if state.funded {
                return Err(Error::AlreadyFunded);
            }
            if input.receiver.account_id
                != compute_pool_pda(
                    state.namespace,
                    input.receiver.program_account_id,
                    state.token_definition_id,
                    state.collateral_definition_id,
                    state.factory_owner,
                )
            {
                return Err(Error::WrongAccount);
            }
            if input.from != Some(Actor::new(state.factory_owner, state.factory_program_id)) {
                return Err(Error::Unauthorized);
            }
            state.pending = Some(Pending {
                activate: true,
                step: 0,
            });
            Ok(state.response().send_sync(
                Actor::new(
                    curve_core::compute_config_pda(state.namespace, state.namespace_program_id),
                    state.namespace_program_id,
                ),
                &curve_core::Message::ReadNamespace {
                    namespace: state.namespace,
                },
            ))
        }
        Message::Execute { pool, instruction } => {
            if !input.is_authorized
                || !matches!(
                    *instruction,
                    Message::BuyExactInput { .. }
                        | Message::BuyExactOutput { .. }
                        | Message::BuyWeightedExactInput { .. }
                        | Message::BuyWeightedExactOutput { .. }
                        | Message::SellExactInput { .. }
                        | Message::SellWeightedExactInput { .. }
                )
            {
                return Err(Error::Unauthorized);
            }
            Ok(Response::keep_state().send_sync(
                Actor::new(pool, input.receiver.program_account_id),
                &*instruction,
            ))
        }
        Message::BuyExactInput { .. }
        | Message::BuyExactOutput { .. }
        | Message::BuyWeightedExactInput { .. }
        | Message::BuyWeightedExactOutput { .. }
        | Message::SellExactInput { .. }
        | Message::SellWeightedExactInput { .. }
        | Message::SettleFixedPrice { .. }
        | Message::CollectFees => trading::begin(input, message),
        Message::SetWeightedPaused { paused } => {
            weighted_lifecycle::end(&PoolAccount::decode(&input.pre_state)?)?;
            weighted_lifecycle::set_paused(input, paused)
        }
        Message::SetPaused { paused } => weighted_lifecycle::set_paused(input, paused),
        Message::SettleWeighted { .. } | Message::Settle { .. } => trading::begin(input, message),
        Message::FinalizeSettlement { .. } => Err(Error::InvalidState),
        Message::PokeWeighted => weighted_lifecycle::poke(input),
        Message::ReadState => {
            PoolAccount::decode(&input.pre_state)?.validate_address(input.receiver)?;
            Ok(Response::keep_state().send_sync(
                input.from.ok_or(Error::Unauthorized)?,
                &StateReply::from(input),
            ))
        }
    }
}
