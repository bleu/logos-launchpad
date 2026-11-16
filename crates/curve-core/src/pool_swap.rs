//! Curve pricing stays in the pure pool crate.
use crate::{
    Error, Message, PoolAccount, TOKEN_PROGRAM_ACCOUNT_ID,
    dispatch::{Pending, Query},
    pool_create::transfer,
};
use lee_core::{
    account::Actor,
    program::{ReceiveInput, Response, SyncMessage},
};
use pool::{ExactInputSwap, ExactOutputSwap, TokenSide};
use sale_policy::events::{SaleEventKindV1, TradeFormV1};

pub fn begin(input: &ReceiveInput, message: Message) -> Result<Response, Error> {
    let state = PoolAccount::decode(&input.pre_state)?;
    state.validate_address(input.receiver)?;
    if state.trade_control.is_paused() {
        return Err(Error::Paused);
    }
    let sender = input.from.ok_or(Error::Unauthorized)?;
    if sender.program_account_id != input.receiver.program_account_id
        || matches!(state.buy_policy, crate::BuyPolicy::Allowlist(_))
            && sender.account_id == input.receiver.account_id
    {
        return Err(Error::Unauthorized);
    }
    let (token_in, membership) = match &message {
        Message::SwapExactInput {
            token_in,
            membership,
            ..
        }
        | Message::SwapExactOutput {
            token_in,
            membership,
            ..
        } => (token_in, membership),
        _ => return Err(Error::InvalidMessage),
    };
    if *token_in == state.token1_definition_id {
        state
            .buy_policy
            .admit(sender.account_id.value(), membership.as_ref())
            .map_err(|_| Error::Unauthorized)?;
    }
    Pending::new(
        state,
        message,
        sender.account_id,
        vec![Query::Namespace, Query::Vault0, Query::Vault1],
    )
    .advance(input.receiver)
}
pub(crate) fn backed(pending: &Pending) -> Result<(), Error> {
    let collateral = pending
        .state
        .pool
        .real_reserve1
        .checked_add(pending.state.pool.fees_accrued)
        .ok_or(Error::Arithmetic)?;
    if pending.balances[0] < pending.state.pool.real_reserve0 || pending.balances[1] < collateral {
        return Err(Error::Underfunded);
    }
    Ok(())
}
pub(crate) fn payout(
    response: Response,
    state: &PoolAccount,
    receiver: Actor,
    definition: lee_core::account::AccountId,
    recipient: lee_core::account::AccountId,
    amount: u128,
) -> Response {
    if amount == 0 || recipient == receiver.account_id {
        return response;
    }
    response.send(
        SyncMessage::new(
            Actor::new(receiver.account_id, TOKEN_PROGRAM_ACCOUNT_ID),
            &lez_compat_core::token::Message::Transfer {
                definition,
                recipient,
                amount,
            },
        )
        .with_pda_seeds([crate::compute_pool_pda_seed(
            state.namespace,
            state.token0_definition_id,
            state.token1_definition_id,
            state.owner,
        )]),
    )
}
pub(crate) fn finish(mut pending: Pending, receiver: Actor) -> Result<Response, Error> {
    pending.state.validate_address(receiver)?;
    if pending.state.trade_control.is_paused() {
        return Err(Error::Paused);
    }
    let config = pending.config.as_ref().ok_or(Error::InvalidState)?;
    if config.emergency.is_frozen {
        return Err(Error::Frozen);
    }
    if !pending.state.funded {
        return Err(Error::PendingFunding);
    }
    let (token_in, now, form) = match pending.message {
        Message::SwapExactInput { token_in, now, .. } => (token_in, now, TradeFormV1::ExactInput),
        Message::SwapExactOutput { token_in, now, .. } => (token_in, now, TradeFormV1::ExactOutput),
        _ => return Err(Error::InvalidMessage),
    };
    let (side, token_out) = if token_in == pending.state.token0_definition_id {
        (TokenSide::Token0, pending.state.token1_definition_id)
    } else if token_in == pending.state.token1_definition_id {
        (TokenSide::Token1, pending.state.token0_definition_id)
    } else {
        return Err(Error::InvalidToken);
    };
    if pending
        .state
        .allowed_input_token
        .is_some_and(|allowed| allowed != side)
    {
        return Err(Error::Restricted);
    }
    backed(&pending)?;
    let bounds = pending
        .state
        .pool
        .trade_bounds()
        .map_err(|_| Error::InvalidPool)?;
    let outcome = match pending.message {
        Message::SwapExactInput {
            amount_in,
            min_amount_out,
            ..
        } => pending.state.pool.swap_exact_input(ExactInputSwap {
            token_in: side,
            amount_in,
            min_amount_out,
            pool_fee_bps: 0,
            protocol_fee_bps: config.protocol_fee_bps,
            now,
        }),
        Message::SwapExactOutput {
            amount_out,
            max_amount_in,
            ..
        } => pending.state.pool.swap_exact_output(ExactOutputSwap {
            token_in: side,
            amount_out,
            max_amount_in,
            pool_fee_bps: 0,
            protocol_fee_bps: config.protocol_fee_bps,
            now,
        }),
        _ => return Err(Error::InvalidMessage),
    }
    .map_err(|_| Error::InvalidPool)?;
    let closure = if pending.state.pool.lifecycle == pool::PoolLifecycle::Closed {
        let reason = pending.state.depletion_reason().ok_or(Error::InvalidPool)?;
        pending.state.observe_closure(receiver, reason, bounds)
    } else {
        None
    };
    let mut response = crate::with_time_bounds(
        Response::set_state(borsh::to_vec(&pending.state).unwrap()),
        bounds.at_least,
        bounds.before,
    )?;
    response = response.event(pending.state.event(
        receiver,
        SaleEventKindV1::Trade {
            trader: *pending.authority.value(),
            form,
            token_in: *token_in.value(),
            token_out: *token_out.value(),
            amount_in: outcome.amount_in,
            pricing_input: outcome.effective_amount_in,
            gross_output: outcome.raw_amount_out,
            amount_out: outcome.amount_out,
            fee_definition: *pending.state.token1_definition_id.value(),
            fee_amount: outcome.protocol_fee,
            fee_bps: config.protocol_fee_bps,
            retained_fee: outcome.retained_fee,
            sell_fee_bps: pending.state.pool.sell_fee_bps,
            pricing_time: None,
        },
    ));
    if let Some(event) = closure {
        response = response.event(event);
    }
    response = transfer(
        response,
        pending.authority,
        token_in,
        receiver.account_id,
        outcome.amount_in,
    );
    Ok(payout(
        response,
        &pending.state,
        receiver,
        token_out,
        pending.authority,
        outcome.amount_out,
    ))
}
