use crate::{Error, Message, PoolAccount, compute_pool_pda_seed};
use borsh::{BorshDeserialize, BorshSerialize};
use curve_core::{Config, TOKEN_PROGRAM_ACCOUNT_ID};
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response, StateReply, SyncMessage},
};
use lez_compat_core::token::{self, TokenKind, TokenState};

#[derive(BorshSerialize, BorshDeserialize)]
struct Pending {
    tag: [u8; 8],
    state: PoolAccount,
    message: Message,
    buyer: AccountId,
    step: u8,
    config: Option<Config>,
    balances: [u128; 3],
}
impl Pending {
    fn holding(&self, receiver: Actor) -> (AccountId, AccountId) {
        match self.step {
            1 => (receiver.account_id, self.state.collateral_definition_id),
            2 => (receiver.account_id, self.state.token_definition_id),
            _ => (self.buyer, self.state.collateral_definition_id),
        }
    }
    fn expected(&self, receiver: Actor) -> Actor {
        if self.step == 0 {
            return Actor::new(
                curve_core::compute_config_pda(
                    self.state.namespace,
                    self.state.namespace_program_id,
                ),
                self.state.namespace_program_id,
            );
        }
        let (owner, definition) = self.holding(receiver);
        Actor::new(
            token::holding_address(TOKEN_PROGRAM_ACCOUNT_ID, owner, definition),
            TOKEN_PROGRAM_ACCOUNT_ID,
        )
    }
    fn advance(self, receiver: Actor) -> Result<Response, Error> {
        if self.step == 4
            || (self.step == 2 && matches!(self.message, Message::CollectFees))
            || (self.step == 3 && matches!(self.message, Message::SettleFixedPrice { .. }))
        {
            return self.finish(receiver);
        }
        let expected = self.expected(receiver);
        let response = Response::set_state(borsh::to_vec(&self).unwrap());
        Ok(if self.step == 0 {
            response.send_sync(
                expected,
                &curve_core::Message::ReadNamespace {
                    namespace: self.state.namespace,
                },
            )
        } else {
            response.send_sync(expected, &token::Message::ReadState)
        })
    }
    fn finish(mut self, receiver: Actor) -> Result<Response, Error> {
        let config = self.config.as_ref().ok_or(Error::InvalidState)?;
        let liabilities = self
            .state
            .recorded_collateral
            .checked_add(self.state.accrued_fees)
            .ok_or(Error::Arithmetic)?;
        if self.balances[0] < liabilities {
            return Err(Error::Underfunded);
        }
        if matches!(self.message, Message::CollectFees) {
            let amount = self.state.accrued_fees;
            self.state.accrued_fees = 0;
            return Ok(payout(
                self.state.response(),
                &self.state,
                receiver,
                self.state.collateral_definition_id,
                config.treasury,
                amount,
            ));
        }
        if let Message::SettleFixedPrice { creator, now } = self.message {
            let clock = self.state.close_clock_assertion(now)?;
            if self.balances[1] < self.state.remaining_inventory {
                return Err(Error::Underfunded);
            }
            let fees = self.state.accrued_fees;
            let proceeds = self.state.recorded_collateral;
            self.state.accrued_fees = 0;
            self.state.recorded_collateral = 0;
            self.state.proceeds_settled = true;
            let mut response = self.state.response();
            if let Some(clock) = clock {
                response = response.send_sync(
                    Actor::new(
                        clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
                        clock_core::clock_account_id(),
                    ),
                    &clock,
                );
            }
            let response = payout(
                response,
                &self.state,
                receiver,
                self.state.collateral_definition_id,
                config.treasury,
                fees,
            );
            let response = if self.state.remaining_inventory == 0 {
                response
            } else {
                response.send(
                    SyncMessage::new(
                        Actor::new(receiver.account_id, TOKEN_PROGRAM_ACCOUNT_ID),
                        &token::Message::Burn {
                            definition: self.state.token_definition_id,
                            amount: self.state.remaining_inventory,
                        },
                    )
                    .with_pda_seeds([compute_pool_pda_seed(
                        self.state.namespace,
                        self.state.token_definition_id,
                        self.state.collateral_definition_id,
                        self.state.factory_owner,
                    )]),
                )
            };
            return Ok(payout(
                response,
                &self.state,
                receiver,
                self.state.collateral_definition_id,
                creator,
                proceeds,
            ));
        }
        if config.emergency.is_frozen {
            return Err(Error::Frozen);
        }
        if config.protocol_fee_bps >= 10_000 {
            return Err(Error::InvalidFee);
        }
        let (amount_in, amount_out, fee, now) = match self.message {
            Message::BuyExactInput {
                amount_in,
                min_amount_out,
                now,
            } => {
                let fee = mul_div(amount_in, u128::from(config.protocol_fee_bps), 10_000, true)?;
                let effective_input = amount_in.checked_sub(fee).ok_or(Error::Arithmetic)?;
                if effective_input == 0 {
                    return Err(Error::InvalidAmount);
                }
                let out = mul_div(
                    effective_input,
                    self.state.price.token_units,
                    self.state.price.collateral_units,
                    false,
                )?;
                if out < min_amount_out {
                    return Err(Error::Slippage);
                }
                (amount_in, out, fee, now)
            }
            Message::BuyExactOutput {
                amount_out,
                max_amount_in,
                now,
            } => {
                let required_input = mul_div(
                    amount_out,
                    self.state.price.collateral_units,
                    self.state.price.token_units,
                    true,
                )?;
                let input = mul_div(
                    required_input,
                    10_000,
                    10_000_u128
                        .checked_sub(u128::from(config.protocol_fee_bps))
                        .ok_or(Error::Arithmetic)?,
                    true,
                )?;
                if input > max_amount_in {
                    return Err(Error::Slippage);
                }
                let fee = mul_div(input, u128::from(config.protocol_fee_bps), 10_000, true)?;
                (input, amount_out, fee, now)
            }
            _ => return Err(Error::InvalidMessage),
        };
        if self.state.proceeds_settled
            || now < self.state.start_timestamp
            || self.state.end_timestamp.is_some_and(|end| now >= end)
            || self.state.remaining_inventory == 0
        {
            return Err(Error::Closed);
        }
        if amount_in == 0 || amount_out == 0 || amount_out > self.state.remaining_inventory {
            return Err(Error::InvalidAmount);
        }
        if self.balances[1] < amount_out || self.balances[2] < amount_in {
            return Err(Error::Underfunded);
        }
        let proceeds = amount_in.checked_sub(fee).ok_or(Error::Arithmetic)?;
        if proceeds == 0 {
            return Err(Error::InvalidAmount);
        }
        self.state.recorded_collateral = self
            .state
            .recorded_collateral
            .checked_add(proceeds)
            .ok_or(Error::Arithmetic)?;
        self.state.accrued_fees = self
            .state
            .accrued_fees
            .checked_add(fee)
            .ok_or(Error::Arithmetic)?;
        self.state.remaining_inventory -= amount_out;
        let next_liabilities = self
            .state
            .recorded_collateral
            .checked_add(self.state.accrued_fees)
            .ok_or(Error::Arithmetic)?;
        if self.balances[0]
            .checked_add(amount_in)
            .ok_or(Error::Arithmetic)?
            < next_liabilities
        {
            return Err(Error::Underfunded);
        }
        let response = self
            .state
            .response()
            .send_sync(
                Actor::new(
                    clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
                    clock_core::clock_account_id(),
                ),
                &curve_core::clock_assertion(now, self.state.end_timestamp)
                    .map_err(|_| Error::InvalidTerms)?,
            )
            .send_sync(
                Actor::new(self.buyer, TOKEN_PROGRAM_ACCOUNT_ID),
                &token::Message::Transfer {
                    definition: self.state.collateral_definition_id,
                    recipient: receiver.account_id,
                    amount: amount_in,
                },
            );
        Ok(payout(
            response,
            &self.state,
            receiver,
            self.state.token_definition_id,
            self.buyer,
            amount_out,
        ))
    }
}

pub(super) fn begin(input: &ReceiveInput, message: Message) -> Result<Response, Error> {
    let state = PoolAccount::decode(&input.pre_state)?;
    state.validate_address(input.receiver)?;
    if !state.funded {
        return Err(Error::PendingFunding);
    }
    let buyer = if let Message::SettleFixedPrice { creator, .. } = message {
        if input.from != Some(Actor::new(state.factory_owner, state.factory_program_id))
            || creator == AccountId::default()
        {
            return Err(Error::Unauthorized);
        }
        if state.proceeds_settled {
            return Ok(state.response());
        }
        creator
    } else if matches!(message, Message::CollectFees) {
        AccountId::default()
    } else {
        let sender = input.from.ok_or(Error::Unauthorized)?;
        if sender.program_account_id != input.receiver.program_account_id
            || sender.account_id == input.receiver.account_id
        {
            return Err(Error::Unauthorized);
        }
        sender.account_id
    };
    Pending {
        tag: *b"LZLBPEND",
        state,
        message,
        buyer,
        step: 0,
        config: None,
        balances: [0; 3],
    }
    .advance(input.receiver)
}
pub(super) fn resume(input: &ReceiveInput) -> Option<Result<Response, Error>> {
    if !input.pre_state.starts_with(b"LZLBPEND") {
        return None;
    }
    Some((|| {
        let mut pending: Pending =
            borsh::from_slice(&input.pre_state).map_err(|_| Error::InvalidState)?;
        pending.state.validate_address(input.receiver)?;
        if pending.step > 3 {
            return Err(Error::InvalidState);
        }
        let expected = pending.expected(input.receiver);
        if input.from != Some(expected) {
            return Err(Error::Unauthorized);
        }
        if pending.step == 0 {
            let reply: StateReply =
                borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
            let config = Config::decode(&reply.state).map_err(|_| Error::InvalidState)?;
            if config.namespace != pending.state.namespace {
                return Err(Error::WrongAccount);
            }
            if config.treasury == AccountId::default() {
                return Err(Error::InvalidState);
            }
            pending.config = Some(config);
        } else {
            let (expected_owner, expected_definition) = pending.holding(input.receiver);
            let TokenState::Holding {
                owner,
                definition,
                kind: TokenKind::Fungible,
                balance,
            } = token::read_reply(input, expected).map_err(|_| Error::InvalidDefinition)?
            else {
                return Err(Error::InvalidDefinition);
            };
            if owner != expected_owner || definition != expected_definition {
                return Err(Error::InvalidDefinition);
            }
            pending.balances[usize::from(pending.step - 1)] = balance;
        }
        pending.step += 1;
        pending.advance(input.receiver)
    })())
}
fn payout(
    response: Response,
    state: &PoolAccount,
    receiver: Actor,
    definition: AccountId,
    recipient: AccountId,
    amount: u128,
) -> Response {
    if amount == 0 || recipient == receiver.account_id {
        return response;
    }
    response.send(
        SyncMessage::new(
            Actor::new(receiver.account_id, TOKEN_PROGRAM_ACCOUNT_ID),
            &token::Message::Transfer {
                definition,
                recipient,
                amount,
            },
        )
        .with_pda_seeds([compute_pool_pda_seed(
            state.namespace,
            state.token_definition_id,
            state.collateral_definition_id,
            state.factory_owner,
        )]),
    )
}
fn mul_div(value: u128, multiplier: u128, divisor: u128, ceil: bool) -> Result<u128, Error> {
    if divisor == 0 {
        return Err(Error::InvalidTerms);
    }
    let whole = (value / divisor)
        .checked_mul(multiplier)
        .ok_or(Error::Arithmetic)?;
    let numerator = (value % divisor)
        .checked_mul(multiplier)
        .ok_or(Error::Arithmetic)?;
    let floor = whole
        .checked_add(numerator / divisor)
        .ok_or(Error::Arithmetic)?;
    floor
        .checked_add(u128::from(ceil && numerator % divisor != 0))
        .ok_or(Error::Arithmetic)
}
