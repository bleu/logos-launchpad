use super::fixture::{
    HOLDER, NS, OWNER, PROGRAM, TOKEN0, TOKEN1, answer_reads, event_kinds, namespace, pool_id,
    pool_state, start,
};
use crate::{Config, Message, PoolAccount, handle};
use lee_core::account::Actor;
use lee_core::program::ReceiveInput;

#[test]
fn neutral_input_restrictions_apply_to_both_buy_forms_and_sells() {
    let mut state = pool_state();
    state.allowed_input_token = Some(pool::TokenSide::Token0);
    for message in [
        Message::SwapExactInput {
            amount_in: 28,
            min_amount_out: 0,
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
    ] {
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
    state.allowed_input_token = Some(pool::TokenSide::Token1);
    assert!(matches!(
        answer_reads(
            start(
                &state,
                Message::SwapExactInput {
                    amount_in: 250,
                    min_amount_out: 0,
                    token_in: TOKEN0,
                    membership: None,
                    now: 1
                }
            )
            .unwrap(),
            &state,
            namespace(),
            [800, 100]
        ),
        Err(crate::Error::Restricted)
    ));
    state.allowed_input_token = None;
    assert!(
        answer_reads(
            start(
                &state,
                Message::SwapExactOutput {
                    amount_out: 19,
                    max_amount_in: 1000,
                    token_in: TOKEN0,
                    membership: None,
                    now: 1
                }
            )
            .unwrap(),
            &state,
            namespace(),
            [800, 100]
        )
        .is_err()
    );
}

#[test]
fn live_fee_changes_respect_buy_and_sell_slippage() {
    let state = pool_state();
    let mut high = Config::decode(&namespace()).unwrap();
    high.protocol_fee_bps = 1000;
    for message in [
        Message::SwapExactInput {
            amount_in: 250,
            min_amount_out: 19,
            token_in: TOKEN0,
            membership: None,
            now: 1,
        },
        Message::SwapExactInput {
            amount_in: 28,
            min_amount_out: 205,
            token_in: TOKEN1,
            membership: None,
            now: 1,
        },
        Message::SwapExactOutput {
            amount_out: 200,
            max_amount_in: 26,
            token_in: TOKEN1,
            membership: None,
            now: 1,
        },
    ] {
        assert!(
            answer_reads(
                start(&state, message.clone()).unwrap(),
                &state,
                namespace(),
                [800, 100]
            )
            .is_ok()
        );
        assert!(
            answer_reads(
                start(&state, message).unwrap(),
                &state,
                borsh::to_vec(&high).unwrap().into(),
                [800, 100]
            )
            .is_err()
        );
    }
}

#[test]
fn swaps_require_inclusion_before_expiry_without_client_time_freshness() {
    let mut state = pool_state();
    state.pool.close_timestamp = Some(1_700_000_042);
    for message in [
        Message::SwapExactInput {
            amount_in: 28,
            min_amount_out: 0,
            token_in: TOKEN1,
            membership: None,
            now: 1_700_000_001,
        },
        Message::SwapExactOutput {
            amount_out: 200,
            max_amount_in: 100,
            token_in: TOKEN1,
            membership: None,
            now: 1_700_000_001,
        },
    ] {
        let response = answer_reads(
            start(&state, message).unwrap(),
            &state,
            namespace(),
            [800, 100],
        )
        .unwrap();
        assert_eq!(response.validity.timestamps.start(), None);
        assert_eq!(response.validity.timestamps.end(), Some(1_700_000_042_000));
        assert_eq!(
            PoolAccount::decode(&response.post_state.unwrap())
                .unwrap()
                .pool
                .close_timestamp,
            state.pool.close_timestamp
        );
    }
}

#[test]
fn exact_input_sell_uses_live_collateral_fee_and_synchronous_token_transfers() {
    let state = pool_state();
    let response = answer_reads(
        start(
            &state,
            Message::SwapExactInput {
                amount_in: 250,
                min_amount_out: 19,
                token_in: TOKEN0,
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
    let updated = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
    assert_eq!(
        (
            updated.pool.real_reserve0,
            updated.pool.real_reserve1,
            updated.pool.fees_accrued
        ),
        (1050, 80, 1)
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
                definition: TOKEN0,
                recipient: pool_id(),
                amount: 250
            },
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN1,
                recipient: HOLDER,
                amount: 19
            }
        ]
    );
    assert!(
        response
            .sync_messages
            .last()
            .unwrap()
            .pda_seeds
            .contains(&crate::compute_pool_pda_seed(NS, TOKEN0, TOKEN1, OWNER))
    );
    assert_eq!(response.validity.timestamps.start(), None);
    assert_eq!(response.validity.timestamps.end(), Some(42_000));
}

#[test]
fn exact_output_buy_caps_full_collateral_input_including_live_fees() {
    let state = pool_state();
    let mut config = Config::decode(&namespace()).unwrap();
    config.protocol_fee_bps = 1000;
    let message = Message::SwapExactOutput {
        amount_out: 200,
        max_amount_in: 28,
        token_in: TOKEN1,
        membership: None,
        now: 1,
    };
    let response = answer_reads(
        start(&state, message).unwrap(),
        &state,
        borsh::to_vec(&config).unwrap().into(),
        [800, 100],
    )
    .unwrap();
    let updated = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
    assert_eq!(
        (
            updated.pool.real_reserve0,
            updated.pool.real_reserve1,
            updated.pool.fees_accrued
        ),
        (600, 125, 3)
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
                recipient: pool_id(),
                amount: 28
            },
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN0,
                recipient: HOLDER,
                amount: 200
            }
        ]
    );
}

#[test]
fn final_buys_keep_pretrade_expiry_bounds_and_match_pure_pool_quotes() {
    for exact_output in [false, true] {
        let mut state = pool_state();
        state.pool = pool::Pool::create(pool::PoolCreateParams {
            token0_amount: 80,
            token1_amount: 100,
            virtual_reserve0: 100,
            virtual_reserve1: 100,
            ending_policy: crate::EndingPolicy::Either,
            sell_fee_bps: 0,
            close_timestamp: Some(42),
            close_on_depletion: Some(pool::TokenSide::Token0),
        })
        .unwrap();
        let mut expected = state.pool.clone();
        let (message, quote) = if exact_output {
            (
                Message::SwapExactOutput {
                    amount_out: 80,
                    max_amount_in: 1000,
                    token_in: TOKEN1,
                    membership: None,
                    now: 41,
                },
                expected
                    .swap_exact_output(pool::ExactOutputSwap {
                        token_in: pool::TokenSide::Token1,
                        amount_out: 80,
                        max_amount_in: 1000,
                        pool_fee_bps: 0,
                        protocol_fee_bps: 100,
                        now: 41,
                    })
                    .unwrap(),
            )
        } else {
            (
                Message::SwapExactInput {
                    amount_in: 405,
                    min_amount_out: 80,
                    token_in: TOKEN1,
                    membership: None,
                    now: 41,
                },
                expected
                    .swap_exact_input(pool::ExactInputSwap {
                        token_in: pool::TokenSide::Token1,
                        amount_in: 405,
                        min_amount_out: 80,
                        pool_fee_bps: 0,
                        protocol_fee_bps: 100,
                        now: 41,
                    })
                    .unwrap(),
            )
        };
        assert_eq!(quote.amount_out, 80);
        let response = answer_reads(
            start(&state, message).unwrap(),
            &state,
            namespace(),
            [80, 100],
        )
        .unwrap();
        assert_eq!(response.validity.timestamps.start(), None);
        assert_eq!(response.validity.timestamps.end(), Some(42_000));
        assert!(response.validity.timestamps.is_valid_for(41_999));
        assert!(!response.validity.timestamps.is_valid_for(42_000));
        let closed = PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(closed.pool, expected);
        assert!(closed.closure_observed);
        assert_eq!(closed.pool.lifecycle, pool::PoolLifecycle::Closed);
        let events = event_kinds(&response);
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[1],
            crate::events::SaleEventKindV1::Closed {
                reason: crate::events::ClosureReasonV1::Depleted {
                    definition: *TOKEN0.value()
                },
                inclusion_bounds: crate::events::EventTimeBoundsV1 {
                    at_least: None,
                    before: Some(42)
                },
            }
        );
        let withdrawn = answer_reads(
            start(&closed, Message::WithdrawReserves { now: 1 }).unwrap(),
            &closed,
            namespace(),
            [0, expected.real_reserve1 + expected.fees_accrued],
        )
        .unwrap();
        assert_eq!(withdrawn.validity.timestamps.start(), None);
        assert!(
            event_kinds(&withdrawn)
                .iter()
                .all(|event| !matches!(event, crate::events::SaleEventKindV1::Closed { .. }))
        );
    }
}

#[test]
fn capped_curve_buys_share_one_allowance_and_bind_inclusion() {
    let mut state = pool_state();
    state.block_allocation = crate::BlockAllocation::new(Some(200)).unwrap();

    let execute = |state: &PoolAccount, message, block| {
        let response = answer_reads(
            start(state, message)?,
            state,
            namespace(),
            [
                state.pool.real_reserve0,
                state.pool.real_reserve1 + state.pool.fees_accrued,
            ],
        )?;
        let reply = borsh::to_vec(&clock_core::Reply::BlockId(block)).unwrap();
        let call = |from| ReceiveInput {
            receiver: Actor::new(pool_id(), PROGRAM),
            from: Some(from),
            is_authorized: false,
            pre_state: response.post_state.clone().unwrap(),
            message: reply.clone(),
        };
        assert!(matches!(
            handle(&call(Actor::new(HOLDER, PROGRAM))),
            Err(crate::Error::Unauthorized)
        ));
        handle(&call(clock_core::clock_actor()))
    };

    let buy = Message::SwapExactOutput {
        token_in: TOKEN1,
        amount_out: 200,
        max_amount_in: 100,
        now: 1,
        membership: None,
    };
    let first = execute(&state, buy, 8).unwrap();
    assert_eq!(first.validity.blocks.start(), Some(9));
    assert_eq!(first.validity.blocks.end(), Some(10));
    state = PoolAccount::decode(&first.post_state.unwrap()).unwrap();
    assert_eq!(state.block_allocation.tokens_sold, 200);

    let buy = Message::SwapExactInput {
        token_in: TOKEN1,
        amount_in: 10,
        min_amount_out: 1,
        now: 1,
        membership: None,
    };
    assert!(matches!(
        execute(&state, buy.clone(), 8),
        Err(crate::Error::BlockAllocationExceeded)
    ));
    let next = execute(&state, buy, 9).unwrap();
    let state = PoolAccount::decode(&next.post_state.unwrap()).unwrap();
    assert_eq!(state.block_allocation.observed_block, Some(9));
    assert!((1..200).contains(&state.block_allocation.tokens_sold));
}
