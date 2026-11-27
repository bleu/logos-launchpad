use super::fixture::{
    config, control, exact_in, funded, id, operation, prepare, sale_events, settle, state_of,
};
use lbp_core::{Message, handle};
use lee_core::account::Actor;

#[test]
fn fixed_controls_preserve_policy_and_emit_only_changes_before_start() {
    let mut input = prepare();
    let Message::PrepareFixedPrice { mut inputs } = borsh::from_slice(&input.message).unwrap()
    else {
        unreachable!()
    };
    for policy in [
        lbp_core::PausePolicy::Disabled,
        lbp_core::PausePolicy::AuthorityControlled,
    ] {
        inputs.pause_policy = policy;
        input.message = borsh::to_vec(&Message::PrepareFixedPrice {
            inputs: inputs.clone(),
        })
        .unwrap();
        let state = state_of(&handle(&input).unwrap());
        assert_eq!(state.trade_control.policy(), policy);
        assert!(!state.trade_control.is_paused());
    }
    let (input, mut state) = funded();
    state.trade_control = lbp_core::TradeControl::new(lbp_core::PausePolicy::AuthorityControlled);
    let original = state.clone();
    for paused in [false, true, true, false, false] {
        let response = control(&input, &state, paused).unwrap();
        assert_eq!(response.validity.timestamps.start(), None);
        assert_eq!(response.validity.timestamps.end(), Some(200_000));
        assert_eq!(
            response.events.len(),
            usize::from(state.trade_control.is_paused() != paused)
        );
        if state.trade_control.is_paused() != paused {
            assert_eq!(
                sale_events(&response)[0].kind,
                curve_core::events::SaleEventKindV1::PauseChanged { paused }
            );
        }
        assert!(response.sync_messages.is_empty());
        state = state_of(&response);
        assert_eq!(state.trade_control.is_paused(), paused);
        assert_eq!(
            state.trade_control.policy(),
            lbp_core::PausePolicy::AuthorityControlled
        );
        let mut unchanged = state.clone();
        unchanged.trade_control = original.trade_control;
        assert_eq!(unchanged, original);
    }
}

#[test]
fn fixed_control_noops_authenticate_and_reject_disabled_unfunded_and_ended_sales() {
    let (mut input, mut state) = funded();
    for paused in [false, true] {
        assert_eq!(
            control(&input, &state, paused).err(),
            Some(lbp_core::Error::PauseDisabled)
        );
    }
    state.trade_control = lbp_core::TradeControl::new(lbp_core::PausePolicy::AuthorityControlled);
    for paused in [false, true] {
        state.trade_control.set_paused(paused).unwrap();
        input.message = borsh::to_vec(&Message::SetPaused { paused }).unwrap();
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        for from in [
            None,
            Some(Actor::new(id(60), id(8))),
            Some(Actor::new(id(99), state.factory_program_id)),
            Some(Actor::new(state.factory_owner, id(99))),
        ] {
            input.from = from;
            input.is_authorized = true;
            assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
        }
        let mut unfunded = state.clone();
        unfunded.funded = false;
        assert_eq!(
            control(&input, &unfunded, paused).err(),
            Some(lbp_core::Error::PendingFunding)
        );
        for end in [None, Some(200)] {
            let mut sold = state.clone();
            sold.end_timestamp = end;
            sold.ending_policy = if end.is_some() {
                lbp_core::EndingPolicy::Either
            } else {
                lbp_core::EndingPolicy::Depletion
            };
            sold.remaining_inventory = 0;
            assert_eq!(
                control(&input, &sold, paused).err(),
                Some(lbp_core::Error::Closed)
            );
        }
        let mut settled = state.clone();
        settled.settlement_progress = lbp_core::SettlementProgress::ManualComplete;
        assert_eq!(
            control(&input, &settled, paused).err(),
            Some(lbp_core::Error::Closed)
        );
        let mut overflow = state.clone();
        overflow.end_timestamp = Some(u64::MAX);
        assert_eq!(
            control(&input, &overflow, paused).err(),
            Some(lbp_core::Error::InvalidTerms)
        );
        let mut open = state.clone();
        open.end_timestamp = None;
        open.ending_policy = lbp_core::EndingPolicy::Depletion;
        let response = control(&input, &open, paused).unwrap();
        assert_eq!(response.validity.timestamps.end(), None);
    }
    input.pre_state = vec![].into();
    input.receiver.account_id = id(60);
    input.message = borsh::to_vec(&Message::Execute {
        pool: prepare().receiver.account_id,
        instruction: Box::new(Message::SetPaused { paused: true }),
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
}

#[test]
fn fixed_pause_gates_both_buys_and_resume_keeps_namespace_freeze() {
    let (input, mut state) = funded();
    state.trade_control = lbp_core::TradeControl::new(lbp_core::PausePolicy::AuthorityControlled);
    state = state_of(&control(&input, &state, true).unwrap());
    for message in [
        exact_in(5),
        Message::BuyExactOutput {
            membership: None,
            amount_out: 3,
            max_amount_in: 6,
            now: 100,
        },
    ] {
        assert_eq!(
            operation(
                &input,
                &state,
                message.clone(),
                config(100, id(43), false),
                [0, 800, 100]
            )
            .err(),
            Some(lbp_core::Error::Paused)
        );
        let resumed = state_of(&control(&input, &state, false).unwrap());
        assert_eq!(
            operation(
                &input,
                &resumed,
                message.clone(),
                config(100, id(43), true),
                [0, 800, 100]
            )
            .err(),
            Some(lbp_core::Error::Frozen)
        );
        assert!(
            operation(
                &input,
                &resumed,
                message,
                config(100, id(43), false),
                [0, 800, 100]
            )
            .is_ok()
        );
    }
}

#[test]
fn fixed_pause_and_freeze_leave_collection_and_safe_settlement_available() {
    let (input, mut state) = funded();
    state.trade_control = lbp_core::TradeControl::new(lbp_core::PausePolicy::AuthorityControlled);
    state = state_of(&control(&input, &state, true).unwrap());
    state.accrued_fees = 1;
    state.recorded_collateral = 4;
    let collected = operation(
        &input,
        &state,
        Message::CollectFees,
        config(100, id(43), true),
        [5, 800, 0],
    )
    .unwrap();
    assert_eq!(state_of(&collected).accrued_fees, 0);
    let response = settle(&input, &state, 100, config(100, id(43), true), [5, 800]).unwrap();
    assert_eq!(response.validity.timestamps.start(), Some(200_000));
    assert!(state_of(&response).settlement_progress.is_complete());
    assert!(state_of(&response).trade_control.is_paused());
    state.end_timestamp = None;
    state.ending_policy = lbp_core::EndingPolicy::Depletion;
    assert_eq!(
        settle(
            &input,
            &state,
            u64::MAX,
            config(100, id(43), true),
            [5, 800]
        )
        .err(),
        Some(lbp_core::Error::NotClosed)
    );
    state.remaining_inventory = 0;
    let response = settle(&input, &state, 100, config(100, id(43), true), [5, 0]).unwrap();
    assert_eq!(response.validity.timestamps.start(), None);
    assert!(state_of(&response).settlement_progress.is_complete());
}

#[test]
fn fixed_completed_progress_blocks_controls_and_both_buy_forms() {
    use lbp_core::SettlementProgress;
    let (input, mut state) = funded();
    state.trade_control = lbp_core::TradeControl::new(lbp_core::PausePolicy::AuthorityControlled);
    for progress in [
        SettlementProgress::ManualComplete,
        SettlementProgress::Graduated,
    ] {
        state.settlement_progress = progress;
        assert_eq!(
            control(&input, &state, false).err(),
            Some(lbp_core::Error::Closed)
        );
        for message in [
            exact_in(5),
            Message::BuyExactOutput {
                membership: None,
                amount_out: 3,
                max_amount_in: 6,
                now: 100,
            },
        ] {
            assert_eq!(
                operation(
                    &input,
                    &state,
                    message,
                    config(100, id(43), false),
                    [0, 800, 100]
                )
                .err(),
                Some(lbp_core::Error::Closed)
            );
        }
    }
}
