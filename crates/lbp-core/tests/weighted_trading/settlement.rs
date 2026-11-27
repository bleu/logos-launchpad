use super::fixture::{config, funded, id, operation, settle, state_of};
use lbp_core::{Message, handle};
use lee_core::account::{AccountId, Actor};

#[test]
fn fixed_price_settlement_cannot_settle_a_weighted_pool() {
    let (mut input, state) = funded();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SettleFixedPrice {
        creator: id(50),
        now: state.end_timestamp.unwrap(),
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
}

#[test]
fn weighted_settlement_pays_inventory_and_recorded_collateral_and_collects_fees() {
    let (input, mut state) = funded();
    // Simulate a buy: 50 in, fee 1, net 49 recorded
    state.recorded_collateral = 149; // seed(100) + net(49)
    state.accrued_fees = 1;
    state.remaining_inventory = 999_500;
    if let lbp_core::PoolMode::Weighted {
        pricing_collateral, ..
    } = &mut state.mode
    {
        *pricing_collateral = 149;
    }
    let vault_collateral = 191; // 150 liabilities + 41 surplus
    let vault_token = 999_507; // remaining + 7 surplus (creator alloc mock)

    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(100, id(43), false),
        [vault_collateral, vault_token],
    )
    .unwrap();
    let next = state_of(&response);
    assert!(next.settlement_progress.is_complete());
    assert_eq!(next.recorded_collateral, 0);
    assert_eq!(next.accrued_fees, 0);

    let messages = &response.sync_messages;
    assert!(
        messages.len() >= 2,
        "expected fee, inventory and collateral transfers"
    );
}

#[test]
fn weighted_settlement_requires_factory_authorization() {
    let (mut input, state) = funded();
    input.from = Some(Actor::new(id(50), id(8)));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SettleWeighted { creator: id(50) }).unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
}

#[test]
fn weighted_settlement_rejects_default_creator() {
    let (input, state) = funded();
    let result = settle(
        &input,
        &state,
        Message::SettleWeighted {
            creator: AccountId::default(),
        },
        config(0, id(43), false),
        [100, 1_000_000],
    );
    assert_eq!(result.err(), Some(lbp_core::Error::Unauthorized));
}

#[test]
fn weighted_settlement_is_idempotent() {
    let (input, mut state) = funded();
    state.recorded_collateral = 100;
    state.accrued_fees = 0;
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), false),
        [100, 1_000_000],
    )
    .unwrap();
    let settled = state_of(&response);
    assert!(settled.settlement_progress.is_complete());

    let mut input2 = input.clone();
    input2.from = Some(Actor::new(
        settled.factory_owner,
        settled.factory_program_id,
    ));
    input2.pre_state = borsh::to_vec(&settled).unwrap().into();
    input2.message = borsh::to_vec(&Message::SettleWeighted { creator: id(50) }).unwrap();
    let response2 = handle(&input2).unwrap();
    let still_settled = state_of(&response2);
    assert!(still_settled.settlement_progress.is_complete());
    assert_eq!(still_settled.recorded_collateral, 0);
}

#[test]
fn weighted_settlement_enforces_time_bound_at_scheduled_end() {
    let (input, state) = funded();
    // Settlement response must have a time lower bound at the scheduled end.
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), false),
        [100, 1_000_000],
    )
    .unwrap();
    // The end_timestamp is 3000, so the time lower bound should be 3_000_000 ms
    assert_eq!(response.validity.timestamps.start(), Some(3_000_000));
}

#[test]
fn weighted_settlement_with_zero_fees_and_no_unsold() {
    let (input, mut state) = funded();
    state.recorded_collateral = 200;
    state.accrued_fees = 0;
    state.remaining_inventory = 0;
    if let lbp_core::PoolMode::Weighted {
        pricing_collateral, ..
    } = &mut state.mode
    {
        *pricing_collateral = 200;
    }
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), false),
        [200, 0],
    )
    .unwrap();
    let next = state_of(&response);
    assert!(next.settlement_progress.is_complete());
}

#[test]
fn weighted_settlement_rejects_underfunded_token_vault() {
    let (input, state) = funded();
    assert_eq!(
        settle(
            &input,
            &state,
            Message::SettleWeighted { creator: id(50) },
            config(0, id(43), false),
            [100, state.remaining_inventory - 1], // token vault below remaining inventory
        )
        .err(),
        Some(lbp_core::Error::Underfunded)
    );
}

#[test]
fn weighted_settlement_rejects_underfunded_collateral_vault() {
    let (input, state) = funded();
    assert_eq!(
        settle(
            &input,
            &state,
            Message::SettleWeighted { creator: id(50) },
            config(0, id(43), false),
            [99, state.remaining_inventory], // collateral vault below liabilities (seed=100)
        )
        .err(),
        Some(lbp_core::Error::Underfunded)
    );
}

#[test]
fn weighted_settlement_available_when_frozen() {
    let (input, state) = funded();
    // Settlement must work even when namespace is frozen
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), true), // frozen
        [100, 1_000_000],
    );
    assert!(response.is_ok());
}

#[test]
fn weighted_settlement_available_when_paused() {
    let (input, mut state) = funded();
    state.trade_control.set_paused(true).unwrap();
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), false),
        [100, 1_000_000],
    );
    assert!(response.is_ok());
}

#[test]
fn weighted_settlement_surplus_not_paid_to_creator() {
    let (input, mut state) = funded();
    state.recorded_collateral = 100; // seed only, no buys
    state.accrued_fees = 0;
    // Vault has surplus beyond recorded liabilities
    let vault_collateral = 500; // 400 surplus
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), false),
        [vault_collateral, 1_000_000],
    )
    .unwrap();
    let next = state_of(&response);
    assert!(next.settlement_progress.is_complete());
    // recorded_collateral is zeroed — only the original 100 is paid, not 500
    assert_eq!(next.recorded_collateral, 0);
}

#[test]
fn weighted_fixed_price_settlement_rejects_weighted_pool() {
    let (input, state) = funded();
    let mut input2 = input.clone();
    input2.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input2.pre_state = borsh::to_vec(&state).unwrap().into();
    input2.message = borsh::to_vec(&Message::SettleFixedPrice {
        creator: id(50),
        now: 3000,
    })
    .unwrap();
    assert_eq!(handle(&input2).err(), Some(lbp_core::Error::InvalidMessage));
}

#[test]
fn weighted_settlement_requires_funded_pool() {
    let (input, mut state) = funded();
    state.funded = false;
    let result = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), false),
        [100, 1_000_000],
    );
    assert_eq!(result.err(), Some(lbp_core::Error::PendingFunding));
}

#[test]
fn weighted_depleted_inventory_before_end_does_not_allow_early_settlement() {
    let (input, mut state) = funded();
    state.remaining_inventory = 0;
    state.recorded_collateral = 100;
    // Even with depleted inventory, the time bound is still the scheduled end
    let response = settle(
        &input,
        &state,
        Message::SettleWeighted { creator: id(50) },
        config(0, id(43), false),
        [100, 0],
    )
    .unwrap();
    // Must still enforce the end timestamp as lower bound
    assert_eq!(response.validity.timestamps.start(), Some(3_000_000));
}

#[test]
fn weighted_shared_closing_keeps_the_schedule_after_depletion_and_payout() {
    let (mut input, original) = funded();
    for remaining in [0, original.remaining_inventory] {
        let mut state = original.clone();
        state.remaining_inventory = remaining;
        assert_eq!(state.close_time_bound().unwrap(), Some(3000));
        let response = settle(
            &input,
            &state,
            Message::SettleWeighted { creator: id(50) },
            config(0, id(43), true),
            [117, remaining + 7],
        )
        .unwrap();
        let next = state_of(&response);
        assert_eq!(next.remaining_inventory, remaining);
        assert_eq!(next.close_time_bound().unwrap(), Some(3000));
        assert_eq!(response.validity.timestamps.start(), Some(3_000_000));
        assert_eq!(response.validity.timestamps.end(), None);
        input.from = Some(Actor::new(next.factory_owner, next.factory_program_id));
        input.pre_state = borsh::to_vec(&next).unwrap().into();
        input.message = borsh::to_vec(&Message::SettleWeighted { creator: id(50) }).unwrap();
        let retry = handle(&input).unwrap();
        assert!(retry.sync_messages.is_empty());
        assert_eq!(retry.validity.timestamps.start(), Some(3_000_000));
        assert_eq!(state_of(&retry), next);
    }
}

#[test]
fn weighted_either_settlement_requires_depletion_or_the_schedule_and_keeps_retry_evidence() {
    let (input, original) = funded();
    for ending in [
        lbp_core::EndingPolicy::Scheduled,
        lbp_core::EndingPolicy::Either,
    ] {
        for remaining in [0, 1] {
            let mut state = original.clone();
            state.ending_policy = ending;
            state.remaining_inventory = remaining;
            let expected = if ending == lbp_core::EndingPolicy::Either && remaining == 0 {
                None
            } else {
                Some(3_000_000)
            };
            let response = settle(
                &input,
                &state,
                Message::SettleWeighted { creator: id(50) },
                config(100, id(43), true),
                [100, remaining],
            )
            .unwrap();
            assert_eq!(response.validity.timestamps.start(), expected);
            let next = state_of(&response);
            assert!(next.settlement_progress.is_complete());
            assert_eq!(next.remaining_inventory, remaining);
            assert_eq!(
                next.close_time_bound().unwrap(),
                expected.map(|time| time / 1000)
            );
            let mut retry = input.clone();
            retry.from = Some(Actor::new(next.factory_owner, next.factory_program_id));
            retry.pre_state = borsh::to_vec(&next).unwrap().into();
            retry.message = borsh::to_vec(&Message::SettleWeighted { creator: id(50) }).unwrap();
            let repeated = handle(&retry).unwrap();
            assert_eq!(repeated.validity.timestamps.start(), expected);
            assert!(repeated.sync_messages.is_empty());
        }
    }
}

#[test]
fn weighted_collection_and_settlement_reject_unbacked_pricing_or_reduced_seed() {
    let (input, original) = funded();
    for (recorded, pricing) in [(99, 99), (100, 101), (101, 100)] {
        let mut state = original.clone();
        state.recorded_collateral = recorded;
        if let lbp_core::PoolMode::Weighted {
            pricing_collateral, ..
        } = &mut state.mode
        {
            *pricing_collateral = pricing;
        }
        assert_eq!(
            operation(
                &input,
                &state,
                Message::CollectFees,
                config(100, id(43), false),
                [1000, 1_000_000, 1000]
            )
            .err(),
            Some(lbp_core::Error::InvalidState)
        );
        assert_eq!(
            settle(
                &input,
                &state,
                Message::SettleWeighted { creator: id(50) },
                config(100, id(43), false),
                [1000, 1_000_000]
            )
            .err(),
            Some(lbp_core::Error::InvalidState)
        );
    }
}

#[test]
fn weighted_atomic_settlement_pays_seed_in_recorded_collateral_once() {
    use lbp_core::SettlementProgress;
    use lez_compat_core::token::Message as TokenMessage;
    let (input, original) = funded();
    for (proceeds, fees, inventory) in [
        (100, 0, 1_000_000),
        (100, 1, 999_500),
        (149, 1, 999_500),
        (100, 0, 0),
        (149, 1, 0),
    ] {
        let mut state = original.clone();
        state.recorded_collateral = proceeds;
        state.accrued_fees = fees;
        state.remaining_inventory = inventory;
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
            config(9999, id(43), true),
            [proceeds + fees + 17, inventory + 7],
        )
        .unwrap();
        let mut expected = state.clone();
        expected.recorded_collateral = 0;
        expected.accrued_fees = 0;
        expected.closure_observed = true;
        expected.settlement_progress = SettlementProgress::ManualComplete;
        assert_eq!(state_of(&response), expected);
        assert_eq!(
            response.post_state.as_ref().unwrap().to_vec(),
            borsh::to_vec(&expected).unwrap()
        );
        assert_eq!(response.validity.timestamps.start(), Some(3_000_000));
        assert_eq!(response.validity.timestamps.end(), None);
        let mut calls = vec![];
        if fees != 0 {
            calls.push(TokenMessage::Transfer {
                definition: id(42),
                recipient: id(43),
                amount: fees,
            });
        }
        if inventory != 0 {
            calls.push(TokenMessage::Transfer {
                definition: id(41),
                recipient: id(50),
                amount: inventory,
            });
        }
        calls.push(TokenMessage::Transfer {
            definition: id(42),
            recipient: id(50),
            amount: proceeds,
        });
        assert_eq!(
            response
                .sync_messages
                .iter()
                .map(|call| borsh::from_slice::<TokenMessage>(&call.message).unwrap())
                .collect::<Vec<_>>(),
            calls
        );
        let mut retry = input.clone();
        retry.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
        retry.pre_state = response.post_state.clone().unwrap();
        retry.message = borsh::to_vec(&Message::SettleWeighted { creator: id(50) }).unwrap();
        let repeated = handle(&retry).unwrap();
        assert_eq!(repeated.post_state, response.post_state);
        assert_eq!(repeated.validity.timestamps.start(), Some(3_000_000));
        assert!(repeated.sync_messages.is_empty());
        assert!(repeated.events.is_empty());
    }
}

#[test]
fn weighted_settlement_rejects_recorded_seed_shortfall_even_with_vault_surplus() {
    let (input, original) = funded();
    for recorded in [0, 1, 99] {
        for inventory in [0, original.remaining_inventory] {
            let mut state = original.clone();
            state.recorded_collateral = recorded;
            state.remaining_inventory = inventory;
            assert_eq!(
                settle(
                    &input,
                    &state,
                    Message::SettleWeighted { creator: id(50) },
                    config(100, id(43), false),
                    [1000, inventory + 7]
                )
                .err(),
                Some(lbp_core::Error::InvalidState)
            );
        }
    }
}

#[test]
fn weighted_completed_retries_still_check_factory_mode_and_recipient() {
    use lbp_core::SettlementProgress;
    let (mut input, mut state) = funded();
    for progress in [
        SettlementProgress::ManualComplete,
        SettlementProgress::Graduated,
    ] {
        state.settlement_progress = progress;
        state.recorded_collateral = 0;
        state.remaining_inventory = 0;
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.message = borsh::to_vec(&Message::SettleWeighted { creator: id(50) }).unwrap();
        for from in [
            None,
            Some(Actor::new(id(50), id(8))),
            Some(Actor::new(id(99), state.factory_program_id)),
            Some(Actor::new(state.factory_owner, id(99))),
        ] {
            input.from = from;
            assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
        }
        input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
        let retry = handle(&input).unwrap();
        assert_eq!(state_of(&retry), state);
        assert!(retry.sync_messages.is_empty());
        assert_eq!(retry.validity.timestamps.start(), Some(3_000_000));
        input.message = borsh::to_vec(&Message::SettleWeighted {
            creator: AccountId::default(),
        })
        .unwrap();
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
        input.message = borsh::to_vec(&Message::SettleFixedPrice {
            creator: id(50),
            now: 3000,
        })
        .unwrap();
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
    }
}

#[test]
fn receipt_settlement_keeps_selected_ending_and_net_collateral_for_both_routes() {
    let (input, original) = funded();
    for ending in [
        lbp_core::EndingPolicy::Scheduled,
        lbp_core::EndingPolicy::Either,
    ] {
        for remaining in [0, 1] {
            for route in [
                lbp_core::SettlementRoute::Manual,
                lbp_core::SettlementRoute::Graduation,
            ] {
                let mut state = original.clone();
                state.ending_policy = ending;
                state.remaining_inventory = remaining;
                state.recorded_collateral = 149;
                state.accrued_fees = 1;
                state.graduation_adapter = Some(Actor::new(id(70), id(71)));
                if let lbp_core::PoolMode::Weighted {
                    pricing_collateral, ..
                } = &mut state.mode
                {
                    *pricing_collateral = 149;
                }
                if route == lbp_core::SettlementRoute::Manual {
                    state.graduation_adapter = None;
                }
                let adapter = state.graduation_adapter;
                let message = Message::Settle {
                    route,
                    creator: id(50),
                    adapter,
                };
                let response = settle(
                    &input,
                    &state,
                    message.clone(),
                    config(100, id(43), false),
                    [150, remaining],
                )
                .unwrap();
                let expected = if ending == lbp_core::EndingPolicy::Either && remaining == 0 {
                    None
                } else {
                    Some(3_000_000)
                };
                assert_eq!(response.validity.timestamps.start(), expected);
                let lbp_core::SettlementReply::Applied { receipt } =
                    borsh::from_slice(&response.sync_messages.last().unwrap().message).unwrap()
                else {
                    panic!("expected applied receipt")
                };
                assert_eq!(receipt.eligible.collateral, 149);
                assert_eq!(
                    receipt.payouts.creator.collateral + receipt.payouts.adapter.collateral,
                    149
                );
                let mut finalize = input.clone();
                finalize.from = Some(receipt.factory);
                finalize.pre_state = response.post_state.unwrap();
                finalize.message = borsh::to_vec(&Message::FinalizeSettlement {
                    receipt_hash: receipt.hash(),
                })
                .unwrap();
                let completed = handle(&finalize).unwrap();
                let next = state_of(&completed);
                assert_eq!(next.remaining_inventory, remaining);
                assert_eq!(next.settlement_progress, receipt.progress);
                assert_eq!(completed.validity.timestamps.start(), expected);
                if let lbp_core::PoolMode::Weighted {
                    pricing_collateral, ..
                } = &mut state.mode
                {
                    *pricing_collateral = 150;
                }
                assert_eq!(
                    settle(
                        &input,
                        &state,
                        message,
                        config(100, id(43), false),
                        [150, remaining]
                    )
                    .err(),
                    Some(lbp_core::Error::InvalidState)
                );
            }
        }
    }
}
