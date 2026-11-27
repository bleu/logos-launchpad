//! Bonding-curve sale callbacks and pool binding.
use super::*;

mod controls;
mod creation;
mod settlement;
pub(super) use controls::begin_pause;
pub(super) use creation::{begin, create};
pub(super) use settlement::{begin_claim, begin_settlement};

fn validate_pool(
    state: &FactoryState,
    actor: Actor,
    saved: &curve_core::PoolAccount,
) -> Result<(), Error> {
    if saved.pool.ending_policy != state.ending_policy
        || saved.pool.sell_fee_bps != state.sell_fee_bps
        || saved.block_allocation.limit != state.per_block_token_limit
    {
        return Err(Error::InvalidCustody);
    }
    saved
        .pool
        .validate_policy()
        .map_err(|_| Error::InvalidState)?;
    if saved.buy_policy != state.buy_policy
        || saved.trade_control.policy() != state.pause_policy
        || saved.namespace != state.namespace
        || saved.owner != actor.account_id
        || saved.owner_program
            != Some((
                actor.program_account_id,
                *compute_factory_seed(state.namespace, state.launch_salt).as_bytes(),
            ))
        || saved.token0_definition_id != state.token_definition_id
        || saved.token1_definition_id != state.collateral_definition_id
        || saved.pool.initial_k
            != state
                .virtual_token_reserve
                .checked_mul(state.virtual_collateral_reserve)
                .ok_or(Error::Overflow)?
        || saved.pool.close_timestamp != state.end_timestamp
        || saved.pool.close_on_depletion
            != match state.ending_policy {
                EndingPolicy::Scheduled => None,
                _ => Some(pool::TokenSide::Token0),
            }
        || saved.allowed_input_token
            != match state.trade_mode {
                TradeMode::BuyOnly => Some(pool::TokenSide::Token1),
                TradeMode::BuyAndSell => None,
            }
    {
        return Err(Error::InvalidState);
    }
    Ok(())
}

pub(super) fn resume(input: &ReceiveInput) -> Option<Result<Response, Error>> {
    let mut state: FactoryState = borsh::from_slice(&input.pre_state).ok()?;
    let mut pending = state.pending.take()?;
    Some((|| {
        state.validate(input.receiver)?;
        if matches!(pending.command, Message::ExecuteGraduationHook { .. })
            && pending.reads.is_empty()
        {
            return settlement::finish_graduation(input, state, pending);
        }
        if pending.proof {
            let creator = match pending.command {
                Message::CreateFactoryPool { creator, .. }
                | Message::ContinueCreation { creator, .. }
                | Message::ClaimCreatorAllocation { creator, .. }
                | Message::WithdrawFactoryProceeds { creator, .. }
                | Message::SetPaused { creator, .. } => creator,
                Message::ExecuteGraduationHook { creator_hint } => {
                    creator_hint.ok_or(Error::Unauthorized)?
                }
                _ => return Err(Error::InvalidState),
            };
            let role = Actor::new(state.creator_role_id, AUTHORITY_PROGRAM_ACCOUNT_ID);
            if matches!(pending.command, Message::ExecuteGraduationHook { .. }) {
                if authority::verify_resolved(input, role)? != creator {
                    return Err(Error::Unauthorized);
                }
            } else {
                authority::verify(input, role, creator)?;
            }
            if let Message::SetPaused { paused, .. } = pending.command {
                return Ok(controls::finish_pause(&state, paused));
            }
            pending.proof = false;
            state.pending = Some(pending);
            let mut response = state.response();
            if matches!(
                state.pending.as_ref().unwrap().command,
                Message::ContinueCreation { .. }
            ) {
                response = response
                    .send_sync(
                        Actor::new(state.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                        &token::Message::Initialize {
                            owner: input.receiver.account_id,
                        },
                    )
                    .send_sync(
                        Actor::new(state.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                        &token::Message::Initialize {
                            owner: state.creator_escrow_id,
                        },
                    );
            }
            response.sync_messages.extend(state.next().sync_messages);
            return Ok(response);
        }
        let expected = pending
            .reads
            .get(pending.values.len())
            .ok_or(Error::InvalidState)?;
        if input.from != Some(expected.actor) {
            return Err(Error::Unauthorized);
        }
        let reply: StateReply =
            borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
        pending.values.push(reply.state.to_vec());
        if pending.values.len() < pending.reads.len() {
            state.pending = Some(pending);
            return Ok(state.next());
        }
        match pending.command {
            Message::ClaimCreatorAllocation { creator, .. } => {
                settlement::finish_claim(input, state, creator, &pending.values)
            }
            command @ (Message::WithdrawFactoryProceeds { .. }
            | Message::ExecuteGraduationHook { .. }) => {
                settlement::finish_payout(input, state, command, &pending.values)
            }
            Message::ContinueCreation { now, .. } => {
                creation::finish_stage(input, state, now, &pending.values)
            }
            Message::CreateFactoryPool { creator, .. } => {
                creation::finish_mint(input, state, creator, &pending.values)
            }
            _ => Err(Error::InvalidMessage),
        }
    })())
}
