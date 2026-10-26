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
    if matches!(message, Message::ClosePool { .. }) {
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
    if let Message::ClosePool { now } = pending.message {
        pending
            .state
            .pool
            .close_pool(now)
            .map_err(|_| Error::Closed)?;
        return crate::pool_create::assert_clock(
            Response::set_state(borsh::to_vec(&pending.state).unwrap()),
            now,
            pending.state.pool.close_timestamp,
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
        let now = if pending.state.pool.lifecycle == pool::PoolLifecycle::Open {
            now.max(pending.state.pool.close_timestamp.ok_or(Error::Closed)?)
        } else {
            now
        };
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
        let response = crate::pool_create::assert_clock(
            Response::set_state(borsh::to_vec(&pending.state).unwrap()),
            now,
            None,
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
