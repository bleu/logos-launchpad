//! Fixed-price sale setup, buys and creator-borne protocol fees.
mod trading;
use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, ReceiveInput, Response, StateReply},
};
use lez_compat_core::token::{self, TokenKind, TokenState};
use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[expect(
    clippy::large_enum_variant,
    reason = "Setup messages carry owned sale terms."
)]
pub enum Message {
    PrepareFixedPrice {
        inputs: FixedPricePoolInputs,
    },
    ActivateFixedPrice,
    ReadState,
    BuyExactInput {
        amount_in: u128,
        min_amount_out: u128,
        now: u64,
    },
    BuyExactOutput {
        amount_out: u128,
        max_amount_in: u128,
        now: u64,
    },
    CollectFees,
    Execute {
        pool: AccountId,
        instruction: Box<Message>,
    },
}
pub type Instruction = Message;
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PoolMode {
    FixedPrice,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PoolAccount {
    pub mode: PoolMode,
    pub funded: bool,
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
    pending: Option<Pending>,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct Pending {
    activate: bool,
    step: u8,
}
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
    AlreadyFunded,
    RecordedCollateral,
    PendingFunding,
    Frozen,
    Closed,
    InvalidFee,
    InvalidAmount,
    Slippage,
    Arithmetic,
    Underfunded,
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
    fn validate_address(&self, receiver: Actor) -> Result<(), Error> {
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
        Ok(())
    }
    fn response(&self) -> Response {
        Response::set_state(borsh::to_vec(self).unwrap())
    }
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
pub fn handle(input: &ReceiveInput) -> Result<Response, Error> {
    if let Some(result) = trading::resume(input) {
        return result;
    }
    if let Ok(state) = PoolAccount::decode(&input.pre_state) {
        state.validate_address(input.receiver)?;
    }
    if let Ok(mut state) = PoolAccount::decode(&input.pre_state)
        && let Some(pending) = state.pending.as_mut()
    {
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
            if pending.activate
                && matches!(&value,TokenState::Holding{balance,..} if (pending.step==3&&*balance!=0)||(pending.step==4&&*balance!=state.sale_inventory))
            {
                return Err(Error::InvalidCustody);
            }
            if !pending.activate
                && pending.step >= 4
                && matches!(&value,TokenState::Holding{balance,..} if *balance!=0)
            {
                return Err(Error::InvalidCustody);
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
                    state.funded = true
                }
                state.pending = None;
                return Ok(state.response().send_sync(
                    Actor::new(
                        clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
                        clock_core::clock_account_id(),
                    ),
                    &curve_core::clock_assertion(0, state.end_timestamp)
                        .map_err(|_| Error::InvalidTerms)?,
                ));
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
            return Ok(response.send_sync(
                Actor::new(next_address, curve_core::TOKEN_PROGRAM_ACCOUNT_ID),
                &token::Message::ReadState,
            ));
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
            let config =
                curve_core::Config::decode(&reply.state).map_err(|_| Error::InvalidState)?;
            if config.namespace != state.namespace {
                return Err(Error::WrongAccount);
            }
            pending.step = 1;
            return Ok(state.response().send_sync(
                Actor::new(
                    state.token_definition_id,
                    curve_core::TOKEN_PROGRAM_ACCOUNT_ID,
                ),
                &token::Message::ReadState,
            ));
        }
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
            let state = PoolAccount {
                mode: PoolMode::FixedPrice,
                funded: false,
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
        Message::Execute { pool, instruction } => {
            if !input.is_authorized
                || !matches!(
                    *instruction,
                    Message::BuyExactInput { .. } | Message::BuyExactOutput { .. }
                )
            {
                return Err(Error::Unauthorized);
            }
            Ok(Response::keep_state().send_sync(
                Actor::new(pool, input.receiver.program_account_id),
                &*instruction,
            ))
        }
        Message::BuyExactInput { .. } | Message::BuyExactOutput { .. } | Message::CollectFees => {
            trading::begin(input, message)
        }
        Message::ReadState => {
            PoolAccount::decode(&input.pre_state)?.validate_address(input.receiver)?;
            Ok(Response::keep_state().send_sync(
                input.from.ok_or(Error::Unauthorized)?,
                &StateReply::from(input),
            ))
        }
    }
}
