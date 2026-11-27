use super::fixture::{
    config, exact_in, funded, holding, id, namespace_state, operation, prepare, reply, settle,
    state_of,
};
use lbp_core::{Message, handle};
use lee_core::account::{AccountId, Actor};

#[test]
fn sellout_settlement_collects_fees_and_pays_all_recorded_creator_proceeds() {
    let (input, mut state) = funded();
    state.remaining_inventory = 0;
    state.recorded_collateral = 1_200;
    state.accrued_fees = 13;
    let response = settle(&input, &state, 100, config(100, id(43), false), [1_230, 17]).unwrap();
    let next = state_of(&response);
    assert!(next.settlement_progress.is_complete());
    assert_eq!((next.recorded_collateral, next.accrued_fees), (0, 0));
    assert_eq!(response.sync_messages.len(), 2);
    for (call, recipient, amount) in [
        (&response.sync_messages[0], id(43), 13),
        (&response.sync_messages[1], id(60), 1_200),
    ] {
        assert_eq!(
            call.to,
            Actor::new(
                input.receiver.account_id,
                curve_core::TOKEN_PROGRAM_ACCOUNT_ID
            )
        );
        assert_eq!(
            borsh::from_slice::<lez_compat_core::token::Message>(&call.message).unwrap(),
            lez_compat_core::token::Message::Transfer {
                definition: id(42),
                recipient,
                amount,
            }
        );
        assert_eq!(
            call.pda_seeds,
            [lbp_core::compute_pool_pda_seed(
                state.namespace,
                id(41),
                id(42),
                state.factory_owner,
            )]
            .into_iter()
            .collect()
        );
    }
}

#[test]
fn expiry_settlement_requires_inclusion_at_the_inclusive_end() {
    let (input, state) = funded();
    let response = settle(&input, &state, 200, config(100, id(43), false), [0, 800]).unwrap();
    assert_eq!(response.validity.timestamps.start(), Some(200_000));
    assert_eq!(response.validity.timestamps.end(), None);
    assert!(state_of(&response).settlement_progress.is_complete());
    assert_eq!(state_of(&response).remaining_inventory, 800);
}

#[test]
fn expiry_pays_unsold_inventory_to_creator_and_excludes_donations() {
    let (input, mut state) = funded();
    state.remaining_inventory = 797;
    state.recorded_collateral = 4;
    state.accrued_fees = 1;
    let response = settle(&input, &state, 200, config(100, id(43), false), [22, 814]).unwrap();
    assert_eq!(response.sync_messages.len(), 3);
    let launch_payout = &response.sync_messages[1];
    assert_eq!(
        launch_payout.to,
        Actor::new(
            input.receiver.account_id,
            curve_core::TOKEN_PROGRAM_ACCOUNT_ID
        )
    );
    assert_eq!(
        borsh::from_slice::<lez_compat_core::token::Message>(&launch_payout.message).unwrap(),
        lez_compat_core::token::Message::Transfer {
            definition: id(41),
            recipient: id(60),
            amount: 797
        }
    );
    assert_eq!(
        launch_payout.pda_seeds,
        [lbp_core::compute_pool_pda_seed(
            state.namespace,
            id(41),
            id(42),
            state.factory_owner
        )]
        .into_iter()
        .collect()
    );
    assert_eq!(
        borsh::from_slice::<lez_compat_core::token::Message>(&response.sync_messages[2].message)
            .unwrap(),
        lez_compat_core::token::Message::Transfer {
            definition: id(42),
            recipient: id(60),
            amount: 4
        }
    );
    assert_eq!(state_of(&response).remaining_inventory, 797);
}

#[test]
fn settlement_retry_cannot_pay_again_even_with_empty_vaults() {
    let (input, mut state) = funded();
    state.remaining_inventory = 797;
    state.recorded_collateral = 4;
    state.accrued_fees = 1;
    let response = settle(&input, &state, 200, config(100, id(43), false), [5, 797]).unwrap();
    let settled = state_of(&response);
    let repeated = settle(&input, &settled, 0, config(100, id(70), true), [0, 0]).unwrap();
    assert_eq!(state_of(&repeated), settled);
    assert!(repeated.sync_messages.is_empty());
}

#[test]
fn settlement_rejects_vaults_that_cannot_cover_all_proceeds_fees_and_unsold_inventory() {
    let (input, mut state) = funded();
    state.remaining_inventory = 797;
    state.recorded_collateral = 4;
    state.accrued_fees = 1;
    for balances in [[4, 797], [5, 796]] {
        assert_eq!(
            settle(&input, &state, 200, config(100, id(43), false), balances).err(),
            Some(lbp_core::Error::Underfunded)
        );
    }
}

#[test]
fn settled_expiry_cannot_reopen_buys_with_a_backdated_timestamp_or_donated_tokens() {
    let (input, state) = funded();
    let response = settle(&input, &state, 200, config(100, id(43), false), [0, 800]).unwrap();
    let settled = state_of(&response);
    assert_eq!(
        operation(
            &input,
            &settled,
            exact_in(5),
            config(100, id(43), false),
            [0, 800, 100]
        )
        .err(),
        Some(lbp_core::Error::Closed)
    );
}

#[test]
fn settlement_is_factory_only_and_cannot_be_forwarded_through_buyer_execute() {
    let (mut input, mut state) = funded();
    state.remaining_inventory = 0;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    let command = Message::SettleFixedPrice {
        creator: id(60),
        now: 100,
    };
    input.message = borsh::to_vec(&command).unwrap();
    for from in [
        None,
        Some(Actor::new(id(60), input.receiver.program_account_id)),
        Some(Actor::new(id(99), state.factory_program_id)),
        Some(Actor::new(state.factory_owner, id(99))),
    ] {
        input.from = from;
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
    }
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.message = borsh::to_vec(&Message::SettleFixedPrice {
        creator: AccountId::default(),
        now: 100,
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
    input.pre_state = vec![].into();
    input.receiver.account_id = id(60);
    input.is_authorized = true;
    input.message = borsh::to_vec(&Message::Execute {
        pool: prepare().receiver.account_id,
        instruction: Box::new(command),
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
}

#[test]
fn prior_fee_collection_preserves_proceeds_and_leaves_collected_self_treasury_fees_in_custody() {
    let (mut input, mut state) = funded();
    state.remaining_inventory = 0;
    state.recorded_collateral = 1_200;
    state.accrued_fees = 13;
    input.from = None;
    for (treasury, balance_after_collection) in
        [(id(70), 1_217), (input.receiver.account_id, 1_230)]
    {
        let collected = operation(
            &input,
            &state,
            Message::CollectFees,
            config(100, treasury, true),
            [1_230, 0, 0],
        )
        .unwrap();
        let collected = state_of(&collected);
        assert_eq!(
            (collected.recorded_collateral, collected.accrued_fees),
            (1_200, 0)
        );
        let response = settle(
            &input,
            &collected,
            100,
            config(9_999, id(71), true),
            [balance_after_collection, 17],
        )
        .unwrap();
        assert_eq!(response.sync_messages.len(), 1);
        assert_eq!(
            borsh::from_slice::<lez_compat_core::token::Message>(
                &response.sync_messages[0].message
            )
            .unwrap(),
            lez_compat_core::token::Message::Transfer {
                definition: id(42),
                recipient: id(60),
                amount: 1_200
            }
        );
        let collected_after_close = operation(
            &input,
            &state_of(&response),
            Message::CollectFees,
            config(100, id(71), true),
            [17, 0, 0],
        )
        .unwrap();
        assert!(collected_after_close.sync_messages.is_empty());
    }
}

#[test]
fn settlement_uses_only_end_or_sellout_and_checks_time_conversion() {
    let (input, mut state) = funded();
    let response = settle(&input, &state, 199, config(100, id(43), false), [0, 800]).unwrap();
    assert_eq!(response.validity.timestamps.start(), Some(200_000));
    state.end_timestamp = None;
    state.ending_policy = lbp_core::EndingPolicy::Depletion;
    assert_eq!(
        settle(
            &input,
            &state,
            u64::MAX,
            config(100, id(43), false),
            [0, 800]
        )
        .err(),
        Some(lbp_core::Error::NotClosed)
    );
    state.end_timestamp = Some(u64::MAX);
    assert_eq!(
        settle(
            &input,
            &state,
            u64::MAX,
            config(100, id(43), false),
            [0, 800]
        )
        .err(),
        Some(lbp_core::Error::InvalidTerms)
    );
    state.remaining_inventory = 0;
    for end in [None, Some(200)] {
        state.end_timestamp = end;
        state.ending_policy = if end.is_some() {
            lbp_core::EndingPolicy::Either
        } else {
            lbp_core::EndingPolicy::Depletion
        };
        let response = settle(&input, &state, 100, config(100, id(43), false), [0, 0]).unwrap();
        assert!(state_of(&response).settlement_progress.is_complete());
        assert!(response.sync_messages.is_empty());
    }
}

#[test]
fn terminal_fees_use_the_current_treasury_without_repricing_proceeds_while_frozen() {
    let (input, mut state) = funded();
    state.remaining_inventory = 0;
    state.recorded_collateral = 1_200;
    state.accrued_fees = 13;
    for treasury in [id(70), input.receiver.account_id] {
        let response = settle(
            &input,
            &state,
            100,
            config(9_999, treasury, true),
            [1_230, 17],
        )
        .unwrap();
        let calls: Vec<lez_compat_core::token::Message> = response
            .sync_messages
            .iter()
            .map(|call| borsh::from_slice(&call.message).unwrap())
            .collect();
        let creator_payout = lez_compat_core::token::Message::Transfer {
            definition: id(42),
            recipient: id(60),
            amount: 1_200,
        };
        let expected = if treasury == input.receiver.account_id {
            vec![creator_payout]
        } else {
            vec![
                lez_compat_core::token::Message::Transfer {
                    definition: id(42),
                    recipient: id(70),
                    amount: 13,
                },
                creator_payout,
            ]
        };
        assert_eq!(calls, expected);
        assert_eq!(
            (
                state_of(&response).recorded_collateral,
                state_of(&response).accrued_fees
            ),
            (0, 0)
        );
    }
}

#[test]
fn settlement_callbacks_require_the_saved_namespace_and_canonical_vaults() {
    let (mut input, state) = funded();
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SettleFixedPrice {
        creator: id(60),
        now: 200,
    })
    .unwrap();
    let mut response = handle(&input).unwrap();
    for (step, bytes) in [
        config(100, id(43), false),
        holding(input.receiver.account_id, id(42), 0),
        holding(input.receiver.account_id, id(41), 800),
    ]
    .into_iter()
    .enumerate()
    {
        let source = response.sync_messages[0].to;
        input = reply(&input, &response, source, bytes);
        for from in [
            None,
            Some(Actor::new(id(99), source.program_account_id)),
            Some(Actor::new(source.account_id, id(99))),
        ] {
            let mut forged = input.clone();
            forged.from = from;
            assert_eq!(handle(&forged).err(), Some(lbp_core::Error::Unauthorized));
        }
        let mut wrong_pool = input.clone();
        wrong_pool.receiver.account_id = id(99);
        assert_eq!(
            handle(&wrong_pool).err(),
            Some(lbp_core::Error::WrongAccount)
        );
        let invalid = if step == 0 {
            let mut namespace = curve_core::Config::decode(&namespace_state()).unwrap();
            namespace.namespace = id(99);
            borsh::to_vec(&namespace).unwrap()
        } else {
            holding(id(99), id(99), 800)
        };
        assert!(handle(&reply(&input, &response, source, invalid)).is_err());
        response = handle(&input).unwrap();
    }
}

#[test]
fn fixed_inventory_payout_keeps_expiry_evidence_after_settlement() {
    let (input, state) = funded();
    let response = settle(&input, &state, 0, config(100, id(43), true), [0, 817]).unwrap();
    let next = state_of(&response);
    assert_eq!(next.remaining_inventory, 800);
    assert_eq!(next.close_time_bound().unwrap(), Some(200));
    assert_eq!(
        next.ending_policy
            .settlement(next.end_timestamp, next.remaining_inventory)
            .unwrap()
            .reason(),
        sale_policy::EndingReason::ScheduledEnd
    );
}

#[test]
fn fixed_selected_ending_separates_empty_inventory_from_scheduled_close() {
    let (input, original) = funded();
    for ending in [
        lbp_core::EndingPolicy::Scheduled,
        lbp_core::EndingPolicy::Depletion,
        lbp_core::EndingPolicy::Either,
    ] {
        let mut state = original.clone();
        state.ending_policy = ending;
        state.end_timestamp = if ending == lbp_core::EndingPolicy::Depletion {
            None
        } else {
            Some(200)
        };
        state.remaining_inventory = 0;
        let response = settle(&input, &state, 100, config(100, id(43), false), [0, 0]).unwrap();
        let expected = if ending == lbp_core::EndingPolicy::Scheduled {
            Some(200_000)
        } else {
            None
        };
        assert_eq!(response.validity.timestamps.start(), expected);
        assert_eq!(
            state_of(&response).close_time_bound().unwrap(),
            expected.map(|time| time / 1000)
        );
        assert_eq!(
            operation(
                &input,
                &state,
                exact_in(5),
                config(100, id(43), false),
                [0, 0, 100]
            )
            .err(),
            Some(lbp_core::Error::Closed)
        );
        state.remaining_inventory = 1;
        let close = state.close_time_bound();
        if ending == lbp_core::EndingPolicy::Depletion {
            assert_eq!(close.err(), Some(lbp_core::Error::NotClosed));
        } else {
            assert_eq!(close.unwrap(), Some(200));
        }
    }
}

#[test]
fn fixed_atomic_settlement_records_accounted_balances_once() {
    use lbp_core::SettlementProgress;
    use lez_compat_core::token::Message as TokenMessage;
    let (input, original) = funded();
    for (proceeds, fees, inventory) in [(0, 0, 800), (0, 1, 800), (4, 1, 797), (4, 0, 0), (0, 0, 0)]
    {
        let mut state = original.clone();
        state.recorded_collateral = proceeds;
        state.accrued_fees = fees;
        state.remaining_inventory = inventory;
        let response = settle(
            &input,
            &state,
            0,
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
        assert_eq!(
            response.validity.timestamps.start(),
            if inventory == 0 { None } else { Some(200_000) }
        );
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
                recipient: id(60),
                amount: inventory,
            });
        }
        if proceeds != 0 {
            calls.push(TokenMessage::Transfer {
                definition: id(42),
                recipient: id(60),
                amount: proceeds,
            });
        }
        assert_eq!(
            response
                .sync_messages
                .iter()
                .map(|call| borsh::from_slice::<TokenMessage>(&call.message).unwrap())
                .collect::<Vec<_>>(),
            calls
        );
        let retry = settle(&input, &expected, 0, config(0, id(70), false), [0, 0]).unwrap();
        assert_eq!(retry.post_state, response.post_state);
        assert!(retry.sync_messages.is_empty());
        assert!(retry.events.is_empty());
    }
}

#[test]
fn fixed_completed_retries_still_check_factory_mode_and_recipient() {
    use lbp_core::SettlementProgress;
    let (mut input, mut state) = funded();
    for progress in [
        SettlementProgress::ManualComplete,
        SettlementProgress::Graduated,
    ] {
        state.settlement_progress = progress;
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.message = borsh::to_vec(&Message::SettleFixedPrice {
            creator: id(60),
            now: 0,
        })
        .unwrap();
        for from in [
            None,
            Some(Actor::new(id(60), id(8))),
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
        input.message = borsh::to_vec(&Message::SettleFixedPrice {
            creator: AccountId::default(),
            now: 0,
        })
        .unwrap();
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
        input.message = borsh::to_vec(&Message::SettleWeighted { creator: id(60) }).unwrap();
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
    }
}

#[test]
fn receipt_settlement_requires_saved_namespace_and_canonical_vault_replies() {
    let (mut input, mut state) = funded();
    state.graduation_adapter = Some(Actor::new(id(70), id(71)));
    state.recorded_collateral = 49;
    state.accrued_fees = 2;
    input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::Settle {
        route: lbp_core::SettlementRoute::Graduation,
        creator: id(60),
        adapter: state.graduation_adapter,
    })
    .unwrap();
    let mut response = handle(&input).unwrap();
    for (step, bytes) in [
        config(100, id(43), false),
        holding(input.receiver.account_id, id(42), 1000),
        holding(input.receiver.account_id, id(41), 900),
    ]
    .into_iter()
    .enumerate()
    {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        for from in [
            None,
            Some(Actor::new(id(99), source.program_account_id)),
            Some(Actor::new(source.account_id, id(99))),
        ] {
            let mut wrong = input.clone();
            wrong.from = from;
            assert_eq!(handle(&wrong).err(), Some(lbp_core::Error::Unauthorized));
        }
        if step > 0 {
            for (owner, definition) in [
                (id(99), if step == 1 { id(42) } else { id(41) }),
                (input.receiver.account_id, id(99)),
            ] {
                let wrong = reply(&input, &response, source, holding(owner, definition, 1000));
                assert_eq!(
                    handle(&wrong).err(),
                    Some(lbp_core::Error::InvalidDefinition)
                );
            }
        }
        response = handle(&input).unwrap();
    }
    assert!(lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).is_err());
    let lbp_core::SettlementReply::Applied { receipt } =
        borsh::from_slice(&response.sync_messages.last().unwrap().message).unwrap()
    else {
        panic!("expected receipt")
    };
    assert_eq!(receipt.eligible.collateral, 49);
    assert_eq!(receipt.pool, input.receiver);
    assert_eq!(receipt.adapter, state.graduation_adapter);
}
