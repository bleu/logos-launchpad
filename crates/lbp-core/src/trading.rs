use crate::{
    Error, Message, PoolAccount, PoolMode, WEIGHT_TOTAL, compute_pool_pda_seed,
    valid_weight_endpoints,
};
use borsh::{BorshDeserialize, BorshSerialize};
use curve_core::{Config, TOKEN_PROGRAM_ACCOUNT_ID};
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response, StateReply, SyncMessage},
};
use lez_compat_core::token::{self, TokenKind, TokenState};
use weighted_math_core::{
    QuoteError, interpolate_weight, try_calc_in_given_out, try_calc_out_given_in,
};

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
        if let Message::SettleFixedPrice { creator, .. } = self.message {
            let expiry = self.state.close_time_bound()?;
            if self.balances[1] < self.state.remaining_inventory {
                return Err(Error::Underfunded);
            }
            let fees = self.state.accrued_fees;
            let proceeds = self.state.recorded_collateral;
            self.state.accrued_fees = 0;
            self.state.recorded_collateral = 0;
            self.state.proceeds_settled = true;
            let response = curve_core::with_time_bounds(self.state.response(), expiry, None)
                .map_err(|_| Error::InvalidTerms)?;
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

        validate_buy_window(&self.state, &self.message)?;
        if self.state.proceeds_settled || self.state.remaining_inventory == 0 {
            return Err(Error::Closed);
        }
        if let PoolMode::Weighted {
            collateral_seed,
            pricing_collateral,
            ..
        } = self.state.mode
        {
            if self.state.remaining_inventory > self.state.sale_inventory
                || pricing_collateral < collateral_seed
                || pricing_collateral < liabilities
            {
                return Err(Error::InvalidState);
            }
            if self.balances[1] < self.state.remaining_inventory {
                return Err(Error::Underfunded);
            }
        }

        let (amount_in, amount_out) = match self.state.mode {
            PoolMode::FixedPrice => {
                fixed_price_amounts(&self.state, &self.message, config.protocol_fee_bps)?
            }
            PoolMode::Weighted {
                token_weight_start,
                token_weight_end,
                pricing_collateral,
                ..
            } => weighted_amounts(
                &self.state,
                &self.message,
                token_weight_start,
                token_weight_end,
                pricing_collateral,
            )?,
        };

        if amount_in == 0 || amount_out == 0 || amount_out > self.state.remaining_inventory {
            return Err(Error::InvalidAmount);
        }
        if self.balances[1] < amount_out || self.balances[2] < amount_in {
            return Err(Error::Underfunded);
        }

        let fee = mul_div(amount_in, u128::from(config.protocol_fee_bps), 10_000, true)?;
        let net_in = amount_in.checked_sub(fee).ok_or(Error::Arithmetic)?;
        if matches!(self.state.mode, PoolMode::FixedPrice) && net_in == 0 {
            return Err(Error::InvalidAmount);
        }

        let next_pricing = match self.state.mode {
            PoolMode::Weighted {
                pricing_collateral, ..
            } => Some(
                pricing_collateral
                    .checked_add(amount_in)
                    .ok_or(Error::Arithmetic)?,
            ),
            PoolMode::FixedPrice => None,
        };
        let next_recorded = self
            .state
            .recorded_collateral
            .checked_add(net_in)
            .ok_or(Error::Arithmetic)?;
        let next_fees = self
            .state
            .accrued_fees
            .checked_add(fee)
            .ok_or(Error::Arithmetic)?;
        let next_inventory = self
            .state
            .remaining_inventory
            .checked_sub(amount_out)
            .ok_or(Error::Arithmetic)?;

        let next_liabilities = next_recorded
            .checked_add(next_fees)
            .ok_or(Error::Arithmetic)?;
        if self.balances[0]
            .checked_add(amount_in)
            .ok_or(Error::Arithmetic)?
            < next_liabilities
        {
            return Err(Error::Underfunded);
        }
        // Save trade records only after all math, coverage and fit checks pass.
        self.state.recorded_collateral = next_recorded;
        self.state.accrued_fees = next_fees;
        self.state.remaining_inventory = next_inventory;
        if let (
            PoolMode::Weighted {
                pricing_collateral, ..
            },
            Some(next),
        ) = (&mut self.state.mode, next_pricing)
        {
            *pricing_collateral = next;
        }
        let response = match self.state.mode {
            PoolMode::FixedPrice => curve_core::with_time_bounds(
                self.state.response(),
                Some(self.state.start_timestamp),
                self.state.end_timestamp,
            )
            .map_err(|_| Error::InvalidTerms)?,
            PoolMode::Weighted { .. } => {
                let (start, end) = weighted_window(&self.message)?;
                curve_core::with_time_bounds(self.state.response(), Some(start), Some(end))
                    .map_err(|_| Error::Arithmetic)?
                    .send_sync(
                        Actor::new(
                            clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
                            clock_core::clock_account_id(),
                        ),
                        &curve_core::clock_assertion(start, Some(end))
                            .map_err(|_| Error::Arithmetic)?,
                    )
            }
        }
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

// ---------------------------------------------------------------------------
// Amount computation helpers
// ---------------------------------------------------------------------------

/// Compute (amount_in, amount_out) for a fixed-price pool.
fn fixed_price_amounts(
    state: &PoolAccount,
    message: &Message,
    fee_bps: u16,
) -> Result<(u128, u128), Error> {
    match message {
        Message::BuyExactInput {
            amount_in,
            min_amount_out,
            ..
        } => {
            let fee = mul_div(*amount_in, u128::from(fee_bps), 10_000, true)?;
            let net = amount_in.checked_sub(fee).ok_or(Error::Arithmetic)?;
            if net == 0 {
                return Err(Error::InvalidAmount);
            }
            let out = mul_div(
                net,
                state.price.token_units,
                state.price.collateral_units,
                false,
            )?;
            if out < *min_amount_out {
                return Err(Error::Slippage);
            }
            Ok((*amount_in, out))
        }
        Message::BuyExactOutput {
            amount_out,
            max_amount_in,
            ..
        } => {
            let input = mul_div(
                *amount_out,
                state.price.collateral_units,
                state.price.token_units,
                true,
            )?;
            let input = mul_div(input, 10_000, 10_000 - u128::from(fee_bps), true)?;
            if input > *max_amount_in {
                return Err(Error::Slippage);
            }
            Ok((input, *amount_out))
        }
        _ => Err(Error::InvalidMessage),
    }
}

/// Compute amounts from recorded pricing reserves. Creator-borne fees do not
/// reduce buyer input or change pricing reserves when they are collected.
fn weighted_amounts(
    state: &PoolAccount,
    message: &Message,
    token_weight_start: u128,
    token_weight_end: u128,
    pricing_collateral: u128,
) -> Result<(u128, u128), Error> {
    let (valid_from, valid_until) = weighted_window(message)?;
    let end_ts = state.end_timestamp.ok_or(Error::InvalidState)?;
    if !valid_weight_endpoints(token_weight_start, token_weight_end) {
        return Err(Error::InvalidTerms);
    }
    // The smallest collateral weight gives the least output and greatest input.
    // Linear schedules are monotone, so only the window endpoints are needed.
    let collateral_at = |time| {
        interpolate_weight(
            WEIGHT_TOTAL - token_weight_start,
            WEIGHT_TOTAL - token_weight_end,
            state.start_timestamp,
            end_ts,
            time,
        )
    };
    let coll_w = collateral_at(valid_from).min(collateral_at(valid_until - 1));
    let token_w = WEIGHT_TOTAL - coll_w;

    let coll_bal = pricing_collateral;
    let token_bal = state.remaining_inventory;

    if coll_bal == 0 || token_bal == 0 {
        return Err(Error::Underfunded);
    }

    match message {
        Message::BuyWeightedExactInput {
            amount_in,
            min_amount_out,
            ..
        } => {
            let out = try_calc_out_given_in(coll_bal, coll_w, token_bal, token_w, *amount_in)
                .map_err(math_error)?;
            if out < *min_amount_out {
                return Err(Error::Slippage);
            }
            Ok((*amount_in, out))
        }
        Message::BuyWeightedExactOutput {
            amount_out,
            max_amount_in,
            ..
        } => {
            let amount_in =
                try_calc_in_given_out(coll_bal, coll_w, token_bal, token_w, *amount_out)
                    .map_err(math_error)?;
            if amount_in > *max_amount_in {
                return Err(Error::Slippage);
            }
            Ok((amount_in, *amount_out))
        }
        _ => Err(Error::InvalidMessage),
    }
}

fn math_error(error: QuoteError) -> Error {
    match error {
        QuoteError::EmptyReserve => Error::Underfunded,
        QuoteError::InvalidWeights => Error::InvalidTerms,
        QuoteError::InvalidAmount | QuoteError::AmountOutOfBounds | QuoteError::Dust => {
            Error::InvalidAmount
        }
        QuoteError::Arithmetic => Error::Arithmetic,
        QuoteError::PowerUnderflow => Error::PowerUnderflow,
        QuoteError::Accuracy => Error::MathAccuracy,
        QuoteError::UnqualifiedScale => Error::InvalidState,
    }
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

pub(super) fn begin(input: &ReceiveInput, message: Message) -> Result<Response, Error> {
    let state = PoolAccount::decode(&input.pre_state)?;
    state.validate_address(input.receiver)?;
    if !state.funded {
        return Err(Error::PendingFunding);
    }
    if !matches!(
        message,
        Message::CollectFees | Message::SettleFixedPrice { .. }
    ) {
        validate_buy_window(&state, &message)?;
    }
    let buyer = if let Message::SettleFixedPrice { creator, .. } = message {
        if state.mode != PoolMode::FixedPrice {
            return Err(Error::InvalidMessage);
        }
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

/// Validate sale bounds before pricing. Weighted buys also check saved clock time.
fn validate_buy_window(state: &PoolAccount, message: &Message) -> Result<(), Error> {
    match (state.mode, message) {
        (
            PoolMode::FixedPrice,
            Message::BuyExactInput { now, .. } | Message::BuyExactOutput { now, .. },
        ) => {
            if *now < state.start_timestamp || state.end_timestamp.is_some_and(|end| *now >= end) {
                return Err(Error::Closed);
            }
            let _ = curve_core::with_time_bounds(
                Response::keep_state(),
                Some(state.start_timestamp),
                state.end_timestamp,
            )
            .map_err(|_| Error::InvalidTerms)?;
            Ok(())
        }
        (
            PoolMode::Weighted { .. },
            Message::BuyWeightedExactInput { .. } | Message::BuyWeightedExactOutput { .. },
        ) => {
            let (valid_from, valid_until) = weighted_window(message)?;
            if valid_from >= valid_until {
                return Err(Error::InvalidTerms);
            }
            let end = state.end_timestamp.ok_or(Error::InvalidState)?;
            if valid_from < state.start_timestamp || valid_until > end {
                return Err(Error::Closed);
            }
            curve_core::clock_assertion(valid_from, Some(valid_until))
                .map_err(|_| Error::Arithmetic)?;
            Ok(())
        }
        _ => Err(Error::InvalidMessage),
    }
}

fn weighted_window(message: &Message) -> Result<(u64, u64), Error> {
    match message {
        Message::BuyWeightedExactInput {
            valid_from,
            valid_until,
            ..
        }
        | Message::BuyWeightedExactOutput {
            valid_from,
            valid_until,
            ..
        } => Ok((*valid_from, *valid_until)),
        _ => Err(Error::InvalidMessage),
    }
}
