use super::*;

#[test]
fn settlement_and_graduation_use_expiry_only_for_a_saved_open_pool() {
    for hook in [false, true] {
        for (lifecycle, expected) in [
            (pool::PoolLifecycle::Open, Some(400_000)),
            (pool::PoolLifecycle::Closed, None),
        ] {
            let (input, active) = active_for_route(hook);
            let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
            let mut saved = pool_state(&state, input.receiver, true);
            saved.pool.lifecycle = lifecycle;
            let (_, response) = settlement(&input, &active, saved, 100, 0, hook);
            assert_eq!(response.validity.timestamps.start(), expected);
            assert_eq!(response.validity.timestamps.end(), None);
        }
    }
}

#[test]
fn expiry_starts_manual_settlement_with_canonical_collateral_custody() {
    let (input, active) = active();
    let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    let (input, response) = settlement(
        &input,
        &active,
        pool_state(&state, input.receiver, true),
        100,
        0,
        false,
    );
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.settlement_stage, SettlementStage::Ready);
    assert!(
        matches!(&launch_events(&response)[..], [SaleEventV1 { kind: SaleEventKindV1::Settlement { step: SettlementStepV1::Prepared, route: None, effects }, .. }] if effects.is_empty())
    );
    assert_eq!(response.validity.timestamps.start(), Some(400_000));
    assert_eq!(response.validity.timestamps.end(), None);
    assert_eq!(state.pool_id, compute_pool_address(&state, input.receiver));
}

fn compute_pool_address(state: &FactoryState, actor: Actor) -> AccountId {
    curve_core::compute_pool_pda(
        state.namespace,
        state.curve_program_id,
        state.token_definition_id,
        state.collateral_definition_id,
        actor.account_id,
    )
}

fn withdrawn() -> (ReceiveInput, Response, curve_core::PoolAccount) {
    withdrawn_for_route(false, 0)
}

fn payout_ready(collateral: u128) -> (ReceiveInput, Response, curve_core::PoolAccount) {
    let (input, response, mut saved) = withdrawn_for_route(false, collateral);
    saved.pool.real_reserve0 = 0;
    saved.pool.real_reserve1 = 0;
    (input, response, saved)
}

#[test]
fn reserve_withdrawal_records_unsold_inventory_and_recorded_collateral() {
    let (input, response, _) = withdrawn();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.settlement_stage,
        SettlementStage::Withdrawn {
            unsold: 800,
            collateral_due: 0
        }
    );
    let call = response
        .sync_messages
        .iter()
        .find(|m| m.to == Actor::new(state.pool_id, state.curve_program_id))
        .unwrap();
    assert_eq!(
        borsh::from_slice::<curve_core::Message>(&call.message).unwrap(),
        curve_core::Message::WithdrawReserves { now: 400 }
    );
    assert_eq!(state.creator_allocation, 100);
    assert_eq!(
        input.receiver.account_id,
        compute_factory_pda(id(40), id(7), [3; 32])
    );
}

#[test]
fn manual_settlement_pays_both_assets_without_burning_or_paying_surplus() {
    let (input, withdrawn, saved) = payout_ready(50);
    let (mut input, complete) = settlement(&input, &withdrawn, saved, 817, 67, false);
    let state = FactoryState::decode(complete.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.settlement_stage, SettlementStage::Complete);
    assert!(!state.creator_allocation_claimed);
    assert_eq!(
        token_calls(&complete),
        vec![
            token::Message::Transfer {
                definition: state.token_definition_id,
                recipient: id(42),
                amount: 800
            },
            token::Message::Transfer {
                definition: state.collateral_definition_id,
                recipient: id(42),
                amount: 50
            },
        ]
    );
    input.pre_state = complete.post_state.unwrap();
    input.from = None;
    input.message = borsh::to_vec(&Message::WithdrawFactoryProceeds {
        creator: id(42),
        now: 400,
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(Error::AlreadySettled));
}

#[test]
fn traded_pool_settlement_checks_immutable_k_instead_of_initial_virtual_reserves() {
    let (input, active) = active();
    let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    let mut saved = pool_state(&state, input.receiver, true);
    saved
        .pool
        .swap_exact_input(pool::ExactInputSwap {
            token_in: pool::TokenSide::Token1,
            amount_in: 10,
            min_amount_out: 1,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 100,
        })
        .unwrap();
    assert_ne!(saved.pool.virtual_reserve0, 1000);
    let (_, ready) = settlement(&input, &active, saved.clone(), 100, 0, false);
    assert_eq!(
        FactoryState::decode(ready.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_stage,
        SettlementStage::Ready
    );
    let mut input = input.clone();
    input.pre_state = active.post_state.unwrap();
    input.message = borsh::to_vec(&Message::ClaimCreatorAllocation {
        creator: id(42),
        now: 400,
    })
    .unwrap();
    let response = handle(&input).unwrap();
    input = proof(&input, &response, id(42));
    let mut response = handle(&input).unwrap();
    for bytes in [
        borsh::to_vec(&saved).unwrap(),
        holding(state.creator_escrow_id, state.token_definition_id, 100),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    assert!(
        FactoryState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .creator_allocation_claimed
    );
}

#[test]
fn expiry_is_proved_separately_from_the_supplied_zero_lower_bound() {
    let (mut input, active) = active();
    let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    let saved = pool_state(&state, input.receiver, true);
    input.pre_state = active.post_state.unwrap();
    input.message = borsh::to_vec(&Message::WithdrawFactoryProceeds {
        creator: id(42),
        now: 0,
    })
    .unwrap();
    let response = handle(&input).unwrap();
    input = proof(&input, &response, id(42));
    let mut response = handle(&input).unwrap();
    for bytes in [
        borsh::to_vec(&saved).unwrap(),
        holding(input.receiver.account_id, state.token_definition_id, 100),
        holding(input.receiver.account_id, state.collateral_definition_id, 0),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    assert_eq!(response.validity.timestamps.start(), Some(400_000));
    assert_eq!(response.validity.timestamps.end(), None);
}

#[test]
fn selected_ending_controls_allocation_proceeds_and_hook_payouts() {
    for ending in [
        EndingPolicy::Scheduled,
        EndingPolicy::Depletion,
        EndingPolicy::Either,
    ] {
        for remaining in [0, 800] {
            for action in 0..3 {
                let (mut input, active) = active_for_route(action == 2);
                let mut state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
                state.ending_policy = ending;
                state.end_timestamp = if ending == EndingPolicy::Depletion {
                    None
                } else {
                    Some(400)
                };
                let mut saved = pool_state(&state, input.receiver, true);
                saved.pool.real_reserve0 = remaining;
                input.pre_state = borsh::to_vec(&state).unwrap().into();
                input.from = None;
                let command = match action {
                    0 => Message::ClaimCreatorAllocation {
                        creator: id(42),
                        now: 999,
                    },
                    1 => Message::WithdrawFactoryProceeds {
                        creator: id(42),
                        now: 999,
                    },
                    _ => Message::ExecuteGraduationHook {
                        creator_hint: Some(id(42)),
                    },
                };
                input.message = borsh::to_vec(&command).unwrap();
                let mut response = handle(&input).unwrap();
                input = if action == 2 {
                    resolved(&input, &response, id(42))
                } else {
                    proof(&input, &response, id(42))
                };
                response = handle(&input).unwrap();
                let mut values = vec![borsh::to_vec(&saved).unwrap()];
                if action == 0 {
                    values.push(holding(
                        state.creator_escrow_id,
                        state.token_definition_id,
                        100,
                    ));
                } else {
                    values.extend([
                        holding(input.receiver.account_id, state.token_definition_id, 100),
                        holding(input.receiver.account_id, state.collateral_definition_id, 0),
                    ]);
                }
                let mut result = Ok(response);
                for value in values {
                    input = reply(&input, result.as_ref().unwrap(), value);
                    result = handle(&input);
                }
                if ending == EndingPolicy::Depletion && remaining != 0 {
                    assert_eq!(result.err(), Some(Error::NotClosed));
                    continue;
                }
                let response = result.unwrap();
                let early = remaining == 0 && ending != EndingPolicy::Scheduled;
                assert_eq!(
                    response.validity.timestamps.start(),
                    if early { None } else { Some(400_000) }
                );
                assert_eq!(response.validity.timestamps.end(), None);
                assert_eq!(response.validity.timestamps.is_valid_for(399_999), early);
                assert!(response.validity.timestamps.is_valid_for(400_000));
                let after = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
                assert_eq!(after.creator_allocation, 100);
                assert_eq!(after.sale_reserve, 800);
            }
        }
    }
}

#[test]
fn curve_settlement_progress_and_wire_tags_match_the_payout_stages() {
    for (stage, progress, tag, payload) in [
        (
            SettlementStage::Unstarted,
            SettlementProgress::Unselected,
            0,
            vec![],
        ),
        (
            SettlementStage::Ready,
            SettlementProgress::Unselected,
            1,
            vec![],
        ),
        (
            SettlementStage::Withdrawn {
                unsold: 17,
                collateral_due: 29,
            },
            SettlementProgress::Unselected,
            2,
            vec![17_u128, 29],
        ),
        (
            SettlementStage::HookReady {
                unsold: 17,
                collateral_due: 29,
            },
            SettlementProgress::Unselected,
            3,
            vec![17, 29],
        ),
        (
            SettlementStage::Complete,
            SettlementProgress::ManualComplete,
            4,
            vec![],
        ),
        (
            SettlementStage::Graduated,
            SettlementProgress::Graduated,
            5,
            vec![],
        ),
    ] {
        assert_eq!(stage.progress(), progress);
        assert_eq!(
            stage.is_manual_settlement_complete(),
            progress.is_complete()
        );
        let mut expected = vec![tag];
        for amount in payload {
            expected.extend_from_slice(&amount.to_le_bytes());
        }
        assert_eq!(borsh::to_vec(&stage).unwrap(), expected);
        assert_eq!(
            borsh::from_slice::<SettlementStage>(&expected).unwrap(),
            stage
        );
    }
}

#[test]
fn curve_manual_self_recipient_reports_discharged_claim_without_token_movement() {
    let (mut input, withdrawn, pool) = payout_ready(0);
    let state = FactoryState::decode(withdrawn.post_state.as_ref().unwrap()).unwrap();
    input.pre_state = withdrawn.post_state.unwrap();
    input.from = None;
    input.message = borsh::to_vec(&Message::WithdrawFactoryProceeds {
        creator: input.receiver.account_id,
        now: 0,
    })
    .unwrap();
    let response = handle(&input).unwrap();
    input = proof(&input, &response, input.receiver.account_id);
    let mut response = handle(&input).unwrap();
    for bytes in [
        borsh::to_vec(&pool).unwrap(),
        holding(input.receiver.account_id, state.token_definition_id, 800),
        holding(input.receiver.account_id, state.collateral_definition_id, 0),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap();
    }
    let events = launch_events(&response);
    assert!(
        matches!(&events[..], [SaleEventV1 { kind: SaleEventKindV1::Settlement { effects, .. }, .. }, SaleEventV1 { kind: SaleEventKindV1::Settlement { step: SettlementStepV1::Completed, .. }, .. }] if matches!(&effects[..], [SettlementEffectV1::Payout { liability_discharged: 800, tokens_transferred: 0, .. }]))
    );
}

#[test]
fn retained_curve_invariant_binds_original_product_and_allows_checked_growth() {
    let (input, mut active) = active();
    let mut state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    state.sell_fee_bps = 125;
    state.trade_mode = TradeMode::BuyAndSell;
    active.post_state = Some(borsh::to_vec(&state).unwrap().into());
    let mut saved = pool_state(&state, input.receiver, true);
    for (side, amount) in [
        (pool::TokenSide::Token1, 100),
        (pool::TokenSide::Token0, 100),
    ] {
        saved
            .pool
            .swap_exact_input(pool::ExactInputSwap {
                token_in: side,
                amount_in: amount,
                min_amount_out: 1,
                pool_fee_bps: 0,
                protocol_fee_bps: 0,
                now: 100,
            })
            .unwrap();
    }
    assert!(saved.pool.k > saved.pool.initial_k);
    for hook in [false, true] {
        let mut routed = active.clone();
        let mut terms = state.clone();
        if hook {
            terms.graduation_hook = Some(GraduationHook {
                program_id: id(9),
                configuration: b"route".to_vec(),
            });
        }
        routed.post_state = Some(borsh::to_vec(&terms).unwrap().into());
        let (_, ready) = settlement(&input, &routed, saved.clone(), 100, 0, hook);
        assert_eq!(
            FactoryState::decode(ready.post_state.as_ref().unwrap())
                .unwrap()
                .settlement_stage,
            SettlementStage::Ready
        );
    }
    for case in 0..5 {
        let mut forged = saved.clone();
        match case {
            0 => forged.pool.sell_fee_bps += 1,
            1 => forged.pool.initial_k -= 1,
            2 => forged.pool.k = forged.pool.initial_k - 1,
            3 => forged.pool.virtual_reserve1 = 1,
            _ => {
                forged.pool.sell_fee_bps = 0;
            }
        }
        for command in [
            Message::ClaimCreatorAllocation {
                creator: id(42),
                now: 400,
            },
            Message::WithdrawFactoryProceeds {
                creator: id(42),
                now: 400,
            },
            Message::ExecuteGraduationHook {
                creator_hint: Some(id(42)),
            },
        ] {
            let allocation = matches!(command, Message::ClaimCreatorAllocation { .. });
            let hook = matches!(command, Message::ExecuteGraduationHook { .. });
            let mut request = input.clone();
            request.from = None;
            let mut terms = state.clone();
            if hook {
                terms.graduation_hook = Some(GraduationHook {
                    program_id: id(9),
                    configuration: b"route".to_vec(),
                });
            }
            request.pre_state = borsh::to_vec(&terms).unwrap().into();
            request.message = borsh::to_vec(&command).unwrap();
            let response = handle(&request).unwrap();
            request = if hook {
                resolved(&request, &response, id(42))
            } else {
                proof(&request, &response, id(42))
            };
            let mut response = handle(&request).unwrap();
            request = reply(&request, &response, borsh::to_vec(&forged).unwrap());
            if allocation {
                response = handle(&request).unwrap();
                request = reply(
                    &request,
                    &response,
                    holding(state.creator_escrow_id, state.token_definition_id, 100),
                );
            }
            if !allocation {
                response = handle(&request).unwrap();
                request = reply(
                    &request,
                    &response,
                    holding(input.receiver.account_id, state.token_definition_id, 100),
                );
                response = handle(&request).unwrap();
                request = reply(
                    &request,
                    &response,
                    holding(input.receiver.account_id, state.collateral_definition_id, 0),
                );
            }
            assert!(handle(&request).is_err(), "case {case}");
        }
    }
}

#[test]
fn curve_payouts_use_the_withdrawal_snapshot_for_no_buyers_partial_sale_and_depletion() {
    for hook in [false, true] {
        for scenario in 0..3 {
            let (input, active) = active_for_route(hook);
            let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
            let mut saved = pool_state(&state, input.receiver, true);
            if scenario != 0 {
                saved
                    .pool
                    .swap_exact_input(pool::ExactInputSwap {
                        token_in: pool::TokenSide::Token1,
                        amount_in: 100,
                        min_amount_out: 1,
                        pool_fee_bps: 0,
                        protocol_fee_bps: 100,
                        now: 100,
                    })
                    .unwrap();
                assert!(saved.pool.fees_accrued > 0);
            }
            if scenario == 2 {
                saved.pool.real_reserve0 = 0;
            }
            let unsold = saved.pool.real_reserve0;
            let collateral = saved.pool.real_reserve1;
            let fees = saved.pool.fees_accrued;
            let (input, ready) = settlement(&input, &active, saved.clone(), 17, 19, hook);
            let (input, withdrawn) = settlement(&input, &ready, saved.clone(), 17, 19, hook);
            assert_eq!(
                FactoryState::decode(withdrawn.post_state.as_ref().unwrap())
                    .unwrap()
                    .settlement_stage,
                SettlementStage::Withdrawn {
                    unsold,
                    collateral_due: collateral
                }
            );
            saved.pool.real_reserve0 = 0;
            saved.pool.real_reserve1 = 0;
            let (mut input, mut response) = settlement(
                &input,
                &withdrawn,
                saved.clone(),
                unsold + 17,
                collateral + fees + 19,
                hook,
            );
            if hook {
                input = reply(
                    &input,
                    &response,
                    adapter_state(input.receiver, response.post_state.as_ref().unwrap()),
                );
                response = handle(&input).unwrap();
                let (next, pending) = settlement(
                    &input,
                    &response,
                    saved,
                    unsold + 17,
                    collateral + fees + 19,
                    true,
                );
                input = reply(
                    &next,
                    &pending,
                    adapter_state(next.receiver, pending.post_state.as_ref().unwrap()),
                );
                response = handle(&input).unwrap();
            }
            let calls = token_calls(&response);
            for (definition, eligible) in [
                (state.token_definition_id, unsold),
                (state.collateral_definition_id, collateral),
            ] {
                let paid: u128 = calls
                    .iter()
                    .filter_map(|call| match call {
                        token::Message::Transfer {
                            definition: actual,
                            amount,
                            ..
                        } if *actual == definition => Some(*amount),
                        _ => None,
                    })
                    .sum();
                assert_eq!(paid, eligible, "hook {hook}, scenario {scenario}");
            }
            assert!(
                calls
                    .iter()
                    .all(|call| matches!(call, token::Message::Transfer { .. }))
            );
            assert!(
                !FactoryState::decode(response.post_state.as_ref().unwrap())
                    .unwrap()
                    .creator_allocation_claimed
            );
        }
    }
}
