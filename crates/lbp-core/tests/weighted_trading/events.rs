use super::fixture::{
    checkpoint_events, config, definition, funded, holding, id, namespace_state, operation_at,
    prepared, reply, sale_events, settle, state_of,
};
use lbp_core::{Message, handle};
use lee_core::account::Actor;
use lee_core::program::ReceiveInput;

#[test]
fn weighted_trade_events_use_net_pricing_input_and_trusted_time_for_both_forms() {
    use curve_core::events::*;
    let (input, state) = funded();
    for (message, form) in [
        (
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: 1,
            },
            TradeFormV1::ExactInput,
        ),
        (
            Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 500,
                max_amount_in: 1000,
            },
            TradeFormV1::ExactOutput,
        ),
    ] {
        let response = operation_at(
            1_500_999,
            &input,
            &state,
            message,
            config(100, id(43), false),
            [100, state.remaining_inventory, 1000],
        )
        .unwrap();
        let next = state_of(&response);
        let events = sale_events(&response);
        assert_eq!(events.len(), 1);
        let fees = next.accrued_fees - state.accrued_fees;
        let gross = next.recorded_collateral - state.recorded_collateral + fees;
        assert_eq!(events[0].engine, PricingEngineV1::Weighted);
        assert_eq!(
            events[0].kind,
            SaleEventKindV1::Trade {
                trader: *id(50).value(),
                form,
                token_in: *id(42).value(),
                token_out: *id(41).value(),
                amount_in: gross,
                pricing_input: gross - fees,
                gross_output: state.remaining_inventory - next.remaining_inventory,
                amount_out: state.remaining_inventory - next.remaining_inventory,
                fee_definition: *id(42).value(),
                fee_amount: fees,

                fee_bps: 100,

                retained_fee: 0,

                sell_fee_bps: 0,
                pricing_time: Some(1500)
            }
        );
        assert_eq!(checkpoint_events(&response).len(), 1);
        assert!(!next.closure_observed);
    }
}

#[test]
fn weighted_settlement_events_report_both_assets_and_scheduled_closure() {
    use curve_core::events::*;
    let (input, original) = funded();
    for proceeds in [100, 149] {
        let mut state = original.clone();
        state.recorded_collateral = proceeds;
        state.accrued_fees = 2;
        if let lbp_core::PoolMode::Weighted {
            pricing_collateral, ..
        } = &mut state.mode
        {
            *pricing_collateral = proceeds;
        }
        let response = settle(
            &input,
            &state,
            Message::SettleWeighted { creator: id(50) },
            config(100, id(43), false),
            [1000, state.remaining_inventory],
        )
        .unwrap();
        let next = state_of(&response);
        assert!(next.closure_observed);
        assert_eq!(response.validity.timestamps.start(), Some(3_000_000));
        let mut expected = vec![
            SaleEventKindV1::Closed {
                reason: ClosureReasonV1::ScheduledEnd { timestamp: 3000 },
                inclusion_bounds: EventTimeBoundsV1 {
                    at_least: Some(3000),
                    before: None,
                },
            },
            SaleEventKindV1::FeesCollected {
                definition: *id(42).value(),
                treasury: *id(43).value(),
                liability_discharged: 2,
                tokens_transferred: 2,
            },
            SaleEventKindV1::Settlement {
                step: SettlementStepV1::Payout,
                route: Some(SettlementRouteV1::Manual),
                effects: vec![SettlementEffectV1::Payout {
                    definition: *id(41).value(),
                    recipient: *id(50).value(),
                    purpose: PayoutPurposeV1::CreatorLaunch,
                    liability_discharged: state.remaining_inventory,
                    tokens_transferred: state.remaining_inventory,
                }],
            },
            SaleEventKindV1::Settlement {
                step: SettlementStepV1::Payout,
                route: Some(SettlementRouteV1::Manual),
                effects: vec![SettlementEffectV1::Payout {
                    definition: *id(42).value(),
                    recipient: *id(50).value(),
                    purpose: PayoutPurposeV1::CreatorProceeds,
                    liability_discharged: proceeds,
                    tokens_transferred: proceeds,
                }],
            },
        ];
        expected.push(SaleEventKindV1::Settlement {
            step: SettlementStepV1::Completed,
            route: Some(SettlementRouteV1::Manual),
            effects: vec![],
        });
        assert_eq!(
            sale_events(&response)
                .into_iter()
                .map(|e| e.kind)
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(next.remaining_inventory, state.remaining_inventory);
    }
}

#[test]
fn weighted_depletion_waits_for_schedule_and_zero_manual_completion_blocks_hooks() {
    use curve_core::events::*;
    let (input, mut state) = funded();
    state.remaining_inventory = 0;
    state.graduation_adapter = None;
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(100, id(43), false),
        [100, 0],
    )
    .unwrap();
    let next = state_of(&response);
    assert!(matches!(
        &sale_events(&response)[0].kind,
        SaleEventKindV1::Closed {
            reason: ClosureReasonV1::ScheduledEnd { timestamp: 3000 },
            ..
        }
    ));
    assert_eq!(response.validity.timestamps.start(), Some(3_000_000));
    assert_eq!(
        next.settlement_progress,
        lbp_core::SettlementProgress::ManualComplete
    );
    assert_eq!(
        sale_events(&response).last().unwrap().kind,
        SaleEventKindV1::Settlement {
            step: SettlementStepV1::Completed,
            route: Some(SettlementRouteV1::Manual),
            effects: vec![],
        }
    );
    assert_eq!(
        settle(
            &input,
            &next,
            Message::Settle {
                route: lbp_core::SettlementRoute::Graduation,
                creator: id(50),
                adapter: next.graduation_adapter,
            },
            config(100, id(43), false),
            [900, 0],
        )
        .err(),
        Some(lbp_core::Error::InvalidTerms)
    );
    let manual_retry = handle(&ReceiveInput {
        receiver: input.receiver,
        from: Some(Actor::new(next.factory_owner, next.factory_program_id)),
        is_authorized: false,
        pre_state: borsh::to_vec(&next).unwrap().into(),
        message: borsh::to_vec(&Message::SettleWeighted { creator: id(50) }).unwrap(),
    })
    .unwrap();
    assert!(sale_events(&manual_retry).is_empty());
    assert_eq!(state_of(&manual_retry), next);
    state.graduation_adapter = Some(Actor::new(id(70), id(71)));
    let hook = settle(
        &input,
        &state,
        Message::Settle {
            route: lbp_core::SettlementRoute::Graduation,
            creator: id(50),
            adapter: state.graduation_adapter,
        },
        config(100, id(43), false),
        [100, 0],
    )
    .unwrap();
    assert_eq!(sale_events(&hook).len(), sale_events(&response).len() - 1);
    assert_eq!(hook.validity.timestamps.start(), Some(3_000_000));
    let lbp_core::SettlementReply::Applied { receipt } =
        borsh::from_slice(&hook.sync_messages.last().unwrap().message).unwrap()
    else {
        panic!("expected receipt")
    };
    let done = handle(&ReceiveInput {
        receiver: input.receiver,
        from: Some(receipt.factory),
        is_authorized: false,
        pre_state: hook.post_state.unwrap(),
        message: borsh::to_vec(&Message::FinalizeSettlement {
            receipt_hash: receipt.hash(),
        })
        .unwrap(),
    })
    .unwrap();
    assert_eq!(sale_events(&done).len(), 1);
    assert_eq!(
        sale_events(&done)[0].kind,
        SaleEventKindV1::Settlement {
            step: SettlementStepV1::Completed,
            route: Some(SettlementRouteV1::Graduation),
            effects: vec![]
        }
    );
    let completed = state_of(&done);
    let repeated = handle(&ReceiveInput {
        receiver: input.receiver,
        from: Some(Actor::new(
            completed.factory_owner,
            completed.factory_program_id,
        )),
        is_authorized: false,
        pre_state: borsh::to_vec(&completed).unwrap().into(),
        message: borsh::to_vec(&Message::SettleWeighted { creator: id(51) }).unwrap(),
    });
    assert_eq!(repeated.err(), Some(lbp_core::Error::InvalidTerms));
}

#[test]
fn weighted_setup_events_preserve_configured_seed_and_activated_accounting() {
    use curve_core::events::*;
    let (mut input, prepared) = prepared();
    let state = state_of(&prepared);
    let events = sale_events(&prepared);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].engine, PricingEngineV1::Weighted);
    assert_eq!(
        events[0].kind,
        SaleEventKindV1::Prepared {
            owner: *state.factory_owner.value(),
            token0_definition: *id(41).value(),
            token1_definition: *id(42).value(),
            reserve0: 1_000_000,
            reserve1: 100,
            policies: EventPoliciesV1 {
                ending: lbp_core::EndingPolicy::Scheduled,
                pause: lbp_core::PausePolicy::AuthorityControlled,
                sell_fee_bps: 0,
            }
        }
    );
    assert!(!state.closure_observed);
    input.pre_state = prepared.post_state.unwrap();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.message = borsh::to_vec(&Message::ActivateWeighted).unwrap();
    let mut response = handle(&input).unwrap();
    assert!(sale_events(&response).is_empty());
    for bytes in [
        namespace_state(),
        definition(),
        definition(),
        holding(state.factory_owner, id(41), 0),
        holding(input.receiver.account_id, id(41), 1_000_000),
        holding(input.receiver.account_id, id(42), 100),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle(&input).unwrap();
    }
    assert!(sale_events(&response).is_empty());
    assert_eq!(response.sync_messages[0].to, clock_core::clock_actor());
    input.pre_state = response.post_state.unwrap();
    input.from = Some(clock_core::clock_actor());
    input.message = borsh::to_vec(&clock_core::Reply::Timestamp(1_000_000)).unwrap();
    response = handle(&input).unwrap();
    assert_eq!(sale_events(&response).len(), 1);
    assert_eq!(
        sale_events(&response)[0].kind,
        SaleEventKindV1::Activated {
            reserve0: 1_000_000,
            reserve1: 100,
            collateral_seed: 100
        }
    );
    assert!(!state_of(&response).closure_observed);
}
