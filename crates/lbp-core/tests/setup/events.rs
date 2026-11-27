use super::fixture::{
    activation, config, exact_in, funded, holding, id, operation, prepare, prepared, reply,
    sale_events, settle, state_of,
};
use lbp_core::{Message, handle};
use lee_core::account::Actor;

#[test]
fn fixed_setup_events_describe_saved_policies_and_reserves() {
    use curve_core::events::*;
    let first = handle(&prepare()).unwrap();
    assert!(sale_events(&first).is_empty());
    let (input, response) = prepared();
    let events = sale_events(&response);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].version, 1);
    assert_eq!(events[0].namespace, *id(40).value());
    assert_eq!(events[0].engine, PricingEngineV1::FixedPrice);
    assert_eq!(
        events[0].subject,
        EventSubjectV1::Pool {
            pool: *input.receiver.account_id.value()
        }
    );
    assert_eq!(
        events[0].kind,
        SaleEventKindV1::Prepared {
            owner: *prepare().from.unwrap().account_id.value(),
            token0_definition: *id(41).value(),
            token1_definition: *id(42).value(),
            reserve0: 800,
            reserve1: 0,
            policies: EventPoliciesV1 {
                ending: lbp_core::EndingPolicy::Either,
                pause: lbp_core::PausePolicy::Disabled,
                sell_fee_bps: 0,
            }
        }
    );
    let (_, response) = activation(0);
    assert_eq!(
        sale_events(&response)[0].kind,
        SaleEventKindV1::Activated {
            reserve0: 800,
            reserve1: 0,
            collateral_seed: 0
        }
    );
    assert!(!state_of(&response).closure_observed);
}

#[test]
fn fixed_trade_events_match_both_transfer_forms_and_saved_fee_accounting() {
    use curve_core::events::*;
    let (input, state) = funded();
    for (message, form) in [
        (exact_in(5), TradeFormV1::ExactInput),
        (
            Message::BuyExactOutput {
                membership: None,
                amount_out: 3,
                max_amount_in: 6,
                now: 100,
            },
            TradeFormV1::ExactOutput,
        ),
    ] {
        let response = operation(
            &input,
            &state,
            message,
            config(100, id(43), false),
            [0, 800, 1000],
        )
        .unwrap();
        let next = state_of(&response);
        let events = sale_events(&response);
        assert_eq!(events.len(), 1);
        let gross = next.recorded_collateral + next.accrued_fees;
        assert_eq!(
            events[0].kind,
            SaleEventKindV1::Trade {
                trader: *id(50).value(),
                form,
                token_in: *id(42).value(),
                token_out: *id(41).value(),
                amount_in: gross,
                pricing_input: next.recorded_collateral,
                gross_output: 800 - next.remaining_inventory,
                amount_out: 800 - next.remaining_inventory,
                fee_definition: *id(42).value(),
                fee_amount: next.accrued_fees,

                fee_bps: 100,

                retained_fee: 0,

                sell_fee_bps: 0,
                pricing_time: None
            }
        );
    }
}

#[test]
fn fixed_sellout_events_follow_selected_ending_without_fabricated_depletion() {
    use curve_core::events::*;
    let (input, original) = funded();
    for ending in [
        lbp_core::EndingPolicy::Scheduled,
        lbp_core::EndingPolicy::Depletion,
        lbp_core::EndingPolicy::Either,
    ] {
        let mut state = original.clone();
        state.ending_policy = ending;
        state.remaining_inventory = 2;
        if ending == lbp_core::EndingPolicy::Depletion {
            state.end_timestamp = None;
        }
        let response = operation(
            &input,
            &state,
            exact_in(5),
            config(100, id(43), false),
            [0, 2, 1000],
        )
        .unwrap();
        let next = state_of(&response);
        let depleted = ending != lbp_core::EndingPolicy::Scheduled;
        assert_eq!(next.closure_observed, depleted);
        let events = sale_events(&response);
        assert_eq!(events.len(), 1 + usize::from(depleted));
        assert!(matches!(&events[0].kind, SaleEventKindV1::Trade { .. }));
        if depleted {
            assert_eq!(
                events[1].kind,
                SaleEventKindV1::Closed {
                    reason: ClosureReasonV1::Depleted {
                        definition: *id(41).value()
                    },
                    inclusion_bounds: EventTimeBoundsV1 {
                        at_least: None,
                        before: None
                    }
                }
            );
        }
        let settled = settle(&input, &next, 0, config(100, id(43), false), [5, 0]).unwrap();
        let close_events: Vec<_> = sale_events(&settled)
            .into_iter()
            .filter(|e| matches!(e.kind, SaleEventKindV1::Closed { .. }))
            .collect();
        assert_eq!(close_events.len(), usize::from(!depleted));
        assert_eq!(
            settled.validity.timestamps.start(),
            if depleted { None } else { Some(200_000) }
        );
        if !depleted {
            assert!(matches!(
                close_events[0].kind,
                SaleEventKindV1::Closed {
                    reason: ClosureReasonV1::ScheduledEnd { timestamp: 200 },
                    ..
                }
            ));
        }
    }
}

#[test]
fn fee_events_distinguish_liability_from_self_recipient_token_movement() {
    use curve_core::events::*;
    let (input, mut state) = funded();
    state.accrued_fees = 3;
    for treasury in [id(43), input.receiver.account_id] {
        let response = operation(
            &input,
            &state,
            Message::CollectFees,
            config(100, treasury, false),
            [20, 800, 0],
        )
        .unwrap();
        assert_eq!(
            sale_events(&response)[0].kind,
            SaleEventKindV1::FeesCollected {
                definition: *id(42).value(),
                treasury: *treasury.value(),
                liability_discharged: 3,
                tokens_transferred: if treasury == input.receiver.account_id {
                    0
                } else {
                    3
                }
            }
        );
        let next = state_of(&response);
        let repeated = operation(
            &input,
            &next,
            Message::CollectFees,
            config(100, treasury, false),
            [17, 800, 0],
        )
        .unwrap();
        assert!(sale_events(&repeated).is_empty());
    }
}

#[test]
fn legacy_fixed_settlement_events_preserve_end_bound_and_inventory_record() {
    use curve_core::events::*;
    let (input, mut state) = funded();
    state.recorded_collateral = 49;
    state.accrued_fees = 2;
    let response = settle(&input, &state, 0, config(100, id(43), true), [1000, 900]).unwrap();
    let next = state_of(&response);
    assert!(next.closure_observed);
    assert_eq!(next.remaining_inventory, 800);
    assert_eq!(response.validity.timestamps.start(), Some(200_000));
    let kinds: Vec<_> = sale_events(&response).into_iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            SaleEventKindV1::Closed {
                reason: ClosureReasonV1::ScheduledEnd { timestamp: 200 },
                inclusion_bounds: EventTimeBoundsV1 {
                    at_least: Some(200),
                    before: None
                }
            },
            SaleEventKindV1::FeesCollected {
                definition: *id(42).value(),
                treasury: *id(43).value(),
                liability_discharged: 2,
                tokens_transferred: 2
            },
            SaleEventKindV1::Settlement {
                step: SettlementStepV1::Payout,
                route: Some(SettlementRouteV1::Manual),
                effects: vec![SettlementEffectV1::Payout {
                    definition: *id(41).value(),
                    recipient: *id(60).value(),
                    purpose: PayoutPurposeV1::CreatorLaunch,
                    liability_discharged: 800,
                    tokens_transferred: 800
                }]
            },
            SaleEventKindV1::Settlement {
                step: SettlementStepV1::Payout,
                route: Some(SettlementRouteV1::Manual),
                effects: vec![SettlementEffectV1::Payout {
                    definition: *id(42).value(),
                    recipient: *id(60).value(),
                    purpose: PayoutPurposeV1::CreatorProceeds,
                    liability_discharged: 49,
                    tokens_transferred: 49
                }]
            },
            SaleEventKindV1::Settlement {
                step: SettlementStepV1::Completed,
                route: Some(SettlementRouteV1::Manual),
                effects: vec![]
            },
        ]
    );
    let repeated = settle(&input, &next, 0, config(100, id(43), true), [0, 0]).unwrap();
    assert!(sale_events(&repeated).is_empty());
}

#[test]
fn guarded_settlement_emits_completion_only_after_exact_factory_finalization() {
    use curve_core::events::*;
    let (mut input, mut state) = funded();
    state.recorded_collateral = 49;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.message = borsh::to_vec(&Message::Settle {
        route: lbp_core::SettlementRoute::Manual,
        creator: id(60),
        adapter: None,
    })
    .unwrap();
    let mut response = handle(&input).unwrap();
    assert!(sale_events(&response).is_empty());
    for bytes in [
        config(100, id(43), false),
        holding(input.receiver.account_id, id(42), 1000),
        holding(input.receiver.account_id, id(41), 900),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle(&input).unwrap();
    }
    assert!(!sale_events(&response).iter().any(|e| matches!(
        &e.kind,
        SaleEventKindV1::Settlement {
            step: SettlementStepV1::Completed,
            ..
        }
    )));
    let lbp_core::SettlementReply::Applied { receipt } =
        borsh::from_slice(&response.sync_messages.last().unwrap().message).unwrap()
    else {
        panic!("expected receipt")
    };
    input.pre_state = response.post_state.unwrap();
    input.from = Some(receipt.factory);
    input.message = borsh::to_vec(&Message::FinalizeSettlement {
        receipt_hash: [0; 32],
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidState));
    input.message = borsh::to_vec(&Message::FinalizeSettlement {
        receipt_hash: receipt.hash(),
    })
    .unwrap();
    let done = handle(&input).unwrap();
    assert_eq!(sale_events(&done).len(), 1);
    assert_eq!(
        sale_events(&done)[0].kind,
        SaleEventKindV1::Settlement {
            step: SettlementStepV1::Completed,
            route: Some(SettlementRouteV1::Manual),
            effects: vec![]
        }
    );
    assert!(state_of(&done).closure_observed);
}
