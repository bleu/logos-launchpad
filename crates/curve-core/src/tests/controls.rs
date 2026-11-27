use super::fixture::{
    HOLDER, NS, OWNER, PROGRAM, TOKEN0, TOKEN1, answer_reads, call, event_kinds, namespace,
    pool_id, pool_state, start,
};
use crate::{Config, Message, PoolAccount, compute_pool_pda, handle};
use lee_core::account::{AccountId, Actor, ActorState};
use lee_core::program::ReceiveInput;

#[test]
fn namespace_freeze_blocks_each_swap_and_unfreeze_does_not_reopen_expired_pools() {
    let state = pool_state();
    let mut config = Config::decode(&namespace()).unwrap();
    config.emergency.is_frozen = true;
    for message in [
        Message::SwapExactInput {
            amount_in: 28,
            min_amount_out: 1,
            token_in: TOKEN1,
            membership: None,
            now: 1,
        },
        Message::SwapExactOutput {
            amount_out: 200,
            max_amount_in: 100,
            token_in: TOKEN1,
            membership: None,
            now: 1,
        },
        Message::SwapExactInput {
            amount_in: 250,
            min_amount_out: 1,
            token_in: TOKEN0,
            membership: None,
            now: 1,
        },
    ] {
        assert!(matches!(
            answer_reads(
                start(&state, message.clone()).unwrap(),
                &state,
                borsh::to_vec(&config).unwrap().into(),
                [800, 100]
            ),
            Err(crate::Error::Frozen)
        ));
        assert!(
            answer_reads(
                start(&state, message).unwrap(),
                &state,
                namespace(),
                [800, 100]
            )
            .is_ok()
        );
    }
    for lifecycle in [
        pool::PoolLifecycle::Open,
        pool::PoolLifecycle::Closed,
        pool::PoolLifecycle::Withdrawn,
    ] {
        let mut retired = state.clone();
        retired.pool.lifecycle = lifecycle;
        assert!(
            answer_reads(
                start(
                    &retired,
                    Message::SwapExactInput {
                        amount_in: 28,
                        min_amount_out: 0,
                        token_in: TOKEN1,
                        membership: None,
                        now: 42
                    }
                )
                .unwrap(),
                &retired,
                namespace(),
                [800, 100]
            )
            .is_err()
        );
    }
}

#[test]
fn pause_controls_emit_only_authenticated_changes() {
    let mut state = pool_state();
    state.trade_control = crate::TradeControl::new(crate::PausePolicy::AuthorityControlled);
    for paused in [false, true, true, false, false] {
        let message = Message::SetPaused { paused };
        let request = start(&state, message).unwrap();
        assert_eq!(
            request.sync_messages[0].to,
            Actor::new(HOLDER, crate::AUTHORITY_PROGRAM_ACCOUNT_ID)
        );
        let pending = request.post_state.as_ref().unwrap().clone();
        let role = crate::compute_pool_owner_role(NS, pool_id(), PROGRAM);
        for (sender, authority) in [
            (Actor::new(role, PROGRAM), HOLDER),
            (Actor::new(role, crate::AUTHORITY_PROGRAM_ACCOUNT_ID), OWNER),
            (
                Actor::new(
                    crate::compute_admin_role(NS, PROGRAM),
                    crate::AUTHORITY_PROGRAM_ACCOUNT_ID,
                ),
                HOLDER,
            ),
        ] {
            assert!(matches!(
                handle(&ReceiveInput {
                    receiver: Actor::new(pool_id(), PROGRAM),
                    from: Some(sender),
                    is_authorized: false,
                    pre_state: pending.clone(),
                    message: borsh::to_vec(&lez_compat_core::authority::Authorized {
                        role,
                        authority
                    })
                    .unwrap(),
                }),
                Err(crate::Error::Unauthorized)
            ));
        }
        let response = answer_reads(request, &state, namespace(), [800, 100]).unwrap();
        assert!(response.sync_messages.is_empty());
        let changed = state.trade_control.is_paused() != paused;
        let expected_events = if changed {
            vec![crate::events::SaleEventKindV1::PauseChanged { paused }]
        } else {
            vec![]
        };
        assert_eq!(event_kinds(&response), expected_events);
        assert_eq!(response.validity.timestamps.start(), None);
        assert_eq!(response.validity.timestamps.end(), Some(42_000));
        let next = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
        let mut expected = state.clone();
        expected.trade_control.set_paused(paused).unwrap();
        assert_eq!(next, expected);
        state = next;
    }
}

#[test]
fn pause_controls_reject_disabled_closed_withdrawn_and_unfunded_pools() {
    let mut state = pool_state();
    for paused in [true, false] {
        assert!(matches!(
            answer_reads(
                start(&state, Message::SetPaused { paused }).unwrap(),
                &state,
                namespace(),
                [800, 100]
            ),
            Err(crate::Error::PauseDisabled)
        ));
    }
    state.trade_control = crate::TradeControl::Paused;
    for lifecycle in [pool::PoolLifecycle::Closed, pool::PoolLifecycle::Withdrawn] {
        state.pool.lifecycle = lifecycle;
        for paused in [true, false] {
            assert!(matches!(
                answer_reads(
                    start(&state, Message::SetPaused { paused }).unwrap(),
                    &state,
                    namespace(),
                    [800, 100]
                ),
                Err(crate::Error::Closed)
            ));
        }
    }
    state.pool.lifecycle = pool::PoolLifecycle::Open;
    state.funded = false;
    assert!(matches!(
        start(&state, Message::SetPaused { paused: false }),
        Err(crate::Error::PendingFunding)
    ));
}

#[test]
fn pause_controls_bind_expiry_even_for_no_ops_and_check_conversion() {
    let mut state = pool_state();
    for control in [crate::TradeControl::Running, crate::TradeControl::Paused] {
        state.trade_control = control;
        for end in [
            None,
            Some(0),
            Some(42),
            Some(u64::MAX / 1000),
            Some(u64::MAX / 1000 + 1),
        ] {
            state.pool.close_timestamp = end;
            state.pool.ending_policy = if end.is_some() {
                crate::EndingPolicy::Scheduled
            } else {
                crate::EndingPolicy::Manual
            };
            let result = answer_reads(
                start(
                    &state,
                    Message::SetPaused {
                        paused: control.is_paused(),
                    },
                )
                .unwrap(),
                &state,
                namespace(),
                [800, 100],
            );
            if end.is_some_and(|end| end > u64::MAX / 1000) {
                assert!(matches!(result, Err(crate::Error::Arithmetic)));
            } else {
                let response = result.unwrap();
                let window = response.validity.timestamps;
                assert_eq!(window.start(), None);
                assert_eq!(window.end(), end.map(|end| end * 1000));
                if let Some(end) = end {
                    if end > 0 {
                        assert!(window.is_valid_for(end * 1000 - 1));
                    }
                    assert!(!window.is_valid_for(end * 1000));
                }
            }
        }
    }
}

#[test]
fn paused_curve_blocks_both_swap_forms_and_directions_at_both_boundaries() {
    let mut state = pool_state();
    state.trade_control = crate::TradeControl::Paused;
    for token_in in [TOKEN0, TOKEN1] {
        for message in [
            Message::SwapExactInput {
                amount_in: 1,
                min_amount_out: 0,
                token_in,
                membership: None,
                now: 1,
            },
            Message::SwapExactOutput {
                amount_out: 1,
                max_amount_in: 100,
                token_in,
                membership: None,
                now: 1,
            },
        ] {
            assert!(matches!(
                start(&state, message.clone()),
                Err(crate::Error::Paused)
            ));
            let mut pending = crate::dispatch::Pending::new(state.clone(), message, HOLDER, vec![]);
            pending.config = Some(Config::decode(&namespace()).unwrap());
            pending.balances = [800, 100];
            assert!(matches!(
                pending.advance(Actor::new(pool_id(), PROGRAM)),
                Err(crate::Error::Paused)
            ));
        }
    }
}

#[test]
fn paused_curve_keeps_fee_collection_close_and_expired_withdrawal_usable() {
    let mut state = pool_state();
    state.trade_control = crate::TradeControl::Paused;
    state.pool.fees_accrued = 7;
    for message in [
        Message::CollectFees,
        Message::ClosePool { now: 1 },
        Message::WithdrawReserves { now: 42 },
    ] {
        let response = answer_reads(
            start(&state, message).unwrap(),
            &state,
            namespace(),
            [800, 107],
        )
        .unwrap();
        let next = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
        assert_eq!(next.trade_control, crate::TradeControl::Paused);
    }
}

#[test]
fn resume_does_not_clear_namespace_freeze_or_change_schedule() {
    let mut state = pool_state();
    state.trade_control = crate::TradeControl::Paused;
    let mut config = Config::decode(&namespace()).unwrap();
    config.emergency.is_frozen = true;
    let bytes: ActorState = borsh::to_vec(&config).unwrap().into();
    let response = answer_reads(
        start(&state, Message::SetPaused { paused: false }).unwrap(),
        &state,
        bytes.clone(),
        [800, 100],
    )
    .unwrap();
    let resumed = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
    assert_eq!(resumed.pool, state.pool);
    assert!(!resumed.trade_control.is_paused());
    assert!(matches!(
        answer_reads(
            start(
                &resumed,
                Message::SwapExactInput {
                    amount_in: 28,
                    min_amount_out: 0,
                    token_in: TOKEN1,
                    membership: None,
                    now: 1
                }
            )
            .unwrap(),
            &resumed,
            bytes,
            [800, 100]
        ),
        Err(crate::Error::Frozen)
    ));
}

#[test]
fn factory_pause_requires_the_exact_bound_owner_actor() {
    let program = AccountId::new([61; 32]);
    let seed = [62; 32];
    let owner = AccountId::for_public_pda(&program, &lee_core::program::PdaSeed::new(seed));
    let mut state = pool_state();
    state.owner = owner;
    state.owner_program = Some((program, seed));
    state.trade_control = crate::TradeControl::Running;
    let id = compute_pool_pda(NS, PROGRAM, TOKEN0, TOKEN1, owner);
    for paused in [true, true, false, false] {
        let request = call(
            id,
            borsh::to_vec(&state).unwrap().into(),
            Some(Actor::new(owner, program)),
            Message::SetPaused { paused },
        );
        for sender in [
            Actor::new(owner, PROGRAM),
            Actor::new(HOLDER, program),
            Actor::new(NS, PROGRAM),
        ] {
            let mut wrong = request.clone();
            wrong.from = Some(sender);
            assert!(matches!(handle(&wrong), Err(crate::Error::Unauthorized)));
        }
        let response = handle(&request).unwrap();
        assert!(response.sync_messages.is_empty());
        assert_eq!(response.validity.timestamps.end(), Some(42_000));
        state = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
        assert_eq!(state.trade_control.is_paused(), paused);
    }
}
