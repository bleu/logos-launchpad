use crate::{Error, Message, PoolAccount, PoolMode, compute_pool_pda_seed};
use borsh::{BorshDeserialize, BorshSerialize};
use curve_core::{Config, TOKEN_PROGRAM_ACCOUNT_ID};
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response, StateReply, SyncMessage},
};
use lez_compat_core::token::{self, TokenKind, TokenState};
use sale_policy::{FeeError, FeeRate};
use weighted_math_core::{QuoteError, try_calc_in_given_out, try_calc_out_given_in};

#[derive(BorshSerialize, BorshDeserialize)]
struct Pending {
    tag: [u8; 8],
    state: PoolAccount,
    message: Message,
    buyer: AccountId,
    step: u8,
    // One reply per buy, in Unix seconds. Never persisted in the final pool state.
    pricing_time: Option<u64>,
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
        if self.step == 4 {
            return crate::pricing_clock::actor();
        }
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
            && self.pricing_time.is_none()
            && matches!(
                self.message,
                Message::BuyWeightedExactInput { .. } | Message::BuyWeightedExactOutput { .. }
            )
        {
            crate::pricing_clock::require_available()?;
            return Ok(
                Response::set_state(borsh::to_vec(&self).unwrap()).send_sync(
                    crate::pricing_clock::actor(),
                    &crate::pricing_clock::Request::ReadTimestamp,
                ),
            );
        }
        if self.step == 4
            || (self.step == 2 && matches!(self.message, Message::CollectFees))
            || (self.step == 3
                && matches!(
                    self.message,
                    Message::SettleFixedPrice { .. }
                        | Message::SettleWeighted { .. }
                        | Message::Settle { .. }
                ))
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
        self.state.validate_policies()?;
        let config = self.config.as_ref().ok_or(Error::InvalidState)?;
        let liabilities = self
            .state
            .recorded_collateral
            .checked_add(self.state.accrued_fees)
            .ok_or(Error::Arithmetic)?;
        if self.balances[0] < liabilities {
            return Err(Error::Underfunded);
        }
        if !self.state.settlement_progress.is_complete()
            && let PoolMode::Weighted {
                collateral_seed,
                pricing_collateral,
                ..
            } = self.state.mode
            && (pricing_collateral != self.state.recorded_collateral
                || pricing_collateral < collateral_seed)
        {
            return Err(Error::InvalidState);
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
            if self.state.mode != PoolMode::FixedPrice {
                return Err(Error::InvalidMessage);
            }
            let expiry = self.state.close_time_bound()?;
            return settlement_payout(
                self.state,
                receiver,
                creator,
                config.treasury,
                self.balances,
                expiry,
            );
        }
        if let Message::SettleWeighted { creator } = self.message {
            crate::weighted_lifecycle::end(&self.state)?;
            let expiry = self.state.close_time_bound()?;
            return settlement_payout(
                self.state,
                receiver,
                creator,
                config.treasury,
                self.balances,
                expiry,
            );
        }
        if let Message::Settle {
            route,
            creator,
            adapter,
        } = self.message
        {
            let expiry = self.state.close_time_bound()?;
            return crate::lbp_settlement::apply(
                self.state,
                receiver,
                (route, creator, adapter),
                config.treasury,
                self.balances,
                expiry,
            );
        }
        if config.emergency.is_frozen {
            return Err(Error::Frozen);
        }
        if config.protocol_fee_bps >= 10_000 {
            return Err(Error::InvalidFee);
        }

        if self.state.trade_control.is_paused() {
            return Err(Error::Paused);
        }
        validate_buy_window(&self.state, &self.message)?;
        if self.state.settlement_progress.is_complete() || self.state.remaining_inventory == 0 {
            return Err(Error::Closed);
        }
        if matches!(self.state.mode, PoolMode::Weighted { .. }) {
            if self.state.remaining_inventory > self.state.sale_inventory {
                return Err(Error::InvalidState);
            }
            if self.balances[1] < self.state.remaining_inventory {
                return Err(Error::Underfunded);
            }
        }

        let weights = self
            .pricing_time
            .map(|time| crate::weighted_lifecycle::weights_at(&self.state, time))
            .transpose()?;
        let (amount_in, amount_out) = match self.state.mode {
            PoolMode::FixedPrice => {
                fixed_price_amounts(&self.state, &self.message, config.protocol_fee_bps)?
            }
            PoolMode::Weighted {
                pricing_collateral, ..
            } => weighted_amounts(
                &self.state,
                &self.message,
                pricing_collateral,
                self.pricing_time.ok_or(Error::InvalidState)?,
                weights.ok_or(Error::InvalidState)?,
                config.protocol_fee_bps,
            )?,
        };

        if amount_in == 0 || amount_out == 0 || amount_out > self.state.remaining_inventory {
            return Err(Error::InvalidAmount);
        }
        if self.balances[1] < amount_out || self.balances[2] < amount_in {
            return Err(Error::Underfunded);
        }

        let rate = FeeRate::new(config.protocol_fee_bps).map_err(fee_error)?;
        let collateral = rate.split(amount_in).map_err(fee_error)?;
        let fee = collateral.fee;
        let net_in = collateral.net;
        if net_in == 0 {
            return Err(Error::InvalidAmount);
        }

        let next_pricing = match self.state.mode {
            PoolMode::Weighted {
                pricing_collateral, ..
            } => Some(
                pricing_collateral
                    .checked_add(net_in)
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
        let bounds = self.state.trade_bounds()?;
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
        // Record an observer checkpoint when the trusted timestamp changes.
        let checkpoint_event = if let (
            PoolMode::Weighted {
                checkpoint_time, ..
            },
            Some(pt),
        ) = (self.state.mode, self.pricing_time)
        {
            if pt != checkpoint_time {
                crate::weighted_lifecycle::record_checkpoint(
                    &mut self.state,
                    pt,
                    weights.ok_or(Error::InvalidState)?.1,
                )
            } else {
                None
            }
        } else {
            None
        };
        let response =
            curve_core::with_time_bounds(self.state.response(), bounds.at_least, bounds.before)
                .map_err(|_| match self.state.mode {
                    PoolMode::FixedPrice => Error::InvalidTerms,
                    PoolMode::Weighted { .. } => Error::Arithmetic,
                })?;
        let response = if let Some(event) = checkpoint_event {
            response.event(event)
        } else {
            response
        };
        let response = response.send_sync(
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
            let net = FeeRate::new(fee_bps)
                .and_then(|rate| rate.split(*amount_in))
                .map_err(fee_error)?
                .net;
            if net == 0 {
                return Err(Error::InvalidAmount);
            }
            let out = state.price.amount_out(net)?;
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
            let input = state.price.amount_in(*amount_out)?;
            let input = FeeRate::new(fee_bps)
                .and_then(|rate| rate.gross_up(input))
                .map_err(fee_error)?
                .gross;
            if input > *max_amount_in {
                return Err(Error::Slippage);
            }
            Ok((input, *amount_out))
        }
        _ => Err(Error::InvalidMessage),
    }
}

/// Price net collateral against recorded reserves. Fee collection leaves pricing unchanged.
fn weighted_amounts(
    state: &PoolAccount,
    message: &Message,
    pricing_collateral: u128,
    pricing_time: u64,
    (coll_w, token_w): (u128, u128),
    fee_bps: u16,
) -> Result<(u128, u128), Error> {
    let end_ts = state.end_timestamp.ok_or(Error::InvalidState)?;
    if pricing_time < state.start_timestamp || pricing_time >= end_ts {
        return Err(Error::Closed);
    }

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
            let net = FeeRate::new(fee_bps)
                .and_then(|rate| rate.split(*amount_in))
                .map_err(fee_error)?
                .net;
            if net == 0 {
                return Err(Error::InvalidAmount);
            }
            let out = try_calc_out_given_in(coll_bal, coll_w, token_bal, token_w, net)
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
            let amount_in = FeeRate::new(fee_bps)
                .and_then(|rate| rate.gross_up(amount_in))
                .map_err(fee_error)?
                .gross;
            if amount_in > *max_amount_in {
                return Err(Error::Slippage);
            }
            Ok((amount_in, *amount_out))
        }
        _ => Err(Error::InvalidMessage),
    }
}

fn fee_error(error: FeeError) -> Error {
    match error {
        FeeError::RateAboveDenominator => Error::InvalidFee,
        FeeError::InputConsumedByFees => Error::InvalidAmount,
        FeeError::Arithmetic => Error::Arithmetic,
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
    if matches!(
        message,
        Message::BuyExactInput { .. }
            | Message::BuyExactOutput { .. }
            | Message::BuyWeightedExactInput { .. }
            | Message::BuyWeightedExactOutput { .. }
    ) && state.trade_control.is_paused()
    {
        return Err(Error::Paused);
    }
    if !matches!(
        message,
        Message::CollectFees
            | Message::SettleFixedPrice { .. }
            | Message::SettleWeighted { .. }
            | Message::Settle { .. }
    ) {
        validate_buy_window(&state, &message)?;
    }
    let buyer = if let Message::Settle {
        route,
        creator,
        adapter,
    } = message
    {
        if input.from != Some(Actor::new(state.factory_owner, state.factory_program_id)) {
            return Err(Error::Unauthorized);
        }
        crate::lbp_settlement::validate_route(&state, route, creator, adapter)?;
        creator
    } else if let Message::SettleFixedPrice { creator, .. } = message {
        if state.mode != PoolMode::FixedPrice {
            return Err(Error::InvalidMessage);
        }
        if input.from != Some(Actor::new(state.factory_owner, state.factory_program_id))
            || creator == AccountId::default()
        {
            return Err(Error::Unauthorized);
        }
        if state.settlement_progress.is_complete() {
            return Ok(state.response());
        }
        creator
    } else if let Message::SettleWeighted { creator } = message {
        crate::weighted_lifecycle::authorize(&state, input)?;
        if creator == AccountId::default() {
            return Err(Error::Unauthorized);
        }
        if state.settlement_progress.is_complete() {
            return curve_core::with_time_bounds(state.response(), state.close_time_bound()?, None)
                .map_err(|_| Error::InvalidTerms);
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
        match &message {
            Message::BuyExactInput { membership, .. }
            | Message::BuyExactOutput { membership, .. }
            | Message::BuyWeightedExactInput { membership, .. }
            | Message::BuyWeightedExactOutput { membership, .. } => state
                .buy_policy
                .admit(sender.account_id.value(), membership.as_ref())
                .map_err(|_| Error::Unauthorized)?,
            _ => {}
        }
        sender.account_id
    };
    Pending {
        tag: *b"LZLBPEND",
        state,
        message,
        buyer,
        step: 0,
        pricing_time: None,
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
        if pending.step == 4 {
            pending.pricing_time = Some(crate::pricing_clock::read(input)?);
            return pending.finish(input.receiver);
        }
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

/// Validate admission bounds. Weighted pricing time arrives later in a clock reply.
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
            crate::pricing_clock::require_available()?;
            let _ = curve_core::with_time_bounds(
                Response::keep_state(),
                Some(state.start_timestamp),
                state.end_timestamp,
            )
            .map_err(|_| Error::Arithmetic)?;
            Ok(())
        }
        _ => Err(Error::InvalidMessage),
    }
}

/// Shared token effects only. Each mechanism supplies its own closing bound.
fn settlement_payout(
    mut state: PoolAccount,
    receiver: Actor,
    creator: AccountId,
    treasury: AccountId,
    balances: [u128; 3],
    expiry: Option<u64>,
) -> Result<Response, Error> {
    let plan = crate::lbp_settlement::plan(&state, crate::SettlementRoute::Manual)?;
    if balances[1] < plan.burn {
        return Err(Error::Underfunded);
    }
    state.settlement_progress = plan.progress;
    state.seed_returned = true;
    state.accrued_fees = 0;
    state.recorded_collateral = 0;
    let response = curve_core::with_time_bounds(state.response(), expiry, None)
        .map_err(|_| Error::InvalidTerms)?;
    Ok(custody_effects(
        response, &state, receiver, creator, None, treasury, &plan,
    ))
}

pub(super) fn custody_effects(
    response: Response,
    state: &PoolAccount,
    receiver: Actor,
    creator: AccountId,
    adapter: Option<Actor>,
    treasury: AccountId,
    plan: &crate::lbp_settlement::CustodyPlan,
) -> Response {
    let response = payout(
        response,
        state,
        receiver,
        state.collateral_definition_id,
        treasury,
        plan.fees,
    );
    let response = if plan.burn == 0 {
        response
    } else {
        response.send(
            SyncMessage::new(
                Actor::new(receiver.account_id, TOKEN_PROGRAM_ACCOUNT_ID),
                &token::Message::Burn {
                    definition: state.token_definition_id,
                    amount: plan.burn,
                },
            )
            .with_pda_seeds([compute_pool_pda_seed(
                state.namespace,
                state.token_definition_id,
                state.collateral_definition_id,
                state.factory_owner,
            )]),
        )
    };
    if let Some(adapter) = adapter {
        let response = payout(
            response,
            state,
            receiver,
            state.collateral_definition_id,
            creator,
            plan.seed,
        );
        payout(
            response,
            state,
            receiver,
            state.collateral_definition_id,
            adapter.account_id,
            plan.amounts.collateral,
        )
    } else {
        payout(
            response,
            state,
            receiver,
            state.collateral_definition_id,
            creator,
            plan.seed + plan.amounts.collateral,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CreatorAllocationRelease, FixedPrice, TradeControl};

    fn fixture() -> (PoolAccount, Config) {
        let id = |n| AccountId::new([n; 32]);
        let namespace = id(40);
        let namespace_program = id(6);
        let namespace_input = ReceiveInput {
            receiver: Actor::new(
                curve_core::compute_config_pda(namespace, namespace_program),
                namespace_program,
            ),
            from: Some(Actor::new(namespace, namespace_program)),
            is_authorized: false,
            pre_state: vec![].into(),
            message: borsh::to_vec(&curve_core::Message::InitializeNamespace {
                namespace,
                protocol_fee_bps: 0,
                treasury: id(43),
            })
            .unwrap(),
        };
        let config = Config::decode(
            curve_core::handle(&namespace_input)
                .unwrap()
                .post_state
                .as_ref()
                .unwrap(),
        )
        .unwrap();
        let state = PoolAccount {
            buy_policy: crate::BuyPolicy::Open,
            trade_control: TradeControl::Paused,
            ending_policy: crate::EndingPolicy::Either,
            graduation_adapter: None,
            mode: PoolMode::FixedPrice,
            funded: true,
            namespace,
            namespace_program_id: namespace_program,
            factory_owner: id(7),
            factory_program_id: id(8),
            factory_seed: [3; 32],
            token_definition_id: id(41),
            collateral_definition_id: id(42),
            sale_inventory: 800,
            remaining_inventory: 800,
            creator_allocation: 0,
            price: FixedPrice {
                collateral_units: 3,
                token_units: 2,
            },
            start_timestamp: 100,
            end_timestamp: Some(200),
            creator_allocation_release: CreatorAllocationRelease::AfterSetup,
            recorded_collateral: 0,
            accrued_fees: 0,
            settlement_progress: crate::SettlementProgress::Unselected,
            seed_returned: true,
            pending: None,
        };
        (state, config)
    }

    #[test]
    fn final_buy_boundary_checks_common_pause_for_every_buy_form() {
        let id = |n| AccountId::new([n; 32]);
        let (state, config) = fixture();
        let weighted = PoolMode::Weighted {
            token_weight_start: crate::WEIGHT_TOTAL / 2,
            token_weight_end: crate::WEIGHT_TOTAL / 2,
            collateral_seed: 100,
            pricing_collateral: 100,
            checkpoint_time: 0,
            checkpoint_token_weight: 0,
        };
        for (mode, message) in [
            (
                PoolMode::FixedPrice,
                Message::BuyExactInput {
                    membership: None,
                    amount_in: 5,
                    min_amount_out: 1,
                    now: 100,
                },
            ),
            (
                PoolMode::FixedPrice,
                Message::BuyExactOutput {
                    membership: None,
                    amount_out: 3,
                    max_amount_in: 6,
                    now: 100,
                },
            ),
            (
                weighted,
                Message::BuyWeightedExactInput {
                    membership: None,
                    amount_in: 5,
                    min_amount_out: 1,
                },
            ),
            (
                weighted,
                Message::BuyWeightedExactOutput {
                    membership: None,
                    amount_out: 3,
                    max_amount_in: 6,
                },
            ),
        ] {
            let mut state = state.clone();
            state.mode = mode;
            if matches!(mode, PoolMode::Weighted { .. }) {
                state.recorded_collateral = 100;
                state.ending_policy = crate::EndingPolicy::Scheduled;
            }
            let pending = Pending {
                tag: *b"LZLBPEND",
                state,
                message,
                buyer: id(50),
                step: 4,
                pricing_time: Some(100),
                config: Some(config.clone()),
                balances: [100, 800, 100],
            };
            assert_eq!(
                pending.finish(Actor::new(id(9), id(8))).err(),
                Some(Error::Paused)
            );
        }
    }

    #[test]
    fn saved_trade_and_settlement_replies_reject_malformed_policies_before_effects() {
        let (base, _) = fixture();
        for weighted in [false, true] {
            let mut state = base.clone();
            state.trade_control = crate::TradeControl::Unrestricted;
            if weighted {
                state.mode = PoolMode::Weighted {
                    token_weight_start: crate::WEIGHT_TOTAL / 2,
                    token_weight_end: crate::WEIGHT_TOTAL / 2,
                    collateral_seed: 100,
                    pricing_collateral: 100,
                    checkpoint_time: 0,
                    checkpoint_token_weight: 0,
                };
                state.ending_policy = crate::EndingPolicy::Scheduled;
            }
            let receiver = Actor::new(
                crate::compute_pool_pda(
                    state.namespace,
                    AccountId::new([8; 32]),
                    state.token_definition_id,
                    state.collateral_definition_id,
                    state.factory_owner,
                ),
                AccountId::new([8; 32]),
            );
            let buys = if weighted {
                [
                    Message::BuyWeightedExactInput {
                        membership: None,
                        amount_in: 5,
                        min_amount_out: 0,
                    },
                    Message::BuyWeightedExactOutput {
                        membership: None,
                        amount_out: 3,
                        max_amount_in: 6,
                    },
                ]
            } else {
                [
                    Message::BuyExactInput {
                        membership: None,
                        amount_in: 5,
                        min_amount_out: 0,
                        now: 100,
                    },
                    Message::BuyExactOutput {
                        membership: None,
                        amount_out: 3,
                        max_amount_in: 6,
                        now: 100,
                    },
                ]
            };
            let settle = if weighted {
                Message::SettleWeighted {
                    creator: AccountId::new([50; 32]),
                }
            } else {
                Message::SettleFixedPrice {
                    creator: AccountId::new([50; 32]),
                    now: 200,
                }
            };
            for message in [
                buys[0].clone(),
                buys[1].clone(),
                Message::CollectFees,
                settle,
            ] {
                for step in [0, 4] {
                    for case in 0..4 {
                        let mut malformed = state.clone();
                        match case {
                            0 => malformed.ending_policy = crate::EndingPolicy::Manual,
                            1 => malformed.ending_policy = crate::EndingPolicy::Depletion,
                            2 => malformed.start_timestamp = u64::MAX,
                            _ => malformed.end_timestamp = Some(100),
                        }
                        let expected = if case == 0 || (case == 1 && weighted) {
                            Error::InvalidState
                        } else {
                            Error::InvalidTerms
                        };
                        let pending = Pending {
                            tag: *b"LZLBPEND",
                            state: malformed,
                            message: message.clone(),
                            buyer: AccountId::new([50; 32]),
                            step,
                            pricing_time: Some(100),
                            config: None,
                            balances: [0; 3],
                        };
                        let input = ReceiveInput {
                            receiver,
                            from: None,
                            is_authorized: false,
                            pre_state: borsh::to_vec(&pending).unwrap().into(),
                            message: vec![],
                        };
                        assert_eq!(crate::handle(&input).err(), Some(expected));
                        assert_eq!(pending.finish(receiver).err(), Some(expected));
                    }
                }
            }
        }
    }

    #[test]
    fn shared_manual_completion_errors_map_to_invalid_state() {
        let (state, _) = fixture();
        let receiver = Actor::new(AccountId::new([9; 32]), AccountId::new([8; 32]));
        for progress in [
            crate::SettlementProgress::ManualComplete,
            crate::SettlementProgress::Graduated,
        ] {
            let mut completed = state.clone();
            completed.settlement_progress = progress;
            assert_eq!(
                settlement_payout(
                    completed,
                    receiver,
                    AccountId::new([50; 32]),
                    AccountId::new([43; 32]),
                    [0, 800, 0],
                    Some(200)
                )
                .err(),
                Some(Error::InvalidState)
            );
        }
    }
}
