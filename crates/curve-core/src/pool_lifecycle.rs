//! Owner lifecycle and permissionless protocol-fee collection.
use crate::{
    Error, Message, PoolAccount,
    dispatch::{Pending, Query},
    pool_swap::payout,
};
use lee_core::{
    account::Actor,
    program::{ReceiveInput, Response},
};

pub fn begin(input: &ReceiveInput, message: Message) -> Result<Response, Error> {
    let state = PoolAccount::decode(&input.pre_state)?;
    state.validate_address(input.receiver)?;
    if !state.funded {
        return Err(Error::PendingFunding);
    }
    if matches!(message, Message::WithdrawReserves { .. }) {
        let (authority, mut queries) = crate::authority::owner(input, &state)?;
        queries.extend([Query::Namespace, Query::Vault0, Query::Vault1]);
        return Pending::new(state, message, authority, queries).advance(input.receiver);
    }
    if matches!(
        message,
        Message::ClosePool { .. } | Message::SetPaused { .. }
    ) {
        let (authority, queries) = crate::authority::owner(input, &state)?;
        return Pending::new(state, message, authority, queries).advance(input.receiver);
    }
    Pending::new(
        state,
        message,
        input.receiver.account_id,
        vec![Query::Namespace, Query::Vault1],
    )
    .advance(input.receiver)
}
pub(crate) fn finish(mut pending: Pending, receiver: Actor) -> Result<Response, Error> {
    pending.state.validate_address(receiver)?;
    if let Message::SetPaused { paused } = pending.message {
        let bounds = pending
            .state
            .pool
            .trade_bounds()
            .map_err(|_| Error::Closed)?;
        pending
            .state
            .trade_control
            .set_paused(paused)
            .map_err(|_| Error::PauseDisabled)?;
        return crate::with_time_bounds(
            Response::set_state(borsh::to_vec(&pending.state).unwrap()),
            bounds.at_least,
            bounds.before,
        );
    }
    if let Message::ClosePool { now } = pending.message {
        let bounds = pending
            .state
            .pool
            .trade_bounds()
            .map_err(|_| Error::Closed)?;
        pending
            .state
            .pool
            .close_pool(now)
            .map_err(|_| Error::Closed)?;
        return crate::with_time_bounds(
            Response::set_state(borsh::to_vec(&pending.state).unwrap()),
            bounds.at_least,
            bounds.before,
        );
    }
    let config = pending.config.as_ref().ok_or(Error::InvalidState)?;
    if pending.balances[1]
        < pending
            .state
            .pool
            .real_reserve1
            .checked_add(pending.state.pool.fees_accrued)
            .ok_or(Error::Arithmetic)?
    {
        return Err(Error::Underfunded);
    }
    if let Message::WithdrawReserves { now } = pending.message {
        crate::pool_swap::backed(&pending)?;
        let bounds = pending
            .state
            .pool
            .ending_decision()
            .map_err(|_| Error::Closed)?
            .inclusion_bounds();
        let now = bounds.at_least.map_or(now, |end| now.max(end));
        let (reserve0, reserve1) = pending
            .state
            .pool
            .withdraw_reserves(now)
            .map_err(|_| Error::Closed)?;
        let fees = pending
            .state
            .pool
            .collect_fees()
            .map_err(|_| Error::Arithmetic)?;
        let response = crate::with_time_bounds(
            Response::set_state(borsh::to_vec(&pending.state).unwrap()),
            bounds.at_least,
            bounds.before,
        )?;
        let response = payout(
            response,
            &pending.state,
            receiver,
            pending.state.token1_definition_id,
            config.treasury,
            fees,
        );
        let response = payout(
            response,
            &pending.state,
            receiver,
            pending.state.token0_definition_id,
            pending.authority,
            reserve0,
        );
        return Ok(payout(
            response,
            &pending.state,
            receiver,
            pending.state.token1_definition_id,
            pending.authority,
            reserve1,
        ));
    }
    let fees = pending
        .state
        .pool
        .collect_fees()
        .map_err(|_| Error::Arithmetic)?;
    Ok(payout(
        Response::set_state(borsh::to_vec(&pending.state).unwrap()),
        &pending.state,
        receiver,
        pending.state.token1_definition_id,
        config.treasury,
        fees,
    ))
}
