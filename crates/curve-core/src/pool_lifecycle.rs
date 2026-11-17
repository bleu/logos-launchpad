//! Owner lifecycle and permissionless protocol-fee collection.
use crate::{
    EndingDecision, Error, Message, PoolAccount,
    dispatch::{Pending, Query},
    pool_swap::payout,
    program_events::transferred_amount,
};
use lee_core::{
    account::Actor,
    program::{ReceiveInput, Response},
};
use sale_policy::events::{
    ClosureReasonV1, PayoutPurposeV1, SaleEventKindV1, SettlementEffectV1, SettlementStepV1,
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
        let changed = pending
            .state
            .trade_control
            .set_paused(paused)
            .map_err(|_| Error::PauseDisabled)?;
        let response = crate::with_time_bounds(
            Response::set_state(borsh::to_vec(&pending.state).unwrap()),
            bounds.at_least,
            bounds.before,
        )?;
        return Ok(if changed {
            response.event(
                pending
                    .state
                    .event(receiver, SaleEventKindV1::PauseChanged { paused }),
            )
        } else {
            response
        });
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
        let event = pending
            .state
            .observe_closure(receiver, ClosureReasonV1::Manual, bounds);
        let response = crate::with_time_bounds(
            Response::set_state(borsh::to_vec(&pending.state).unwrap()),
            bounds.at_least,
            bounds.before,
        )?;
        return Ok(if let Some(event) = event {
            response.event(event)
        } else {
            response
        });
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
        let decision = pending
            .state
            .pool
            .ending_decision()
            .map_err(|_| Error::Closed)?;
        let bounds = decision.inclusion_bounds();
        let reason = match decision {
            EndingDecision::ScheduledEnd { timestamp } => {
                ClosureReasonV1::ScheduledEnd { timestamp }
            }
            EndingDecision::Depleted => {
                pending.state.depletion_reason().ok_or(Error::InvalidPool)?
            }
            EndingDecision::Manual => pending
                .state
                .depletion_reason()
                .unwrap_or(ClosureReasonV1::Manual),
        };
        let closure = pending.state.observe_closure(receiver, reason, bounds);
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
        let mut response = crate::with_time_bounds(
            Response::set_state(borsh::to_vec(&pending.state).unwrap()),
            bounds.at_least,
            bounds.before,
        )?;
        if let Some(event) = closure {
            response = response.event(event);
        }
        if let Some(event) = pending.state.fees_event(receiver, config.treasury, fees) {
            response = response.event(event);
        }
        let mut effects = Vec::new();
        for (definition, amount) in [
            (pending.state.token0_definition_id, reserve0),
            (pending.state.token1_definition_id, reserve1),
        ] {
            if amount != 0 {
                effects.push(SettlementEffectV1::Payout {
                    definition: *definition.value(),
                    recipient: *pending.authority.value(),
                    purpose: PayoutPurposeV1::Reserve,
                    liability_discharged: amount,
                    tokens_transferred: transferred_amount(receiver, pending.authority, amount),
                });
            }
        }
        response = response.event(pending.state.event(
            receiver,
            SaleEventKindV1::Settlement {
                step: SettlementStepV1::ReservesWithdrawn,
                route: None,
                effects,
            },
        ));
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
    let mut response = Response::set_state(borsh::to_vec(&pending.state).unwrap());
    if let Some(event) = pending.state.fees_event(receiver, config.treasury, fees) {
        response = response.event(event);
    }
    Ok(payout(
        response,
        &pending.state,
        receiver,
        pending.state.token1_definition_id,
        config.treasury,
        fees,
    ))
}
