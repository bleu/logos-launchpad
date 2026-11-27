use super::fixture::{
    HOLDER, OWNER, PROGRAM, TOKEN0, TOKEN1, answer_reads, call, create_message, event_kinds,
    namespace, pool_id, pool_state, start,
};
use crate::{Config, Message, PoolAccount, handle};
use lee_core::account::{Actor, ActorState};

#[test]
fn creation_events_separate_preparation_from_activation() {
    use crate::events::{EventPoliciesV1, SaleEventKindV1};
    for deferred in [false, true] {
        let pending = handle(&call(
            pool_id(),
            ActorState::empty(),
            Some(Actor::new(OWNER, PROGRAM)),
            create_message(deferred),
        ))
        .unwrap();
        let created = answer_reads(pending, &pool_state(), namespace(), [0, 0]).unwrap();
        let prepared = SaleEventKindV1::Prepared {
            owner: *OWNER.value(),
            token0_definition: *TOKEN0.value(),
            token1_definition: *TOKEN1.value(),
            reserve0: 800,
            reserve1: 25,
            policies: EventPoliciesV1 {
                ending: crate::EndingPolicy::Scheduled,
                pause: crate::PausePolicy::Disabled,
                sell_fee_bps: 0,
            },
        };
        let activated = SaleEventKindV1::Activated {
            reserve0: 800,
            reserve1: 25,
            collateral_seed: 0,
        };
        assert_eq!(
            event_kinds(&created),
            if deferred {
                vec![prepared]
            } else {
                vec![prepared, activated.clone()]
            }
        );
        let state = PoolAccount::decode(created.post_state.as_ref().unwrap()).unwrap();
        assert!(!state.closure_observed);
        if deferred {
            let response = answer_reads(
                start(&state, Message::ActivatePool { now: 1 }).unwrap(),
                &state,
                namespace(),
                [0, 0],
            )
            .unwrap();
            assert_eq!(event_kinds(&response), vec![activated]);
        }
    }
}

#[test]
fn trade_events_copy_quotes_and_actual_direct_pool_definitions() {
    use crate::events::{SaleEventKindV1, TradeFormV1};
    let state = pool_state();
    for (side, definition, output, form) in [
        (
            pool::TokenSide::Token0,
            TOKEN0,
            TOKEN1,
            TradeFormV1::ExactInput,
        ),
        (
            pool::TokenSide::Token1,
            TOKEN1,
            TOKEN0,
            TradeFormV1::ExactInput,
        ),
        (
            pool::TokenSide::Token1,
            TOKEN1,
            TOKEN0,
            TradeFormV1::ExactOutput,
        ),
    ] {
        let mut quoted = state.pool.clone();
        let (message, outcome) = match form {
            TradeFormV1::ExactInput => {
                let amount_in = if definition == TOKEN0 { 250 } else { 28 };
                (
                    Message::SwapExactInput {
                        amount_in,
                        min_amount_out: 0,
                        token_in: definition,
                        membership: None,
                        now: 1,
                    },
                    quoted
                        .swap_exact_input(pool::ExactInputSwap {
                            token_in: side,
                            amount_in,
                            min_amount_out: 0,
                            pool_fee_bps: 0,
                            protocol_fee_bps: 100,
                            now: 1,
                        })
                        .unwrap(),
                )
            }
            TradeFormV1::ExactOutput => (
                Message::SwapExactOutput {
                    amount_out: 200,
                    max_amount_in: 100,
                    token_in: definition,
                    membership: None,
                    now: 1,
                },
                quoted
                    .swap_exact_output(pool::ExactOutputSwap {
                        token_in: side,
                        amount_out: 200,
                        max_amount_in: 100,
                        pool_fee_bps: 0,
                        protocol_fee_bps: 100,
                        now: 1,
                    })
                    .unwrap(),
            ),
        };
        let response = answer_reads(
            start(&state, message).unwrap(),
            &state,
            namespace(),
            [800, 100],
        )
        .unwrap();
        assert_eq!(
            event_kinds(&response),
            vec![SaleEventKindV1::Trade {
                trader: *HOLDER.value(),
                form,
                token_in: *definition.value(),
                token_out: *output.value(),
                amount_in: outcome.amount_in,
                pricing_input: outcome.effective_amount_in,
                gross_output: outcome.raw_amount_out,
                amount_out: outcome.amount_out,
                fee_definition: *TOKEN1.value(),
                fee_amount: outcome.protocol_fee,
                fee_bps: 100,
                retained_fee: 0,
                sell_fee_bps: 0,
                pricing_time: None,
            }]
        );
        if definition == TOKEN0 {
            assert_eq!(
                (
                    outcome.amount_in,
                    outcome.effective_amount_in,
                    outcome.raw_amount_out,
                    outcome.amount_out,
                    outcome.protocol_fee
                ),
                (250, 250, 20, 19, 1)
            );
        }
    }
}

#[test]
fn fee_events_distinguish_liability_from_self_transfer_and_skip_zero_retry() {
    use crate::events::SaleEventKindV1;
    let mut state = pool_state();
    state.pool.fees_accrued = 10;
    for treasury in [pool_id(), HOLDER] {
        let mut config = Config::decode(&namespace()).unwrap();
        config.treasury = treasury;
        let bytes: ActorState = borsh::to_vec(&config).unwrap().into();
        let response = answer_reads(
            start(&state, Message::CollectFees).unwrap(),
            &state,
            bytes.clone(),
            [800, 110],
        )
        .unwrap();
        assert_eq!(
            event_kinds(&response),
            vec![SaleEventKindV1::FeesCollected {
                definition: *TOKEN1.value(),
                treasury: *treasury.value(),
                liability_discharged: 10,
                tokens_transferred: if treasury == pool_id() { 0 } else { 10 },
            }]
        );
        let collected = PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(
            (collected.pool.real_reserve0, collected.pool.real_reserve1),
            (800, 100)
        );
        assert!(!collected.closure_observed);
        let retry = answer_reads(
            start(&collected, Message::CollectFees).unwrap(),
            &collected,
            bytes,
            [800, 110],
        )
        .unwrap();
        assert!(retry.events.is_empty());
        assert!(retry.sync_messages.is_empty());
    }
}

#[test]
fn manual_close_is_stored_once_and_withdrawal_reports_reserve_effects() {
    use crate::events::{
        ClosureReasonV1, EventTimeBoundsV1, PayoutPurposeV1, SaleEventKindV1, SettlementEffectV1,
        SettlementStepV1,
    };
    let state = pool_state();
    let response = answer_reads(
        start(&state, Message::ClosePool { now: 1 }).unwrap(),
        &state,
        namespace(),
        [800, 100],
    )
    .unwrap();
    assert_eq!(
        event_kinds(&response),
        vec![SaleEventKindV1::Closed {
            reason: ClosureReasonV1::Manual,
            inclusion_bounds: EventTimeBoundsV1 {
                at_least: None,
                before: Some(42)
            },
        }]
    );
    let closed = PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(closed.closure_observed);
    let response = answer_reads(
        start(&closed, Message::WithdrawReserves { now: 1 }).unwrap(),
        &closed,
        namespace(),
        [800, 100],
    )
    .unwrap();
    assert_eq!(
        event_kinds(&response),
        vec![SaleEventKindV1::Settlement {
            step: SettlementStepV1::ReservesWithdrawn,
            route: None,
            effects: [(TOKEN0, 800), (TOKEN1, 100)]
                .into_iter()
                .map(|(definition, amount)| SettlementEffectV1::Payout {
                    definition: *definition.value(),
                    recipient: *HOLDER.value(),
                    purpose: PayoutPurposeV1::Reserve,
                    liability_discharged: amount,
                    tokens_transferred: amount,
                })
                .collect(),
        }]
    );
    assert!(
        PoolAccount::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .closure_observed
    );
}

#[test]
fn scheduled_closure_uses_inclusion_bounds_and_reads_do_not_observe_it() {
    use crate::events::{ClosureReasonV1, EventTimeBoundsV1, SaleEventKindV1};
    for deferred in [false, true] {
        let mut state = pool_state();
        state.funded = !deferred;
        let read = start(&state, Message::ReadState).unwrap();
        assert!(read.events.is_empty());
        assert!(read.post_state.is_none());
        let message = if deferred {
            Message::ActivatePool { now: 42 }
        } else {
            Message::WithdrawReserves { now: 0 }
        };
        let response = answer_reads(
            start(&state, message).unwrap(),
            &state,
            namespace(),
            [800, 100],
        )
        .unwrap();
        let closure = SaleEventKindV1::Closed {
            reason: ClosureReasonV1::ScheduledEnd { timestamp: 42 },
            inclusion_bounds: EventTimeBoundsV1 {
                at_least: Some(42),
                before: None,
            },
        };
        let events = event_kinds(&response);
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, SaleEventKindV1::Closed { .. }))
                .collect::<Vec<_>>(),
            vec![&closure]
        );
        assert_eq!(response.validity.timestamps.start(), Some(42_000));
        assert_eq!(response.validity.timestamps.end(), None);
        let next = PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert!(next.closure_observed);
        if deferred {
            let withdrawn = answer_reads(
                start(&next, Message::WithdrawReserves { now: 0 }).unwrap(),
                &next,
                namespace(),
                [800, 100],
            )
            .unwrap();
            assert!(
                event_kinds(&withdrawn)
                    .iter()
                    .all(|event| !matches!(event, SaleEventKindV1::Closed { .. }))
            );
        }
    }
}

#[test]
fn final_buy_emits_depletion_closure_only_for_selected_endings() {
    use crate::events::{ClosureReasonV1, SaleEventKindV1};
    for ending in [
        crate::EndingPolicy::Scheduled,
        crate::EndingPolicy::Depletion,
        crate::EndingPolicy::Either,
    ] {
        let mut state = pool_state();
        state.pool = pool::Pool::create(pool::PoolCreateParams {
            token0_amount: 80,
            token1_amount: 100,
            virtual_reserve0: 100,
            virtual_reserve1: 100,
            ending_policy: ending,
            sell_fee_bps: 0,
            close_timestamp: if ending == crate::EndingPolicy::Depletion {
                None
            } else {
                Some(42)
            },
            close_on_depletion: if ending == crate::EndingPolicy::Scheduled {
                None
            } else {
                Some(pool::TokenSide::Token0)
            },
        })
        .unwrap();
        let response = answer_reads(
            start(
                &state,
                Message::SwapExactOutput {
                    amount_out: 80,
                    max_amount_in: 1000,
                    token_in: TOKEN1,
                    membership: None,
                    now: 41,
                },
            )
            .unwrap(),
            &state,
            namespace(),
            [80, 100],
        )
        .unwrap();
        let events = event_kinds(&response);
        let closures: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                SaleEventKindV1::Closed { reason, .. } => Some(reason),
                _ => None,
            })
            .collect();
        let next = PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(next.pool.real_reserve0, 0);
        if ending == crate::EndingPolicy::Scheduled {
            assert!(closures.is_empty());
            assert!(!next.closure_observed);
            assert_eq!(next.pool.lifecycle, pool::PoolLifecycle::Open);
        } else {
            assert_eq!(
                closures,
                vec![&ClosureReasonV1::Depleted {
                    definition: *TOKEN0.value()
                }]
            );
            assert!(next.closure_observed);
            assert_eq!(next.pool.lifecycle, pool::PoolLifecycle::Closed);
        }
    }
}

#[test]
fn retained_sell_event_and_transfers_keep_collateral_in_reserves() {
    use crate::events::SaleEventKindV1;
    let mut state = pool_state();
    state.pool.sell_fee_bps = 125;
    let response = answer_reads(
        start(
            &state,
            Message::SwapExactInput {
                amount_in: 250,
                min_amount_out: 18,
                token_in: TOKEN0,
                membership: None,
                now: 1,
            },
        )
        .unwrap(),
        &state,
        namespace(),
        [800, 107],
    )
    .unwrap();
    let after = PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        (
            after.pool.real_reserve1,
            after.pool.virtual_reserve1,
            after.pool.fees_accrued
        ),
        (81, 81, 1)
    );
    assert_eq!((after.pool.initial_k, after.pool.k), (100_000, 101_250));
    assert!(matches!(
        event_kinds(&response).as_slice(),
        [SaleEventKindV1::Trade {
            gross_output: 20,
            amount_out: 18,
            fee_amount: 1,
            fee_bps: 100,
            retained_fee: 1,
            sell_fee_bps: 125,
            ..
        }]
    ));
    let transfers: Vec<lez_compat_core::token::Message> = response
        .sync_messages
        .iter()
        .map(|call| borsh::from_slice(&call.message).unwrap())
        .collect();
    assert!(matches!(
        transfers.as_slice(),
        [
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN0,
                amount: 250,
                ..
            },
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN1,
                amount: 18,
                ..
            }
        ]
    ));
    assert_eq!(
        107 - 18,
        after.pool.real_reserve1 + after.pool.fees_accrued + 7
    );
}
