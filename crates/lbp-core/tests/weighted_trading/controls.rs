use super::fixture::{config, funded, id, operation, settle, state_of};
use lbp_core::{Message, handle};
use lee_core::account::Actor;

#[test]
fn weighted_pause_requires_factory_authorization() {
    let (mut input, state) = funded();
    // Factory owner is the authorized caller.
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SetWeightedPaused { paused: true }).unwrap();
    assert!(handle(&input).is_ok());

    // Wrong program ID
    input.from = Some(Actor::new(state.factory_owner, id(99)));
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));

    // Wrong account ID
    input.from = Some(Actor::new(id(99), state.factory_program_id));
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));

    // No sender
    input.from = None;
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));

    // Random buyer cannot pause
    input.from = Some(Actor::new(id(50), id(8)));
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
}

#[test]
fn weighted_repeated_pause_succeeds_without_event() {
    let (mut input, state) = funded();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();

    input.message = borsh::to_vec(&Message::SetWeightedPaused { paused: true }).unwrap();
    let response = handle(&input).unwrap();
    assert!(!response.events.is_empty(), "first pause should emit event");
    let paused_state =
        lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(paused_state.trade_control.is_paused());

    input.pre_state = response.post_state.unwrap();
    let response = handle(&input).unwrap();
    assert!(
        response.events.is_empty(),
        "repeated pause should not emit event"
    );

    input.pre_state = response.post_state.unwrap();
    input.message = borsh::to_vec(&Message::SetWeightedPaused { paused: false }).unwrap();
    let response = handle(&input).unwrap();
    assert!(!response.events.is_empty(), "resume should emit event");

    input.pre_state = response.post_state.unwrap();
    let response = handle(&input).unwrap();
    assert!(
        response.events.is_empty(),
        "repeated resume should not emit event"
    );
}

#[test]
fn weighted_pause_blocks_both_buy_forms() {
    let (mut input, mut state) = funded();
    state.trade_control.set_paused(true).unwrap();
    for message in [
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 50,
            min_amount_out: 1,
        },
        Message::BuyWeightedExactOutput {
            membership: None,
            amount_out: 500,
            max_amount_in: 1000,
        },
    ] {
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.message = borsh::to_vec(&message).unwrap();
        // Reject at entry, before any namespace, holding or clock request.
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::Paused));
    }
}

#[test]
fn weighted_resume_cannot_override_namespace_freeze() {
    let (input, state) = funded();
    // Even when unpaused, frozen config blocks buys
    assert_eq!(
        operation(
            &input,
            &state,
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: 1,
            },
            config(0, id(43), true),
            [100, 1_000_000, 1000]
        )
        .err(),
        Some(lbp_core::Error::Frozen)
    );
}

#[test]
fn weighted_fees_collectible_when_paused() {
    let (input, mut state) = funded();
    state.accrued_fees = 5;
    state.recorded_collateral = 100;
    state.trade_control.set_paused(true).unwrap();
    let response = operation(
        &input,
        &state,
        Message::CollectFees,
        config(0, id(43), false),
        [105, state.remaining_inventory, 0],
    )
    .unwrap();
    let next = state_of(&response);
    assert_eq!(next.accrued_fees, 0);
}

#[test]
fn weighted_pause_rejects_after_settlement() {
    let (mut input, mut state) = funded();
    state.settlement_progress = lbp_core::SettlementProgress::ManualComplete;
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SetWeightedPaused { paused: true }).unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Closed));
}

#[test]
fn weighted_pause_requires_funded_pool() {
    let (mut input, mut state) = funded();
    state.funded = false;
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SetWeightedPaused { paused: true }).unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::PendingFunding));
}

#[test]
fn fixed_price_pool_rejects_weighted_pause() {
    let (mut input, state) = funded();
    let mut fixed = state.clone();
    fixed.mode = lbp_core::PoolMode::FixedPrice;

    fixed.ending_policy = lbp_core::EndingPolicy::Either;
    input.from = Some(Actor::new(fixed.factory_owner, fixed.factory_program_id));
    input.pre_state = borsh::to_vec(&fixed).unwrap().into();
    input.message = borsh::to_vec(&Message::SetWeightedPaused { paused: true }).unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
}

#[test]
fn common_weighted_controls_match_legacy_events_and_keep_the_scheduled_end() {
    let (mut input, mut state) = funded();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    state.remaining_inventory = 0;
    for paused in [false, true, true, false, false] {
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.message = borsh::to_vec(&Message::SetWeightedPaused { paused }).unwrap();
        let legacy = handle(&input).unwrap();
        input.message = borsh::to_vec(&Message::SetPaused { paused }).unwrap();
        let common = handle(&input).unwrap();
        assert_eq!(common.post_state, legacy.post_state);
        assert_eq!(common.events.len(), legacy.events.len());
        let changed = state.trade_control.is_paused() != paused;
        assert_eq!(common.events.len(), 2 * usize::from(changed));
        if changed {
            let selector = [109, 140, 24, 3, 232, 43, 58, 157];
            let common_event = common
                .events
                .iter()
                .find(|e| e.selector == selector)
                .unwrap();
            let legacy_event = legacy
                .events
                .iter()
                .find(|e| e.selector == selector)
                .unwrap();
            assert_eq!(common_event.data, borsh::to_vec(&paused).unwrap());
            assert_eq!(common_event.data, legacy_event.data);
        }
        assert_eq!(common.validity.timestamps.start(), None);
        assert_eq!(common.validity.timestamps.end(), Some(3_000_000));
        state = state_of(&common);
        assert_eq!(
            state.trade_control.policy(),
            lbp_core::PausePolicy::AuthorityControlled
        );
        for from in [
            None,
            Some(Actor::new(id(50), id(8))),
            Some(Actor::new(id(99), state.factory_program_id)),
            Some(Actor::new(state.factory_owner, id(99))),
        ] {
            let mut forged = input.clone();
            forged.from = from;
            forged.pre_state = common.post_state.clone().unwrap();
            forged.is_authorized = true;
            assert_eq!(handle(&forged).err(), Some(lbp_core::Error::Unauthorized));
        }
    }
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), true),
        [100, 0],
    )
    .unwrap();
    assert_eq!(response.validity.timestamps.start(), Some(3_000_000));
}

#[test]
fn weighted_controls_reject_disabled_unfunded_settled_and_overflow_even_on_noops() {
    let (mut input, original) = funded();
    input.from = Some(Actor::new(
        original.factory_owner,
        original.factory_program_id,
    ));
    for legacy in [false, true] {
        for paused in [false, true] {
            for case in 0..4 {
                let mut state = original.clone();
                state.trade_control.set_paused(paused).unwrap();
                let expected = match case {
                    0 => {
                        state.trade_control = lbp_core::TradeControl::Unrestricted;
                        lbp_core::Error::PauseDisabled
                    }
                    1 => {
                        state.funded = false;
                        lbp_core::Error::PendingFunding
                    }
                    2 => {
                        state.settlement_progress = lbp_core::SettlementProgress::ManualComplete;
                        lbp_core::Error::Closed
                    }
                    _ => {
                        state.end_timestamp = Some(u64::MAX);
                        lbp_core::Error::InvalidTerms
                    }
                };
                input.pre_state = borsh::to_vec(&state).unwrap().into();
                input.message = borsh::to_vec(&if legacy {
                    Message::SetWeightedPaused { paused }
                } else {
                    Message::SetPaused { paused }
                })
                .unwrap();
                assert_eq!(handle(&input).err(), Some(expected));
            }
        }
    }
}

#[test]
fn common_weighted_resume_keeps_namespace_freeze_for_both_buys() {
    let (mut input, state) = funded();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SetPaused { paused: true }).unwrap();
    let response = handle(&input).unwrap();
    input.pre_state = response.post_state.unwrap();
    input.message = borsh::to_vec(&Message::SetPaused { paused: false }).unwrap();
    let resumed = state_of(&handle(&input).unwrap());
    input.from = Some(Actor::new(id(50), id(8)));
    for message in [
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        Message::BuyWeightedExactOutput {
            membership: None,
            amount_out: 500,
            max_amount_in: 1000,
        },
    ] {
        assert_eq!(
            operation(
                &input,
                &resumed,
                message.clone(),
                config(0, id(43), true),
                [100, 1_000_000, 1000]
            )
            .err(),
            Some(lbp_core::Error::Frozen)
        );
        assert!(
            operation(
                &input,
                &resumed,
                message,
                config(0, id(43), false),
                [100, 1_000_000, 1000]
            )
            .is_ok()
        );
    }
}

#[test]
fn weighted_completed_progress_blocks_controls_and_both_buy_forms() {
    use lbp_core::SettlementProgress;
    let (mut input, mut state) = funded();
    for progress in [
        SettlementProgress::ManualComplete,
        SettlementProgress::Graduated,
    ] {
        state.settlement_progress = progress;
        input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        for message in [
            Message::SetPaused { paused: false },
            Message::SetWeightedPaused { paused: false },
        ] {
            input.message = borsh::to_vec(&message).unwrap();
            assert_eq!(handle(&input).err(), Some(lbp_core::Error::Closed));
        }
        input.from = Some(Actor::new(id(50), id(8)));
        for message in [
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: 1,
            },
            Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 500,
                max_amount_in: 1000,
            },
        ] {
            assert_eq!(
                operation(
                    &input,
                    &state,
                    message,
                    config(100, id(43), false),
                    [100, state.remaining_inventory, 1000]
                )
                .err(),
                Some(lbp_core::Error::Closed)
            );
        }
    }
}
