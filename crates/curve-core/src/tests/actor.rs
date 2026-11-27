use super::fixture::{
    HOLDER, NS, PROGRAM, TOKEN0, TOKEN1, call, create_message, namespace, pool_id, pool_state,
    reply_to, start,
};
use crate::{Config, Message, handle};
use lee_core::account::{AccountId, Actor, ActorState};
use lee_core::program::{ReceiveInput, StateReply};
use lez_compat_core::token::{TokenKind, TokenState};

#[test]
fn pool_read_replies_with_canonical_state_to_the_requesting_actor() {
    let state = pool_state();
    let response = start(&state, Message::ReadState).unwrap();
    assert!(response.post_state.is_none());
    assert_eq!(response.sync_messages.len(), 1);
    let reply = &response.sync_messages[0];
    assert_eq!(reply.to, Actor::new(HOLDER, PROGRAM));
    assert_eq!(
        borsh::from_slice::<StateReply>(&reply.message)
            .unwrap()
            .state,
        borsh::to_vec(&state).unwrap().into()
    );
    let foreign = call(
        AccountId::new([71; 32]),
        borsh::to_vec(&state).unwrap().into(),
        Some(Actor::new(HOLDER, PROGRAM)),
        Message::ReadState,
    );
    assert!(matches!(handle(&foreign), Err(crate::Error::WrongAccount)));
}

#[test]
fn namespace_and_vault_replies_require_the_exact_account_and_program() {
    let state = pool_state();
    let response = start(
        &state,
        Message::SwapExactInput {
            amount_in: 250,
            min_amount_out: 0,
            token_in: TOKEN0,
            membership: None,
            now: 1,
        },
    )
    .unwrap();
    let query = &response.sync_messages[0];
    let pending = response.post_state.unwrap();
    let bytes = borsh::to_vec(&StateReply { state: namespace() }).unwrap();
    for origin in [
        Actor::new(query.to.account_id, AccountId::new([77; 32])),
        Actor::new(AccountId::new([78; 32]), query.to.program_account_id),
    ] {
        assert!(matches!(
            handle(&ReceiveInput {
                receiver: Actor::new(pool_id(), PROGRAM),
                from: Some(origin),
                is_authorized: false,
                pre_state: pending.clone(),
                message: bytes.clone()
            }),
            Err(crate::Error::Unauthorized)
        ));
    }
    let mut foreign = Config::decode(&namespace()).unwrap();
    foreign.namespace = AccountId::new([79; 32]);
    assert!(matches!(
        reply_to(
            pending.clone(),
            query,
            borsh::to_vec(&StateReply {
                state: borsh::to_vec(&foreign).unwrap().into()
            })
            .unwrap()
        ),
        Err(crate::Error::WrongAccount)
    ));
    let response = reply_to(pending, query, bytes).unwrap();
    let vault_query = &response.sync_messages[0];
    let malformed = TokenState::Holding {
        definition: TOKEN0,
        owner: HOLDER,
        kind: TokenKind::Fungible,
        balance: 10000,
    };
    assert!(matches!(
        reply_to(
            response.post_state.unwrap(),
            vault_query,
            borsh::to_vec(&StateReply {
                state: borsh::to_vec(&malformed).unwrap().into()
            })
            .unwrap()
        ),
        Err(crate::Error::InvalidToken)
    ));
}

#[test]
fn signed_pool_entry_cannot_be_used_without_authorization_or_for_namespace_control() {
    for instruction in [
        create_message(false),
        Message::SetPaused { paused: true },
        Message::ActivatePool { now: 1 },
        Message::ClosePool { now: 1 },
        Message::WithdrawReserves { now: 42 },
        Message::SwapExactInput {
            amount_in: 1,
            min_amount_out: 0,
            token_in: TOKEN1,
            membership: None,
            now: 1,
        },
        Message::SwapExactOutput {
            amount_out: 1,
            max_amount_in: 1,
            token_in: TOKEN1,
            membership: None,
            now: 1,
        },
    ] {
        let mut request = call(
            HOLDER,
            ActorState::empty(),
            None,
            Message::Execute {
                pool: pool_id(),
                instruction: Box::new(instruction.clone()),
            },
        );
        assert!(matches!(handle(&request), Err(crate::Error::Unauthorized)));
        request.is_authorized = true;
        let forward = handle(&request).unwrap().sync_messages.remove(0);
        assert_eq!(forward.to, Actor::new(pool_id(), PROGRAM));
        assert_eq!(
            borsh::from_slice::<Message>(&forward.message).unwrap(),
            instruction
        );
    }
    let mut request = call(
        HOLDER,
        ActorState::empty(),
        None,
        Message::Execute {
            pool: pool_id(),
            instruction: Box::new(Message::SetFrozen {
                namespace: NS,
                frozen: true,
            }),
        },
    );
    request.is_authorized = true;
    assert!(matches!(handle(&request), Err(crate::Error::Unauthorized)));
}
