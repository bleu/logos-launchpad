//! Pool creation preserves the bounded AMM and token custody rules.
use crate::{
    Error, Message, PoolAccount, TOKEN_PROGRAM_ACCOUNT_ID,
    dispatch::{Pending, Query},
};
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response},
};
use lez_compat_core::token;
use pool::{Pool, PoolCreateParams, TokenSide};

pub(crate) fn side(side: Option<u8>) -> Result<Option<TokenSide>, Error> {
    match side {
        None => Ok(None),
        Some(0) => Ok(Some(TokenSide::Token0)),
        Some(1) => Ok(Some(TokenSide::Token1)),
        _ => Err(Error::InvalidPool),
    }
}
pub fn begin(input: &ReceiveInput, message: Message) -> Result<Response, Error> {
    let Message::CreatePool {
        namespace,
        token0_definition_id,
        token1_definition_id,
        allowed_input_token,
        defer_funding,
        token0_amount,
        token1_amount,
        virtual_reserve0,
        virtual_reserve1,
        close_timestamp,
        close_on_depletion,
        owner,
        owner_program_id,
        owner_program_seed,
        curve_program_id,
        now,
    } = message.clone()
    else {
        return Err(Error::InvalidMessage);
    };
    if !input.pre_state.is_empty() {
        return Err(Error::AlreadyInitialized);
    }
    if curve_program_id != input.receiver.program_account_id {
        return Err(Error::WrongAccount);
    }
    if owner == AccountId::default() || token0_definition_id == token1_definition_id {
        return Err(Error::InvalidPool);
    }
    if !defer_funding && close_timestamp.is_some_and(|end| now >= end) {
        return Err(Error::Closed);
    }
    let owner_program = match (owner_program_id, owner_program_seed) {
        (None, None) => None,
        (Some(p), Some(s)) if p != AccountId::default() => Some((p, s)),
        _ => return Err(Error::InvalidPool),
    };
    let sender = input.from.ok_or(Error::Unauthorized)?;
    match owner_program {
        Some((program, seed)) => {
            if AccountId::for_public_pda(&program, &lee_core::program::PdaSeed::new(seed)) != owner
            {
                return Err(Error::Unauthorized);
            }
            if input.receiver.account_id == owner {
                if !input.is_authorized || sender != Actor::new(owner, program) {
                    return Err(Error::Unauthorized);
                }
                let seed = crate::compute_pool_pda_seed(
                    namespace,
                    token0_definition_id,
                    token1_definition_id,
                    owner,
                );
                return Ok(Response::keep_state().send(
                    lee_core::program::SyncMessage::new(
                        Actor::new(
                            AccountId::for_public_pda(&curve_program_id, &seed),
                            curve_program_id,
                        ),
                        &message,
                    )
                    .with_pda_seeds([seed]),
                ));
            }
            if sender != Actor::new(owner, curve_program_id) {
                return Err(Error::Unauthorized);
            }
        }
        None if sender == Actor::new(owner, input.receiver.program_account_id) => (),
        None => return Err(Error::Unauthorized),
    }
    let state = PoolAccount {
        allowed_input_token: side(allowed_input_token)?,
        funded: !defer_funding,
        namespace,
        token0_definition_id,
        token1_definition_id,
        owner,
        owner_program,
        pool: Pool::create(PoolCreateParams {
            token0_amount,
            token1_amount,
            virtual_reserve0,
            virtual_reserve1,
            close_timestamp,
            close_on_depletion: side(close_on_depletion)?,
        })
        .map_err(|_| Error::InvalidPool)?,
    };
    state.validate_address(input.receiver)?;
    let mut queries = vec![Query::Namespace, Query::Definition0, Query::Definition1];
    if owner_program.is_some() && !defer_funding {
        queries.extend([Query::Vault0, Query::Vault1]);
    }
    Pending::new(state, message, sender.account_id, queries).advance(input.receiver)
}
pub(crate) fn assert_clock(
    response: Response,
    now: u64,
    before: Option<u64>,
) -> Result<Response, Error> {
    Ok(response.send_sync(
        Actor::new(
            clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
            clock_core::clock_account_id(),
        ),
        &crate::clock_assertion(now, before)?,
    ))
}
pub(crate) fn transfer(
    response: Response,
    owner: AccountId,
    definition: AccountId,
    recipient: AccountId,
    amount: u128,
) -> Response {
    if amount == 0 || owner == recipient {
        return response;
    }
    response.send_sync(
        Actor::new(owner, TOKEN_PROGRAM_ACCOUNT_ID),
        &token::Message::Transfer {
            definition,
            recipient,
            amount,
        },
    )
}
pub fn activate(input: &ReceiveInput, message: Message) -> Result<Response, Error> {
    let state = PoolAccount::decode(&input.pre_state)?;
    state.validate_address(input.receiver)?;
    if state.funded {
        return Err(Error::AlreadyInitialized);
    }
    let (authority, mut queries) = crate::authority::owner(input, &state)?;
    queries.extend([Query::Namespace, Query::Vault0, Query::Vault1]);
    Pending::new(state, message, authority, queries).advance(input.receiver)
}
pub(crate) fn finish(mut pending: Pending, receiver: Actor) -> Result<Response, Error> {
    if let Message::ActivatePool { now } = pending.message {
        if pending.state.owner_program.is_some()
            && (pending.balances[0] < pending.state.pool.real_reserve0
                || pending.balances[1] < pending.state.pool.real_reserve1)
        {
            return Err(Error::Underfunded);
        }
        pending.state.funded = true;
        if pending
            .state
            .pool
            .close_timestamp
            .is_some_and(|end| now >= end)
        {
            pending.state.pool.lifecycle = pool::PoolLifecycle::Closed;
        }
        let mut response = assert_clock(
            Response::set_state(borsh::to_vec(&pending.state).unwrap()),
            now,
            None,
        )?;
        if pending.state.owner_program.is_none() {
            response = transfer(
                response,
                pending.authority,
                pending.state.token0_definition_id,
                receiver.account_id,
                pending.state.pool.real_reserve0,
            );
            response = transfer(
                response,
                pending.authority,
                pending.state.token1_definition_id,
                receiver.account_id,
                pending.state.pool.real_reserve1,
            );
        }
        return Ok(response);
    }
    let Message::CreatePool {
        defer_funding, now, ..
    } = pending.message
    else {
        return Err(Error::InvalidMessage);
    };
    if pending.state.owner_program.is_some()
        && !defer_funding
        && (pending.balances[0] < pending.state.pool.real_reserve0
            || pending.balances[1] < pending.state.pool.real_reserve1)
    {
        return Err(Error::Underfunded);
    }
    let before = if defer_funding {
        None
    } else {
        pending.state.pool.close_timestamp
    };
    let mut response = assert_clock(
        Response::set_state(borsh::to_vec(&pending.state).unwrap()),
        now,
        before,
    )?;
    for (definition, amount) in [
        (
            pending.state.token0_definition_id,
            pending.state.pool.real_reserve0,
        ),
        (
            pending.state.token1_definition_id,
            pending.state.pool.real_reserve1,
        ),
    ] {
        response = response.send_sync(
            Actor::new(definition, TOKEN_PROGRAM_ACCOUNT_ID),
            &token::Message::Initialize {
                owner: receiver.account_id,
            },
        );
        if !defer_funding && pending.state.owner_program.is_none() {
            response = transfer(
                response,
                pending.authority,
                definition,
                receiver.account_id,
                amount,
            );
        }
    }
    if pending.state.owner_program.is_none() {
        response = response.send(
            lee_core::program::SyncMessage::new(
                Actor::new(
                    crate::compute_pool_owner_role(
                        pending.state.namespace,
                        receiver.account_id,
                        receiver.program_account_id,
                    ),
                    crate::AUTHORITY_PROGRAM_ACCOUNT_ID,
                ),
                &lez_compat_core::authority::Message::BootstrapTransferOnly {
                    authority: pending.state.owner,
                },
            )
            .with_pda_seeds([crate::compute_pool_owner_role_seed(
                pending.state.namespace,
                receiver.account_id,
            )]),
        );
    }
    Ok(response)
}
