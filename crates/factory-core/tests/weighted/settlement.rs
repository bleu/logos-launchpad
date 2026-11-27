use super::*;

#[test]
fn fixed_price_payout_commands_cannot_release_weighted_sale_funds() {
    let (mut input, response, _) = minted(CreatorAllocationRelease::AfterEnd);
    let mut state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = LbpCreationStage::Complete;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    for message in [
        Message::WithdrawFactoryProceeds {
            creator: id(42),
            now: 300,
        },
        Message::ClaimCreatorAllocation {
            creator: id(42),
            now: 300,
        },
    ] {
        input.message = borsh::to_vec(&message).unwrap();
        assert_eq!(handle(&input).err(), Some(Error::InvalidState));
    }
}

fn weighted_callback<T: borsh::BorshSerialize>(
    input: &ReceiveInput,
    response: &Response,
    message: &T,
) -> ReceiveInput {
    ReceiveInput {
        receiver: input.receiver,
        from: Some(response.sync_messages.last().unwrap().to),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(message).unwrap(),
    }
}

#[test]
fn weighted_hook_binds_both_assets_and_includes_seed_in_the_split() {
    use factory_core::{
        AdapterMessage, GraduationIntent, GraduationIntentKind, GraduationResult,
        SettlementProgress, SettlementReceipt, SettlementReply, SettlementRoute,
    };
    let mut terms = inputs();
    terms.graduation_hook = Some(factory_core::GraduationHook {
        program_id: id(9),
        configuration: b"weighted".to_vec(),
    });
    terms.graduation_policy = factory_core::GraduationPolicy {
        launch_bps: 2500,
        collateral_bps: 6000,
    };
    let (mut input, prepared, bytes, setup) = prepared_with_terms(terms.clone());
    let mut state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.graduation_hook, terms.graduation_hook);
    let mut pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    let adapter = Actor::new(
        factory_core::compute_graduation_pda(id(9), input.receiver.account_id),
        id(9),
    );
    assert_eq!(pool.graduation_adapter, Some(adapter));
    let mut changed = pool.clone();
    changed.graduation_adapter = None;
    assert_eq!(
        advance(
            &setup,
            &prepared,
            vec![
                borsh::to_vec(&changed).unwrap(),
                holding(state.pool_id, state.token_definition_id, 0),
                holding(state.pool_id, state.collateral_definition_id, 0),
                holding(id(42), state.collateral_definition_id, 100)
            ]
        )
        .err(),
        Some(Error::InvalidCustody)
    );
    state.creation_stage = LbpCreationStage::Complete;
    pool.funded = true;
    pool.remaining_inventory = 790;
    pool.recorded_collateral = 125;
    pool.accrued_fees = 3;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.from = None;
    input.message = borsh::to_vec(&Message::ExecuteGraduationHook {
        creator_hint: Some(id(99)),
    })
    .unwrap();
    let response = handle(&input).unwrap();
    input = reply(&input, &response, borsh::to_vec(&pool).unwrap());
    let response = handle(&input).unwrap();
    let initialized = factory_core::handle_adapter(&ReceiveInput {
        receiver: adapter,
        from: None,
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&AdapterMessage::Initialize {
            factory_program_id: id(7),
            namespace: state.namespace,
            launch_salt: state.launch_salt,
            accepted_intent: GraduationIntentKind::Weighted,
        })
        .unwrap(),
    })
    .unwrap();
    let adapter_state = factory_core::handle_adapter(&ReceiveInput {
        receiver: adapter,
        from: Some(input.receiver),
        is_authorized: false,
        pre_state: initialized.post_state.unwrap(),
        message: borsh::to_vec(&lee_core::program::StateReply {
            state: borsh::to_vec(&state).unwrap().into(),
        })
        .unwrap(),
    })
    .unwrap()
    .post_state
    .unwrap();
    input = reply(&input, &response, adapter_state.to_vec());
    let mut response = handle(&input).unwrap();
    assert_eq!(response.validity.timestamps.start(), Some(300_000));
    assert!(!response.validity.timestamps.is_valid_for(299_999));
    for definition in [state.token_definition_id, state.collateral_definition_id] {
        input = reply(
            &input,
            &response,
            holding(adapter.account_id, definition, 0),
        );
        response = handle(&input).unwrap();
    }
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(
            state.creator_role_id,
            factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID
        )
    );
    input = weighted_callback(
        &input,
        &response,
        &lez_compat_core::authority::CurrentAuthority {
            role: state.creator_role_id,
            authority: id(99),
        },
    );
    let pending = handle(&input).unwrap();
    assert_eq!(pending.validity.timestamps.start(), Some(300_000));
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&pending.sync_messages.last().unwrap().message)
            .unwrap(),
        lbp_core::Message::Settle {
            route: SettlementRoute::Graduation,
            creator: id(99),
            adapter: Some(adapter)
        }
    );
    let receipt = SettlementReceipt {
        factory: input.receiver,
        pool: Actor::new(state.pool_id, state.lbp_program_id),
        route: SettlementRoute::Graduation,
        creator: id(99),
        adapter: Some(adapter),
        eligible: factory_core::SettlementAmounts {
            launch_tokens: pool.remaining_inventory,
            collateral: pool.recorded_collateral,
        },
        payouts: state
            .graduation_policy
            .payouts(
                factory_core::SettlementAmounts {
                    launch_tokens: pool.remaining_inventory,
                    collateral: pool.recorded_collateral,
                },
                factory_core::SettlementRoute::Graduation,
                state.graduation_hook.is_some(),
            )
            .unwrap(),
        progress: SettlementProgress::Graduated,
    };
    for case in 0..3 {
        let mut changed = receipt.clone();
        match case {
            0 => changed.payouts.creator.collateral += 1,
            1 => changed.eligible.collateral += 1,
            _ => changed.eligible.launch_tokens = 1,
        }
        let bad = weighted_callback(
            &input,
            &pending,
            &SettlementReply::Applied { receipt: changed },
        );
        assert_eq!(handle(&bad).err(), Some(Error::InvalidState));
    }
    input = weighted_callback(
        &input,
        &pending,
        &SettlementReply::Applied {
            receipt: receipt.clone(),
        },
    );
    let callback = handle(&input).unwrap();
    assert!(pending.events.is_empty());
    assert!(callback.events.is_empty());
    assert!(callback.sync_messages[0].pda_seeds.is_empty());
    let AdapterMessage::Graduate { request } =
        borsh::from_slice(&callback.sync_messages[0].message).unwrap()
    else {
        panic!("graduate");
    };
    assert_eq!(
        request.intent,
        GraduationIntent::Weighted {
            launch_tokens: 197,
            collateral: 75
        }
    );
    assert_eq!(
        LbpLaunchState::decode(callback.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_progress,
        SettlementProgress::Unselected
    );
    input = weighted_callback(
        &input,
        &callback,
        &GraduationResult {
            factory: input.receiver.account_id,
            request_hash: request.hash(),
        },
    );
    let finalize = handle(&input).unwrap();
    assert!(finalize.events.is_empty());
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&finalize.sync_messages[0].message).unwrap(),
        lbp_core::Message::FinalizeSettlement {
            receipt_hash: receipt.hash()
        }
    );
    input = weighted_callback(
        &input,
        &finalize,
        &SettlementReply::Completed {
            receipt_hash: receipt.hash(),
            progress: receipt.progress,
        },
    );
    let completed = handle(&input).unwrap();
    assert_eq!(
        launch_events(&completed),
        vec![SaleEventV1::new(
            *state.namespace.value(),
            PricingEngineV1::Weighted,
            EventSubjectV1::Launch {
                factory: *input.receiver.account_id.value(),
                pool: *state.pool_id.value()
            },
            SaleEventKindV1::Graduated {
                adapter_program: *adapter.program_account_id.value(),
                adapter_custody: *adapter.account_id.value(),
                launch_tokens: 197,
                collateral: 75
            }
        )]
    );
    assert_eq!(
        LbpLaunchState::decode(completed.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_progress,
        SettlementProgress::Graduated
    );
}

#[test]
fn weighted_no_buyer_manual_completion_pays_inventory_and_seed_once() {
    use factory_core::{SettlementProgress, SettlementReceipt, SettlementReply, SettlementRoute};
    let (mut input, prepared, bytes, _) = prepared(CreatorAllocationRelease::AfterEnd);
    let mut state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = LbpCreationStage::Complete;
    let mut pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    pool.funded = true;
    pool.recorded_collateral = 100;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.from = None;
    input.message = borsh::to_vec(&Message::WithdrawWeightedProceeds { creator: id(42) }).unwrap();
    let response = handle(&input).unwrap();
    input = weighted_authorized(&input, &response);
    let response = handle(&input).unwrap();
    input = reply(&input, &response, borsh::to_vec(&pool).unwrap());
    let pending = handle(&input).unwrap();
    assert_eq!(pending.validity.timestamps.start(), Some(300_000));
    let receipt = SettlementReceipt {
        factory: input.receiver,
        pool: Actor::new(state.pool_id, state.lbp_program_id),
        route: SettlementRoute::Manual,
        creator: id(42),
        adapter: None,
        eligible: factory_core::SettlementAmounts {
            launch_tokens: pool.remaining_inventory,
            collateral: pool.recorded_collateral,
        },
        payouts: state
            .graduation_policy
            .payouts(
                factory_core::SettlementAmounts {
                    launch_tokens: pool.remaining_inventory,
                    collateral: pool.recorded_collateral,
                },
                factory_core::SettlementRoute::Manual,
                state.graduation_hook.is_some(),
            )
            .unwrap(),
        progress: SettlementProgress::ManualComplete,
    };
    input = weighted_callback(
        &input,
        &pending,
        &SettlementReply::Applied {
            receipt: receipt.clone(),
        },
    );
    let finalize = handle(&input).unwrap();
    assert!(finalize.events.is_empty());
    assert_eq!(
        LbpLaunchState::decode(finalize.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_progress,
        SettlementProgress::Unselected
    );
    input = weighted_callback(
        &input,
        &finalize,
        &SettlementReply::Completed {
            receipt_hash: receipt.hash(),
            progress: receipt.progress,
        },
    );
    let completed = handle(&input).unwrap();
    assert!(completed.events.is_empty());
    let state = LbpLaunchState::decode(completed.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.settlement_progress,
        SettlementProgress::ManualComplete
    );
    pool.settlement_progress = state.settlement_progress;
    pool.recorded_collateral = 0;
    input.pre_state = completed.post_state.unwrap();
    input.from = None;
    input.message = borsh::to_vec(&Message::WithdrawWeightedProceeds { creator: id(42) }).unwrap();
    let response = handle(&input).unwrap();
    input = weighted_authorized(&input, &response);
    let response = handle(&input).unwrap();
    input = reply(&input, &response, borsh::to_vec(&pool).unwrap());
    let noop = handle(&input).unwrap();
    assert!(noop.events.is_empty());
    assert!(noop.sync_messages.is_empty());
    assert_eq!(noop.validity.timestamps.start(), Some(300_000));
}

#[test]
fn weighted_payouts_and_hook_reject_a_substituted_trade_mode() {
    let mut terms = inputs();
    terms.graduation_hook = Some(factory_core::GraduationHook {
        program_id: id(9),
        configuration: vec![],
    });
    let (input, prepared, bytes, _) = prepared_with_terms(terms);
    let mut state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = LbpCreationStage::Complete;
    let mut pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    pool.funded = true;
    pool.trade_mode = factory_core::TradeMode::BuyAndSell;
    for command in [
        Message::ClaimWeightedAllocation { creator: id(42) },
        Message::WithdrawWeightedProceeds { creator: id(42) },
        Message::ExecuteGraduationHook {
            creator_hint: Some(id(42)),
        },
    ] {
        let hook = matches!(command, Message::ExecuteGraduationHook { .. });
        let allocation = matches!(command, Message::ClaimWeightedAllocation { .. });
        let mut request = input.clone();
        request.from = None;
        request.pre_state = borsh::to_vec(&state).unwrap().into();
        request.message = borsh::to_vec(&command).unwrap();
        if matches!(command, Message::WithdrawWeightedProceeds { .. }) {
            assert_eq!(handle(&request).err(), Some(Error::InvalidTerms));
            continue;
        }
        let mut response = handle(&request).unwrap();
        if !hook {
            request = weighted_authorized(&request, &response);
            response = handle(&request).unwrap();
        }
        request = reply(&request, &response, borsh::to_vec(&pool).unwrap());
        if allocation {
            response = handle(&request).unwrap();
            request = reply(
                &request,
                &response,
                holding(state.creator_escrow_id, state.token_definition_id, 200),
            );
        }
        assert_eq!(handle(&request).err(), Some(Error::InvalidCustody));
    }
}

#[test]
fn weighted_payouts_and_hook_reject_a_substituted_sell_fee() {
    let mut terms = inputs();
    terms.graduation_hook = Some(factory_core::GraduationHook {
        program_id: id(9),
        configuration: vec![],
    });
    let (input, prepared, bytes, _) = prepared_with_terms(terms);
    let mut state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = LbpCreationStage::Complete;
    let mut pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    pool.funded = true;
    pool.sell_fee_bps = 1;
    for command in [
        Message::ClaimWeightedAllocation { creator: id(42) },
        Message::WithdrawWeightedProceeds { creator: id(42) },
        Message::ExecuteGraduationHook {
            creator_hint: Some(id(42)),
        },
    ] {
        let hook = matches!(command, Message::ExecuteGraduationHook { .. });
        let allocation = matches!(command, Message::ClaimWeightedAllocation { .. });
        let mut request = input.clone();
        request.from = None;
        request.pre_state = borsh::to_vec(&state).unwrap().into();
        request.message = borsh::to_vec(&command).unwrap();
        if matches!(command, Message::WithdrawWeightedProceeds { .. }) {
            assert_eq!(handle(&request).err(), Some(Error::InvalidTerms));
            continue;
        }
        let mut response = handle(&request).unwrap();
        if !hook {
            request = weighted_authorized(&request, &response);
            response = handle(&request).unwrap();
        }
        request = reply(&request, &response, borsh::to_vec(&pool).unwrap());
        if allocation {
            response = handle(&request).unwrap();
            request = reply(
                &request,
                &response,
                holding(state.creator_escrow_id, state.token_definition_id, 200),
            );
        }
        assert_eq!(handle(&request).err(), Some(Error::InvalidCustody));
    }
}

#[test]
fn weighted_graduation_policy_validates_creation_and_forbids_manual_even_after_completion() {
    use factory_core::{GraduationHook, GraduationPolicy, SettlementProgress};
    for hook in [
        None,
        Some(GraduationHook {
            program_id: id(9),
            configuration: vec![],
        }),
    ] {
        for policy in [
            GraduationPolicy::default(),
            GraduationPolicy {
                launch_bps: 10_000,
                collateral_bps: 0,
            },
            GraduationPolicy {
                launch_bps: 0,
                collateral_bps: 1,
            },
            GraduationPolicy {
                launch_bps: 10_001,
                collateral_bps: 0,
            },
            GraduationPolicy {
                launch_bps: 0,
                collateral_bps: 10_001,
            },
        ] {
            let mut terms = inputs();
            terms.graduation_hook = hook.clone();
            terms.graduation_policy = policy;
            let result = handle(&creation(terms));
            if policy.validate(hook.is_some()).is_err() {
                assert_eq!(result.err(), Some(Error::InvalidTerms));
            } else {
                assert_eq!(
                    LbpLaunchState::decode(result.unwrap().post_state.as_ref().unwrap())
                        .unwrap()
                        .graduation_policy,
                    policy
                );
            }
        }
    }
    let (mut input, prepared, _, _) = prepared(CreatorAllocationRelease::AfterEnd);
    let mut state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = LbpCreationStage::Complete;
    state.graduation_hook = Some(GraduationHook {
        program_id: id(9),
        configuration: vec![],
    });
    for progress in [
        SettlementProgress::Unselected,
        SettlementProgress::ManualComplete,
        SettlementProgress::Graduated,
    ] {
        state.settlement_progress = progress;
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.message =
            borsh::to_vec(&Message::WithdrawWeightedProceeds { creator: id(42) }).unwrap();
        assert_eq!(handle(&input).err(), Some(Error::InvalidTerms));
    }
}
