use super::fixture::{
    HOLDER, NS, OWNER, PROGRAM, TOKEN0, TOKEN1, answer_reads, call, create_message, namespace,
    pool_id, pool_state, start,
};
use crate::{Message, PoolAccount, TOKEN_PROGRAM_ACCOUNT_ID, compute_pool_pda, handle};
use lee_core::account::{AccountId, Actor, ActorState};
use lee_core::program::{ReceiveInput, Response};

fn role_call(
    receiver: Actor,
    state: ActorState,
    from: Option<Actor>,
    authorized: bool,
    message: Vec<u8>,
) -> Result<Response, lez_compat_core::Error> {
    lez_compat_core::authority::handle(&ReceiveInput {
        receiver,
        pre_state: state,
        from,
        is_authorized: authorized,
        message,
    })
}

#[test]
fn direct_pool_role_transfer_requires_consent_and_preserves_pool_identity() {
    use lez_compat_core::authority::{self, Message as RoleMessage};
    let mut message = create_message(false);
    if let Message::CreatePool { pause_policy, .. } = &mut message {
        *pause_policy = crate::PausePolicy::AuthorityControlled;
    }
    let pending = handle(&call(
        pool_id(),
        ActorState::empty(),
        Some(Actor::new(OWNER, PROGRAM)),
        message,
    ))
    .unwrap();
    let created = answer_reads(pending, &pool_state(), namespace(), [0, 0]).unwrap();
    let pool_bytes = created.post_state.unwrap();
    let bootstrap = created.sync_messages.last().unwrap();
    let role_actor = bootstrap.to;
    let initial = role_call(
        role_actor,
        ActorState::empty(),
        Some(Actor::new(pool_id(), PROGRAM)),
        true,
        bootstrap.message.clone(),
    )
    .unwrap();
    let consent = &initial.sync_messages[0];
    assert!(
        role_call(
            consent.to,
            ActorState::empty(),
            Some(role_actor),
            false,
            consent.message.clone()
        )
        .is_err()
    );
    let accepted = role_call(
        consent.to,
        ActorState::empty(),
        Some(role_actor),
        true,
        consent.message.clone(),
    )
    .unwrap();
    let installed = role_call(
        role_actor,
        initial.post_state.unwrap(),
        Some(consent.to),
        false,
        accepted.sync_messages[0].message.clone(),
    )
    .unwrap()
    .post_state
    .unwrap();
    let transfer = borsh::to_vec(&RoleMessage::Transfer {
        role: role_actor.account_id,
        successor: HOLDER,
    })
    .unwrap();
    assert!(
        role_call(
            consent.to,
            ActorState::empty(),
            None,
            false,
            transfer.clone()
        )
        .is_err()
    );
    let proposed = role_call(consent.to, ActorState::empty(), None, true, transfer).unwrap();
    let proposal = role_call(
        role_actor,
        installed.clone(),
        Some(consent.to),
        false,
        proposed.sync_messages[0].message.clone(),
    )
    .unwrap();
    let successor = &proposal.sync_messages[0];
    assert!(
        role_call(
            successor.to,
            ActorState::empty(),
            Some(role_actor),
            false,
            successor.message.clone()
        )
        .is_err()
    );
    let approved = role_call(
        successor.to,
        ActorState::empty(),
        Some(role_actor),
        true,
        successor.message.clone(),
    )
    .unwrap();
    let transferred = role_call(
        role_actor,
        proposal.post_state.unwrap(),
        Some(successor.to),
        false,
        approved.sync_messages[0].message.clone(),
    )
    .unwrap()
    .post_state
    .unwrap();
    for caller in [OWNER, NS, AccountId::new([92; 32])] {
        let request = handle(&call(
            pool_id(),
            pool_bytes.clone(),
            Some(Actor::new(caller, PROGRAM)),
            Message::SetPaused { paused: true },
        ))
        .unwrap();
        let check = &request.sync_messages[0];
        let assertion = role_call(
            check.to,
            ActorState::empty(),
            Some(Actor::new(pool_id(), PROGRAM)),
            true,
            check.message.clone(),
        )
        .unwrap();
        assert!(
            role_call(
                role_actor,
                transferred.clone(),
                Some(check.to),
                false,
                assertion.sync_messages[0].message.clone()
            )
            .is_err()
        );
    }
    let request = handle(&call(
        pool_id(),
        pool_bytes.clone(),
        Some(Actor::new(HOLDER, PROGRAM)),
        Message::SetPaused { paused: true },
    ))
    .unwrap();
    let check = &request.sync_messages[0];
    let assertion = role_call(
        check.to,
        ActorState::empty(),
        Some(Actor::new(pool_id(), PROGRAM)),
        true,
        check.message.clone(),
    )
    .unwrap();
    let proof = role_call(
        role_actor,
        transferred.clone(),
        Some(check.to),
        false,
        assertion.sync_messages[0].message.clone(),
    )
    .unwrap();
    let authorized: authority::Authorized =
        borsh::from_slice(&proof.sync_messages[0].message).unwrap();
    assert_eq!(authorized.authority, HOLDER);
    let closed = handle(&ReceiveInput {
        receiver: proof.sync_messages[0].to,
        pre_state: request.post_state.unwrap(),
        from: Some(role_actor),
        is_authorized: false,
        message: proof.sync_messages[0].message.clone(),
    })
    .unwrap();
    let state = PoolAccount::decode(&closed.post_state.unwrap()).unwrap();
    assert_eq!(state.owner, OWNER);
    assert_eq!(state.namespace, NS);
    assert_eq!(state.pool.lifecycle, pool::PoolLifecycle::Open);
    assert!(state.trade_control.is_paused());
    assert!(
        state
            .validate_address(Actor::new(pool_id(), PROGRAM))
            .is_ok()
    );
    let revoke = role_call(
        successor.to,
        ActorState::empty(),
        None,
        true,
        borsh::to_vec(&RoleMessage::Renounce {
            role: role_actor.account_id,
        })
        .unwrap(),
    )
    .unwrap();
    assert!(
        role_call(
            role_actor,
            transferred,
            Some(successor.to),
            false,
            revoke.sync_messages[0].message.clone()
        )
        .is_err()
    );
}

#[test]
fn pool_owner_proofs_reject_wrong_namespace_role_holder_program_and_callback() {
    use crate::{AUTHORITY_PROGRAM_ACCOUNT_ID, compute_pool_owner_role};
    use lez_compat_core::authority::Authorized;
    let state = pool_state();
    let pending = start(&state, Message::ClosePool { now: 1 })
        .unwrap()
        .post_state
        .unwrap();
    let role = compute_pool_owner_role(NS, pool_id(), PROGRAM);
    let valid = Authorized {
        role,
        authority: HOLDER,
    };
    let wrong_namespace = compute_pool_owner_role(AccountId::new([93; 32]), pool_id(), PROGRAM);
    let admin_role = crate::compute_admin_role(NS, PROGRAM);
    for (receiver, sender, proof) in [
        (pool_id(), Actor::new(role, PROGRAM), valid.clone()),
        (
            pool_id(),
            Actor::new(admin_role, AUTHORITY_PROGRAM_ACCOUNT_ID),
            Authorized {
                role: admin_role,
                authority: HOLDER,
            },
        ),
        (
            pool_id(),
            Actor::new(wrong_namespace, AUTHORITY_PROGRAM_ACCOUNT_ID),
            Authorized {
                role: wrong_namespace,
                authority: HOLDER,
            },
        ),
        (
            pool_id(),
            Actor::new(role, AUTHORITY_PROGRAM_ACCOUNT_ID),
            Authorized {
                role: admin_role,
                authority: HOLDER,
            },
        ),
        (
            pool_id(),
            Actor::new(role, AUTHORITY_PROGRAM_ACCOUNT_ID),
            Authorized {
                role,
                authority: OWNER,
            },
        ),
        (
            AccountId::new([94; 32]),
            Actor::new(role, AUTHORITY_PROGRAM_ACCOUNT_ID),
            valid,
        ),
    ] {
        assert!(
            handle(&ReceiveInput {
                receiver: Actor::new(receiver, PROGRAM),
                from: Some(sender),
                is_authorized: false,
                pre_state: pending.clone(),
                message: borsh::to_vec(&proof).unwrap(),
            })
            .is_err()
        );
    }
    let legacy = lez_compat_core::token::NftHolder {
        definition: OWNER,
        owner: HOLDER,
    };
    assert!(
        handle(&ReceiveInput {
            receiver: Actor::new(pool_id(), PROGRAM),
            from: Some(Actor::new(
                lez_compat_core::token::holding_address(TOKEN_PROGRAM_ACCOUNT_ID, HOLDER, OWNER),
                TOKEN_PROGRAM_ACCOUNT_ID
            )),
            is_authorized: false,
            pre_state: pending,
            message: borsh::to_vec(&legacy).unwrap(),
        })
        .is_err()
    );
}

#[test]
fn owner_actions_check_the_separate_stable_pool_role() {
    let state = pool_state();
    let response = start(&state, Message::ClosePool { now: 1 }).unwrap();
    let request = &response.sync_messages[0];
    assert_eq!(
        request.to,
        Actor::new(HOLDER, crate::AUTHORITY_PROGRAM_ACCOUNT_ID)
    );
    assert_eq!(
        borsh::from_slice::<lez_compat_core::authority::Message>(&request.message).unwrap(),
        lez_compat_core::authority::Message::Check {
            role: crate::compute_pool_owner_role(NS, pool_id(), PROGRAM),
            callback: Actor::new(pool_id(), PROGRAM),
        }
    );
}

#[test]
fn factory_pool_close_requires_the_exact_derived_owner_actor() {
    let owner_program = AccountId::new([61; 32]);
    let seed = [62; 32];
    let owner = AccountId::for_public_pda(&owner_program, &lee_core::program::PdaSeed::new(seed));
    let mut state = pool_state();
    state.owner = owner;
    state.owner_program = Some((owner_program, seed));
    let id = compute_pool_pda(NS, PROGRAM, TOKEN0, TOKEN1, owner);
    let request = call(
        id,
        borsh::to_vec(&state).unwrap().into(),
        Some(Actor::new(owner, owner_program)),
        Message::ClosePool { now: 1 },
    );
    let response = handle(&request).unwrap();
    assert_eq!(
        PoolAccount::decode(&response.post_state.unwrap())
            .unwrap()
            .pool
            .lifecycle,
        pool::PoolLifecycle::Closed
    );
    assert!(response.sync_messages.is_empty());
    assert_eq!(response.validity.timestamps.end(), Some(42_000));
    for sender in [
        Actor::new(owner, PROGRAM),
        Actor::new(HOLDER, owner_program),
    ] {
        let mut wrong = request.clone();
        wrong.from = Some(sender);
        assert!(matches!(handle(&wrong), Err(crate::Error::Unauthorized)));
    }
    state.owner_program = Some((owner_program, [63; 32]));
    let mut wrong = request;
    wrong.pre_state = borsh::to_vec(&state).unwrap().into();
    assert!(matches!(handle(&wrong), Err(crate::Error::Unauthorized)));
}

#[test]
fn pool_close_rights_follow_the_current_role_holder() {
    let state = pool_state();
    let response = answer_reads(
        start(&state, Message::ClosePool { now: 1 }).unwrap(),
        &state,
        namespace(),
        [800, 100],
    )
    .unwrap();
    let closed = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
    assert_eq!(closed.pool.lifecycle, pool::PoolLifecycle::Closed);
    assert_eq!(closed.owner, OWNER);
    assert_eq!(response.validity.timestamps.start(), None);
    assert_eq!(response.validity.timestamps.end(), Some(42_000));
    assert!(response.sync_messages.is_empty());
}
