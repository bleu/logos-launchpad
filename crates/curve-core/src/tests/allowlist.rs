use super::fixture::{
    HOLDER, NS, OWNER, PROGRAM, TOKEN0, TOKEN1, answer_reads, call, create_message, namespace,
    pool_id, pool_state, start,
};
use crate::{Message, PoolAccount, handle};
use lee_core::account::{AccountId, Actor, ActorState};

#[test]
fn allowlist_creation_rejects_a_different_sale_policy_id() {
    let mut message = create_message(false);
    if let Message::CreatePool { buy_policy, .. } = &mut message {
        *buy_policy = crate::BuyPolicy::Allowlist(crate::AllowlistPolicy {
            version: 1,
            namespace: *NS.value(),
            sale_policy_id: [99; 32],
            root: [8; 32],
        });
    }
    assert!(matches!(
        handle(&call(
            pool_id(),
            ActorState::empty(),
            Some(Actor::new(OWNER, PROGRAM)),
            message
        )),
        Err(crate::Error::InvalidPool)
    ));
}

#[test]
fn allowlist_buy_without_membership_fails_before_namespace_or_token_calls() {
    let mut state = pool_state();
    state.buy_policy = crate::BuyPolicy::Allowlist(crate::AllowlistPolicy {
        version: 1,
        namespace: *NS.value(),
        sale_policy_id: *pool_id().value(),
        root: [8; 32],
    });
    assert!(matches!(
        start(
            &state,
            Message::SwapExactInput {
                amount_in: 28,
                min_amount_out: 0,
                token_in: TOKEN1,
                membership: None,
                now: 1,
            }
        ),
        Err(crate::Error::Unauthorized)
    ));
}

fn allowlist_for(trader: AccountId) -> (crate::BuyPolicy, crate::MembershipPath) {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let path = crate::MembershipPath {
        index: 0,
        salt: [5; 32],
        siblings: [[17; 32]; 17],
    };
    let context = [&[1][..], &NS.value()[..], &pool_id().value()[..]].concat();
    let hash = |parts: &[&[u8]]| -> [u8; 32] {
        Impl::hash_bytes(&parts.concat())
            .as_bytes()
            .try_into()
            .unwrap()
    };
    let mut root = hash(&[
        b"LP/allowlist/leaf/v1\0",
        &context,
        trader.value(),
        &path.salt,
    ]);
    for sibling in &path.siblings {
        root = hash(&[b"LP/allowlist/node/v1\0", &context, &root, sibling]);
    }
    (
        crate::BuyPolicy::Allowlist(crate::AllowlistPolicy {
            version: 1,
            namespace: *NS.value(),
            sale_policy_id: *pool_id().value(),
            root,
        }),
        path,
    )
}

#[test]
fn allowlist_rejects_the_pool_as_its_own_trader() {
    let mut state = pool_state();
    let (policy, path) = allowlist_for(pool_id());
    state.buy_policy = policy;
    assert!(matches!(
        handle(&call(
            pool_id(),
            borsh::to_vec(&state).unwrap().into(),
            Some(Actor::new(pool_id(), PROGRAM)),
            Message::SwapExactInput {
                amount_in: 28,
                min_amount_out: 0,
                token_in: TOKEN1,
                membership: Some(path),
                now: 1,
            },
        )),
        Err(crate::Error::Unauthorized)
    ));
}

#[test]
fn allowlist_exact_input_and_output_buys_can_reuse_membership() {
    let mut state = pool_state();
    let (policy, path) = allowlist_for(HOLDER);
    state.buy_policy = policy.clone();
    for message in [
        Message::SwapExactInput {
            amount_in: 28,
            min_amount_out: 1,
            token_in: TOKEN1,
            membership: Some(path.clone()),
            now: 1,
        },
        Message::SwapExactOutput {
            amount_out: 10,
            max_amount_in: 100,
            token_in: TOKEN1,
            membership: Some(path.clone()),
            now: 1,
        },
    ] {
        let before = state.pool.real_reserve0;
        let response = answer_reads(
            start(&state, message).unwrap(),
            &state,
            namespace(),
            [
                state.pool.real_reserve0,
                state.pool.real_reserve1 + state.pool.fees_accrued,
            ],
        )
        .unwrap();
        state = PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(state.buy_policy, policy);
        assert!(state.pool.real_reserve0 < before);
        assert!(response.sync_messages.iter().any(|call| matches!(
            borsh::from_slice::<lez_compat_core::token::Message>(&call.message),
            Ok(lez_compat_core::token::Message::Transfer {
                definition: TOKEN0,
                recipient: HOLDER,
                ..
            })
        )));
    }
}

#[test]
fn allowlist_copied_membership_cannot_buy_for_another_trader() {
    let mut state = pool_state();
    let (policy, path) = allowlist_for(OWNER);
    state.buy_policy = policy;
    for membership in [None, Some(path)] {
        for message in [
            Message::SwapExactInput {
                amount_in: 28,
                min_amount_out: 0,
                token_in: TOKEN1,
                membership: membership.clone(),
                now: 1,
            },
            Message::SwapExactOutput {
                amount_out: 10,
                max_amount_in: 100,
                token_in: TOKEN1,
                membership: membership.clone(),
                now: 1,
            },
        ] {
            assert!(matches!(
                start(&state, message),
                Err(crate::Error::Unauthorized)
            ));
        }
    }
}

#[test]
fn allowlist_does_not_gate_permitted_sells() {
    let mut state = pool_state();
    state.buy_policy = allowlist_for(OWNER).0;
    let message = Message::SwapExactInput {
        amount_in: 250,
        min_amount_out: 1,
        token_in: TOKEN0,
        membership: None,
        now: 1,
    };
    let response = answer_reads(
        start(&state, message.clone()).unwrap(),
        &state,
        namespace(),
        [800, 100],
    )
    .unwrap();
    let sold = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
    assert!(sold.pool.real_reserve0 > 800);
    assert_eq!(sold.buy_policy, state.buy_policy);
    state.allowed_input_token = Some(pool::TokenSide::Token1);
    assert!(matches!(
        answer_reads(
            start(&state, message).unwrap(),
            &state,
            namespace(),
            [800, 100]
        ),
        Err(crate::Error::Restricted)
    ));
}

#[test]
fn allowlist_creation_and_activation_keep_the_same_policy() {
    let (policy, _) = allowlist_for(HOLDER);
    let mut message = create_message(true);
    if let Message::CreatePool { buy_policy, .. } = &mut message {
        *buy_policy = policy.clone();
    }
    let created = handle(&call(
        pool_id(),
        ActorState::empty(),
        Some(Actor::new(OWNER, PROGRAM)),
        message,
    ))
    .unwrap();
    let created = answer_reads(created, &pool_state(), namespace(), [0, 0]).unwrap();
    let state = PoolAccount::decode(&created.post_state.unwrap()).unwrap();
    assert_eq!(state.buy_policy, policy);
    assert!(!state.funded);
    let activated = answer_reads(
        start(&state, Message::ActivatePool { now: 1 }).unwrap(),
        &state,
        namespace(),
        [0, 0],
    )
    .unwrap();
    let state = PoolAccount::decode(&activated.post_state.unwrap()).unwrap();
    assert_eq!(state.buy_policy, policy);
    assert!(state.funded);
}
