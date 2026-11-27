use super::fixture::{
    HOLDER, NS, PROGRAM, TOKEN0, TOKEN1, answer_reads, call, namespace, pool_id, pool_state, start,
};
use crate::{Config, Message, PoolAccount, TOKEN_PROGRAM_ACCOUNT_ID, compute_config_pda, handle};
use lee_core::account::{AccountId, Actor};
use lee_core::program::{ReceiveInput, StateReply};

#[test]
fn closed_pool_withdrawal_does_not_wait_for_its_future_deadline() {
    let mut state = pool_state();
    state.pool.lifecycle = pool::PoolLifecycle::Closed;
    for deadline in [Some(42), None] {
        state.pool.close_timestamp = deadline;
        state.pool.ending_policy = if deadline.is_some() {
            crate::EndingPolicy::Scheduled
        } else {
            crate::EndingPolicy::Manual
        };
        let response = answer_reads(
            start(&state, Message::WithdrawReserves { now: 1 }).unwrap(),
            &state,
            namespace(),
            [800, 100],
        )
        .unwrap();
        assert_eq!(response.validity.timestamps.start(), None);
        assert_eq!(response.validity.timestamps.end(), None);
        assert_eq!(
            PoolAccount::decode(&response.post_state.unwrap())
                .unwrap()
                .pool
                .lifecycle,
            pool::PoolLifecycle::Withdrawn
        );
    }
}

#[test]
fn pools_without_expiry_keep_time_independent_trading_and_cannot_withdraw_while_open() {
    let mut state = pool_state();
    state.pool.close_timestamp = None;
    state.pool.ending_policy = crate::EndingPolicy::Manual;
    let response = answer_reads(
        start(
            &state,
            Message::SwapExactInput {
                amount_in: 28,
                min_amount_out: 0,
                token_in: TOKEN1,
                membership: None,
                now: 1,
            },
        )
        .unwrap(),
        &state,
        namespace(),
        [800, 100],
    )
    .unwrap();
    assert!(
        response
            .sync_messages
            .iter()
            .all(|message| message.to.program_account_id == TOKEN_PROGRAM_ACCOUNT_ID)
    );
    assert!(matches!(
        answer_reads(
            start(&state, Message::WithdrawReserves { now: 42 }).unwrap(),
            &state,
            namespace(),
            [800, 100]
        ),
        Err(crate::Error::Closed)
    ));
    let closed = answer_reads(
        start(&state, Message::ClosePool { now: 1 }).unwrap(),
        &state,
        namespace(),
        [800, 100],
    )
    .unwrap();
    assert_eq!(closed.validity.timestamps.start(), None);
    assert_eq!(closed.validity.timestamps.end(), None);
    assert!(closed.sync_messages.is_empty());
}

#[test]
fn accrued_fees_cannot_back_swaps_collection_or_withdrawal() {
    let mut state = pool_state();
    state.pool.fees_accrued = 7;
    for message in [
        Message::SwapExactInput {
            amount_in: 250,
            min_amount_out: 0,
            token_in: TOKEN0,
            membership: None,
            now: 1,
        },
        Message::CollectFees,
        Message::WithdrawReserves { now: 42 },
    ] {
        assert!(matches!(
            answer_reads(
                start(&state, message).unwrap(),
                &state,
                namespace(),
                [800, 100]
            ),
            Err(crate::Error::Underfunded)
        ));
    }
    assert!(matches!(
        answer_reads(
            start(&state, Message::WithdrawReserves { now: 42 }).unwrap(),
            &state,
            namespace(),
            [799, 107]
        ),
        Err(crate::Error::Underfunded)
    ));
}

#[test]
fn pool_treasury_leaves_collected_fees_in_custody_and_owner_treasury_receives_both() {
    let mut state = pool_state();
    state.pool.fees_accrued = 7;
    for treasury in [pool_id(), HOLDER] {
        let mut config = Config::decode(&namespace()).unwrap();
        config.treasury = treasury;
        let response = answer_reads(
            start(&state, Message::WithdrawReserves { now: 42 }).unwrap(),
            &state,
            borsh::to_vec(&config).unwrap().into(),
            [811, 118],
        )
        .unwrap();
        let withdrawn = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
        assert_eq!(
            (withdrawn.pool.fees_accrued, withdrawn.pool.fees_collected),
            (0, 7)
        );
        let transfers: Vec<_> = response
            .sync_messages
            .iter()
            .filter_map(|c| borsh::from_slice::<lez_compat_core::token::Message>(&c.message).ok())
            .collect();
        let expected = if treasury == pool_id() {
            vec![
                lez_compat_core::token::Message::Transfer {
                    definition: TOKEN0,
                    recipient: HOLDER,
                    amount: 800,
                },
                lez_compat_core::token::Message::Transfer {
                    definition: TOKEN1,
                    recipient: HOLDER,
                    amount: 100,
                },
            ]
        } else {
            vec![
                lez_compat_core::token::Message::Transfer {
                    definition: TOKEN1,
                    recipient: HOLDER,
                    amount: 7,
                },
                lez_compat_core::token::Message::Transfer {
                    definition: TOKEN0,
                    recipient: HOLDER,
                    amount: 800,
                },
                lez_compat_core::token::Message::Transfer {
                    definition: TOKEN1,
                    recipient: HOLDER,
                    amount: 100,
                },
            ]
        };
        assert_eq!(transfers, expected);
    }
}

#[test]
fn withdrawal_requires_inclusion_at_expiry_not_a_client_time() {
    let state = pool_state();
    for now in [0, 42, 43] {
        let response = answer_reads(
            start(&state, Message::WithdrawReserves { now }).unwrap(),
            &state,
            namespace(),
            [800, 100],
        )
        .unwrap();
        assert_eq!(response.validity.timestamps.start(), Some(42_000));
        assert_eq!(response.validity.timestamps.end(), None);
        assert_eq!(
            PoolAccount::decode(&response.post_state.unwrap())
                .unwrap()
                .pool
                .lifecycle,
            pool::PoolLifecycle::Withdrawn
        );
    }
}

#[test]
fn permissionless_collection_uses_current_treasury_and_leaves_donations_as_surplus() {
    let mut state = pool_state();
    state.pool.fees_accrued = 7;
    let mut config = Config::decode(&namespace()).unwrap();
    config.treasury = AccountId::new([64; 32]);
    config.emergency.is_frozen = true;
    let response = answer_reads(
        handle(&call(
            pool_id(),
            borsh::to_vec(&state).unwrap().into(),
            None,
            Message::CollectFees,
        ))
        .unwrap(),
        &state,
        borsh::to_vec(&config).unwrap().into(),
        [800, 118],
    )
    .unwrap();
    let collected = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
    assert_eq!(
        (
            collected.pool.fees_accrued,
            collected.pool.fees_collected,
            collected.pool.real_reserve1
        ),
        (0, 7, 100)
    );
    assert_eq!(response.sync_messages.len(), 1);
    assert_eq!(
        borsh::from_slice::<lez_compat_core::token::Message>(&response.sync_messages[0].message)
            .unwrap(),
        lez_compat_core::token::Message::Transfer {
            definition: TOKEN1,
            recipient: config.treasury,
            amount: 7
        }
    );
    let replay = answer_reads(
        handle(&call(
            pool_id(),
            borsh::to_vec(&collected).unwrap().into(),
            None,
            Message::CollectFees,
        ))
        .unwrap(),
        &collected,
        borsh::to_vec(&config).unwrap().into(),
        [800, 111],
    )
    .unwrap();
    assert!(replay.sync_messages.is_empty());
}

#[test]
fn expired_withdrawal_pays_current_fees_and_principal_once_leaving_surplus() {
    let mut state = pool_state();
    state.pool.fees_accrued = 7;
    let mut config = Config::decode(&namespace()).unwrap();
    config.emergency.is_frozen = true;
    let response = answer_reads(
        start(&state, Message::WithdrawReserves { now: 42 }).unwrap(),
        &state,
        borsh::to_vec(&config).unwrap().into(),
        [811, 118],
    )
    .unwrap();
    let withdrawn = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
    assert_eq!(
        (
            withdrawn.pool.real_reserve0,
            withdrawn.pool.real_reserve1,
            withdrawn.pool.fees_accrued,
            withdrawn.pool.fees_collected
        ),
        (0, 0, 0, 7)
    );
    let transfers: Vec<_> = response
        .sync_messages
        .iter()
        .filter_map(|c| borsh::from_slice::<lez_compat_core::token::Message>(&c.message).ok())
        .collect();
    assert_eq!(
        transfers,
        vec![
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN1,
                recipient: config.treasury,
                amount: 7
            },
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN0,
                recipient: HOLDER,
                amount: 800
            },
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN1,
                recipient: HOLDER,
                amount: 100
            }
        ]
    );
    assert!(
        answer_reads(
            start(&withdrawn, Message::WithdrawReserves { now: 43 }).unwrap(),
            &withdrawn,
            namespace(),
            [11, 11]
        )
        .is_err()
    );
}

#[test]
fn malformed_saved_policy_blocks_reads_activation_swaps_controls_and_settlement() {
    for malformed in 0..3 {
        let mut state = pool_state();
        match malformed {
            0 => state.pool.ending_policy = crate::EndingPolicy::Manual,
            1 => state.pool.ending_policy = crate::EndingPolicy::Depletion,
            _ => state.pool.close_on_depletion = Some(pool::TokenSide::Token1),
        }
        for message in [
            Message::ReadState,
            Message::ActivatePool { now: 1 },
            Message::SwapExactInput {
                amount_in: 28,
                min_amount_out: 0,
                token_in: TOKEN1,
                membership: None,
                now: 1,
            },
            Message::SwapExactInput {
                amount_in: 250,
                min_amount_out: 0,
                token_in: TOKEN0,
                membership: None,
                now: 1,
            },
            Message::SwapExactOutput {
                amount_out: 100,
                max_amount_in: 100,
                token_in: TOKEN1,
                membership: None,
                now: 1,
            },
            Message::SetPaused { paused: true },
            Message::ClosePool { now: 1 },
            Message::CollectFees,
            Message::WithdrawReserves { now: 42 },
        ] {
            assert!(matches!(
                start(&state, message.clone()),
                Err(crate::Error::InvalidPool)
            ));
            let pending = crate::dispatch::Pending::new(
                state.clone(),
                message,
                HOLDER,
                vec![crate::dispatch::Query::Namespace],
            );
            assert!(matches!(
                handle(&ReceiveInput {
                    receiver: Actor::new(pool_id(), PROGRAM),
                    from: Some(Actor::new(compute_config_pda(NS, PROGRAM), PROGRAM)),
                    is_authorized: false,
                    pre_state: borsh::to_vec(&pending).unwrap().into(),
                    message: borsh::to_vec(&StateReply { state: namespace() }).unwrap(),
                }),
                Err(crate::Error::InvalidPool)
            ));
        }
    }
}

#[test]
fn saved_open_depletion_uses_selected_custody_reserve_and_blocks_controls() {
    for side in [pool::TokenSide::Token0, pool::TokenSide::Token1] {
        let mut state = pool_state();
        state.pool.ending_policy = crate::EndingPolicy::Either;
        state.pool.close_on_depletion = Some(side);
        state.trade_control = crate::TradeControl::Running;
        match side {
            pool::TokenSide::Token0 => state.pool.real_reserve0 = 0,
            pool::TokenSide::Token1 => state.pool.real_reserve1 = 0,
        }
        let balances = [state.pool.real_reserve0, state.pool.real_reserve1];
        let response = answer_reads(
            start(&state, Message::WithdrawReserves { now: 1 }).unwrap(),
            &state,
            namespace(),
            balances,
        )
        .unwrap();
        assert_eq!(response.validity.timestamps.start(), None);
        assert_eq!(response.validity.timestamps.end(), None);
        for message in [
            Message::SetPaused { paused: true },
            Message::SetPaused { paused: false },
        ] {
            assert!(matches!(
                answer_reads(
                    start(&state, message).unwrap(),
                    &state,
                    namespace(),
                    balances
                ),
                Err(crate::Error::Closed)
            ));
        }
    }
}

#[test]
fn depleted_policy_expiry_requires_lower_bound_when_inventory_remains() {
    let mut state = pool_state();
    state.pool.ending_policy = crate::EndingPolicy::Either;
    state.pool.close_on_depletion = Some(pool::TokenSide::Token0);
    let response = answer_reads(
        start(&state, Message::WithdrawReserves { now: 0 }).unwrap(),
        &state,
        namespace(),
        [800, 100],
    )
    .unwrap();
    assert_eq!(response.validity.timestamps.start(), Some(42_000));
    assert_eq!(response.validity.timestamps.end(), None);
    assert!(!response.validity.timestamps.is_valid_for(41_999));
    assert!(response.validity.timestamps.is_valid_for(42_000));
    let state = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
    assert_eq!(
        state.pool.ending_decision(),
        Ok(crate::EndingDecision::Manual)
    );
}
