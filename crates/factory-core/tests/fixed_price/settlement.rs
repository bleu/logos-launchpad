use super::*;

fn settlement_callback<T: borsh::BorshSerialize>(
    input: &ReceiveInput,
    response: &lee_core::program::Response,
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

fn fixed_hook_resolve(
    progress: factory_core::SettlementProgress,
    proceeds: u128,
) -> (
    ReceiveInput,
    lee_core::program::Response,
    factory_core::LbpLaunchState,
    lbp_core::PoolAccount,
) {
    use factory_core::{AdapterMessage, GraduationIntentKind};
    let (mut input, mut state) = completed();
    state.graduation_hook = Some(factory_core::GraduationHook {
        program_id: id(9),
        configuration: b"fixed".to_vec(),
    });
    state.graduation_policy = factory_core::GraduationPolicy {
        launch_bps: 2500,
        collateral_bps: 6000,
    };
    state.settlement_progress = progress;
    let mut pool = active_pool(&state, input.receiver);
    pool.recorded_collateral = proceeds;
    pool.settlement_progress = progress;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.from = None;
    input.message = borsh::to_vec(&Message::ExecuteGraduationHook {
        creator_hint: Some(id(99)),
    })
    .unwrap();
    let response = handle(&input).unwrap();
    input = reply(&input, &response, borsh::to_vec(&pool).unwrap());
    let response = handle(&input).unwrap();
    let adapter = Actor::new(
        factory_core::compute_graduation_pda(id(9), input.receiver.account_id),
        id(9),
    );
    let initialized = factory_core::handle_adapter(&ReceiveInput {
        receiver: adapter,
        from: None,
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&AdapterMessage::Initialize {
            factory_program_id: id(7),
            namespace: state.namespace,
            launch_salt: state.launch_salt,
            accepted_intent: GraduationIntentKind::FixedPrice,
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
    for accepted_intent in [GraduationIntentKind::Curve, GraduationIntentKind::Weighted] {
        let mut changed = factory_core::AdapterState::decode(&adapter_state).unwrap();
        changed.accepted_intent = accepted_intent;
        let mut rejected = input.clone();
        rejected.message = borsh::to_vec(&lee_core::program::StateReply {
            state: borsh::to_vec(&changed).unwrap().into(),
        })
        .unwrap();
        assert_eq!(handle(&rejected).err(), Some(Error::AdapterIntentMismatch));
    }
    let mut response = handle(&input).unwrap();
    assert_eq!(response.sync_messages.len(), 3);
    for definition in [state.token_definition_id, state.collateral_definition_id] {
        input = reply(
            &input,
            &response,
            holding(adapter.account_id, definition, 0),
        );
        let mut forged = input.clone();
        forged.from = Some(Actor::new(forged.from.unwrap().account_id, id(99)));
        assert_eq!(handle(&forged).err(), Some(Error::Unauthorized));
        forged = input.clone();
        forged.message = borsh::to_vec(&lee_core::program::StateReply {
            state: holding(id(99), definition, 0).into(),
        })
        .unwrap();
        assert_eq!(handle(&forged).err(), Some(Error::InvalidCustody));
        response = handle(&input).unwrap();
    }
    assert_eq!(
        borsh::from_slice::<lez_compat_core::authority::Message>(
            &response.sync_messages[0].message
        )
        .unwrap(),
        lez_compat_core::authority::Message::Resolve
    );
    (input, response, state, pool)
}

#[test]
fn fixed_price_proceeds_require_creator_role_before_forwarding_to_the_bound_pool() {
    let (mut input, state) = completed();
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.from = None;
    input.message = borsh::to_vec(&Message::WithdrawFactoryProceeds {
        creator: id(42),
        now: 100,
    })
    .unwrap();
    let response = handle(&input).unwrap();
    assert_eq!(response.sync_messages.len(), 1);
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(id(42), factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID)
    );
    assert_eq!(
        borsh::from_slice::<lez_compat_core::authority::Message>(
            &response.sync_messages[0].message
        )
        .unwrap(),
        lez_compat_core::authority::Message::Check {
            role: state.creator_role_id,
            callback: input.receiver,
        }
    );
    input = authorized(&input, &response, id(42));
    let response = handle(&input).unwrap();
    assert_eq!(response.sync_messages.len(), 1);
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&response.sync_messages[0].message).unwrap(),
        lbp_core::Message::ReadState
    );
    let pool = active_pool(&state, input.receiver);
    input = reply(&input, &response, borsh::to_vec(&pool).unwrap());
    let response = handle(&input).unwrap();
    let call = response.sync_messages.last().unwrap();
    assert_eq!(call.to, Actor::new(state.pool_id, state.lbp_program_id));
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&call.message).unwrap(),
        lbp_core::Message::Settle {
            route: factory_core::SettlementRoute::Manual,
            creator: id(42),
            adapter: None
        }
    );
    assert_eq!(
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_progress,
        state.settlement_progress
    );
}

fn manual_pending(
    pool_proceeds: u128,
) -> (
    ReceiveInput,
    lee_core::program::Response,
    factory_core::SettlementReceipt,
) {
    use factory_core::{SettlementProgress, SettlementReceipt, SettlementRoute};
    let (mut input, state) = completed();
    let mut pool = active_pool(&state, input.receiver);
    pool.recorded_collateral = pool_proceeds;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.from = None;
    input.message = borsh::to_vec(&Message::WithdrawFactoryProceeds {
        creator: id(42),
        now: 0,
    })
    .unwrap();
    let response = handle(&input).unwrap();
    input = authorized(&input, &response, id(42));
    let response = handle(&input).unwrap();
    input = reply(&input, &response, borsh::to_vec(&pool).unwrap());
    let response = handle(&input).unwrap();
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
    (input, response, receipt)
}

#[test]
fn manual_progress_changes_only_after_exact_pool_completion() {
    use factory_core::{SettlementProgress, SettlementReply};
    let (input, pending, receipt) = manual_pending(4);
    let applied = settlement_callback(
        &input,
        &pending,
        &SettlementReply::Applied {
            receipt: receipt.clone(),
        },
    );
    let finalize = handle(&applied).unwrap();
    assert!(pending.events.is_empty());
    assert!(finalize.events.is_empty());
    assert_eq!(
        factory_core::LbpLaunchState::decode(finalize.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_progress,
        SettlementProgress::Unselected
    );
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&finalize.sync_messages[0].message).unwrap(),
        lbp_core::Message::FinalizeSettlement {
            receipt_hash: receipt.hash()
        }
    );
    for (hash, progress) in [
        ([0; 32], receipt.progress),
        (receipt.hash(), SettlementProgress::Graduated),
    ] {
        let bad = settlement_callback(
            &input,
            &finalize,
            &SettlementReply::Completed {
                receipt_hash: hash,
                progress,
            },
        );
        assert_eq!(handle(&bad).err(), Some(Error::InvalidState));
    }
    let mut complete = settlement_callback(
        &input,
        &finalize,
        &SettlementReply::Completed {
            receipt_hash: receipt.hash(),
            progress: receipt.progress,
        },
    );
    complete.from = Some(Actor::new(receipt.pool.account_id, id(99)));
    assert_eq!(handle(&complete).err(), Some(Error::Unauthorized));
    complete.from = Some(receipt.pool);
    let response = handle(&complete).unwrap();
    assert!(response.sync_messages.is_empty());
    assert!(response.events.is_empty());
    let state =
        factory_core::LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.settlement_progress,
        SettlementProgress::ManualComplete
    );
    let mut retry = complete.clone();
    retry.pre_state = response.post_state.unwrap();
    retry.from = None;
    retry.message = borsh::to_vec(&Message::WithdrawFactoryProceeds {
        creator: id(42),
        now: 0,
    })
    .unwrap();
    let auth = handle(&retry).unwrap();
    assert_eq!(
        auth.sync_messages[0].to,
        Actor::new(id(42), factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID)
    );
    retry = authorized(&retry, &auth, id(42));
    let read = handle(&retry).unwrap();
    let mut pool = active_pool(&state, retry.receiver);
    pool.recorded_collateral = 0;
    pool.accrued_fees = 0;
    pool.settlement_progress = state.settlement_progress;
    retry = reply(&retry, &read, borsh::to_vec(&pool).unwrap());
    let noop = handle(&retry).unwrap();
    assert!(noop.events.is_empty());
    assert!(noop.sync_messages.is_empty());
    assert_eq!(noop.validity.timestamps.start(), Some(200_000));
}

#[test]
fn manual_applied_rejects_every_changed_receipt_binding_and_reentry() {
    use factory_core::{SettlementProgress, SettlementReply, SettlementRoute};
    let (input, response, receipt) = manual_pending(4);
    for case in 0..10 {
        let mut changed = receipt.clone();
        match case {
            0 => changed.factory.program_account_id = id(99),
            1 => changed.pool.account_id = id(99),
            2 => changed.route = SettlementRoute::Graduation,
            3 => changed.creator = id(99),
            4 => changed.adapter = Some(Actor::new(id(99), id(9))),
            5 => changed.eligible.launch_tokens = 1,
            6 => changed.eligible.collateral += 1,
            7 => changed.payouts.creator.launch_tokens += 1,
            8 => changed.progress = SettlementProgress::Graduated,
            _ => changed.pool.program_account_id = id(99),
        }
        let next = settlement_callback(
            &input,
            &response,
            &SettlementReply::Applied { receipt: changed },
        );
        assert_eq!(
            handle(&next).err(),
            Some(Error::InvalidState),
            "case {case}"
        );
    }
    let mut next = settlement_callback(&input, &response, &SettlementReply::Applied { receipt });
    next.from = Some(Actor::new(next.from.unwrap().account_id, id(99)));
    assert_eq!(handle(&next).err(), Some(Error::Unauthorized));
    next.from = Some(response.sync_messages.last().unwrap().to);
    next.message = borsh::to_vec(&Message::ReadState).unwrap();
    assert_eq!(handle(&next).err(), Some(Error::InvalidMessage));
}

#[test]
fn fixed_hook_resolves_current_creator_then_guards_both_completion_hashes() {
    use factory_core::{
        AdapterMessage, GraduationIntent, GraduationResult, SettlementProgress, SettlementReply,
        SettlementRoute,
    };
    let (input, resolve, state, pool) = fixed_hook_resolve(SettlementProgress::Unselected, 4);
    let mut resolved = settlement_callback(
        &input,
        &resolve,
        &lez_compat_core::authority::CurrentAuthority {
            role: state.creator_role_id,
            authority: id(42),
        },
    );
    assert_eq!(handle(&resolved).err(), Some(Error::Unauthorized));
    resolved.message = borsh::to_vec(&lez_compat_core::authority::CurrentAuthority {
        role: state.creator_role_id,
        authority: id(99),
    })
    .unwrap();
    resolved.from = Some(Actor::new(state.creator_role_id, id(99)));
    assert_eq!(handle(&resolved).err(), Some(Error::Unauthorized));
    resolved.from = Some(Actor::new(
        state.creator_role_id,
        factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
    ));
    let pending = handle(&resolved).unwrap();
    assert_eq!(pending.sync_messages.len(), 3);
    assert_eq!(
        borsh::from_slice::<lez_compat_core::token::Message>(&pending.sync_messages[0].message)
            .unwrap(),
        lez_compat_core::token::Message::Initialize { owner: id(99) }
    );
    assert!(
        pending
            .sync_messages
            .iter()
            .all(|call| call.pda_seeds.is_empty())
    );
    let receipt = factory_core::SettlementReceipt {
        factory: input.receiver,
        pool: Actor::new(state.pool_id, state.lbp_program_id),
        route: SettlementRoute::Graduation,
        creator: id(99),
        adapter: pool.graduation_adapter,
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
    let applied = settlement_callback(
        &input,
        &pending,
        &SettlementReply::Applied {
            receipt: receipt.clone(),
        },
    );
    let callback = handle(&applied).unwrap();
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
        GraduationIntent::FixedPrice {
            launch_tokens: 199,
            collateral: 2
        }
    );
    let mut result = settlement_callback(
        &input,
        &callback,
        &GraduationResult {
            factory: input.receiver.account_id,
            request_hash: [0; 32],
        },
    );
    assert_eq!(handle(&result).err(), Some(Error::InvalidState));
    result.message = borsh::to_vec(&GraduationResult {
        factory: input.receiver.account_id,
        request_hash: request.hash(),
    })
    .unwrap();
    let mut forged = result.clone();
    forged.from = Some(Actor::new(result.from.unwrap().account_id, id(99)));
    assert_eq!(handle(&forged).err(), Some(Error::Unauthorized));
    let finalize = handle(&result).unwrap();
    assert!(finalize.events.is_empty());
    assert_eq!(
        factory_core::LbpLaunchState::decode(finalize.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_progress,
        SettlementProgress::Unselected
    );
    let completed = settlement_callback(
        &input,
        &finalize,
        &SettlementReply::Completed {
            receipt_hash: receipt.hash(),
            progress: receipt.progress,
        },
    );
    let response = handle(&completed).unwrap();
    assert_eq!(
        launch_events(&response),
        vec![SaleEventV1::new(
            *state.namespace.value(),
            PricingEngineV1::FixedPrice,
            EventSubjectV1::Launch {
                factory: *input.receiver.account_id.value(),
                pool: *state.pool_id.value()
            },
            SaleEventKindV1::Graduated {
                adapter_program: *id(9).value(),
                adapter_custody: *pool.graduation_adapter.unwrap().account_id.value(),
                launch_tokens: 199,
                collateral: 2
            }
        )]
    );
    assert_eq!(
        factory_core::LbpLaunchState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_progress,
        SettlementProgress::Graduated
    );
    let mut replay = input;
    replay.pre_state = response.post_state.unwrap();
    replay.message = borsh::to_vec(&Message::ExecuteGraduationHook {
        creator_hint: Some(id(99)),
    })
    .unwrap();
    assert_eq!(handle(&replay).err(), Some(Error::AlreadySettled));
}

#[test]
fn fixed_zero_manual_completion_is_final_but_zero_first_graduation_is_available() {
    use factory_core::{SettlementProgress, SettlementReply};
    let (input, pending, receipt) = manual_pending(0);
    assert_eq!(receipt.progress, SettlementProgress::ManualComplete);
    let applied = settlement_callback(
        &input,
        &pending,
        &SettlementReply::Applied {
            receipt: receipt.clone(),
        },
    );
    let finalize = handle(&applied).unwrap();
    assert!(pending.events.is_empty());
    assert!(finalize.events.is_empty());
    let completed = settlement_callback(
        &input,
        &finalize,
        &SettlementReply::Completed {
            receipt_hash: receipt.hash(),
            progress: receipt.progress,
        },
    );
    let response = handle(&completed).unwrap();
    assert!(response.events.is_empty());
    assert_eq!(
        factory_core::LbpLaunchState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_progress,
        SettlementProgress::ManualComplete
    );
    let mut final_state =
        factory_core::LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    final_state.graduation_hook = Some(factory_core::GraduationHook {
        program_id: id(9),
        configuration: vec![],
    });
    let mut retry = completed;
    retry.pre_state = borsh::to_vec(&final_state).unwrap().into();
    retry.from = None;
    retry.message = borsh::to_vec(&Message::ExecuteGraduationHook {
        creator_hint: Some(id(99)),
    })
    .unwrap();
    assert_eq!(handle(&retry).err(), Some(Error::ManualPayoutStarted));
    let (input, resolve, state, pool) = fixed_hook_resolve(SettlementProgress::Unselected, 0);
    let resolved = settlement_callback(
        &input,
        &resolve,
        &lez_compat_core::authority::CurrentAuthority {
            role: state.creator_role_id,
            authority: id(99),
        },
    );
    let pending = handle(&resolved).unwrap();
    assert!(matches!(
        borsh::from_slice::<lbp_core::Message>(&pending.sync_messages.last().unwrap().message)
            .unwrap(),
        lbp_core::Message::Settle {
            route: factory_core::SettlementRoute::Graduation,
            ..
        }
    ));
    let receipt = factory_core::SettlementReceipt {
        factory: input.receiver,
        pool: Actor::new(state.pool_id, state.lbp_program_id),
        route: factory_core::SettlementRoute::Graduation,
        creator: id(99),
        adapter: pool.graduation_adapter,
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
    let applied = settlement_callback(
        &input,
        &pending,
        &SettlementReply::Applied {
            receipt: receipt.clone(),
        },
    );
    let callback = handle(&applied).unwrap();
    assert!(callback.events.is_empty());
    let factory_core::AdapterMessage::Graduate { request } =
        borsh::from_slice(&callback.sync_messages[0].message).unwrap()
    else {
        panic!("graduate");
    };
    let result = settlement_callback(
        &input,
        &callback,
        &factory_core::GraduationResult {
            factory: input.receiver.account_id,
            request_hash: request.hash(),
        },
    );
    let finalize = handle(&result).unwrap();
    assert!(finalize.events.is_empty());
    let completed = settlement_callback(
        &input,
        &finalize,
        &SettlementReply::Completed {
            receipt_hash: receipt.hash(),
            progress: receipt.progress,
        },
    );
    let response = handle(&completed).unwrap();
    assert!(matches!(
        &launch_events(&response)[..],
        [SaleEventV1 {
            kind: SaleEventKindV1::Graduated {
                launch_tokens: 199,
                collateral: 0,
                ..
            },
            ..
        }]
    ));
}

#[test]
fn fixed_settlement_rejects_stale_progress_adapter_and_immutable_pool_terms() {
    let (mut input, mut state) = completed();
    state.graduation_hook = Some(factory_core::GraduationHook {
        program_id: id(9),
        configuration: vec![],
    });
    let original = active_pool(&state, input.receiver);
    for case in 0..7 {
        let mut pool = original.clone();
        match case {
            0 => pool.settlement_progress = factory_core::SettlementProgress::ManualComplete,
            1 => pool.graduation_adapter = None,
            2 => pool.factory_program_id = id(99),
            3 => pool.start_timestamp += 1,
            4 => pool.namespace_program_id = id(99),
            5 => pool.sale_inventory += 1,
            _ => pool.trade_mode = factory_core::TradeMode::BuyAndSell,
        }
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.from = None;
        input.message = borsh::to_vec(&Message::ExecuteGraduationHook {
            creator_hint: Some(id(42)),
        })
        .unwrap();
        let response = handle(&input).unwrap();
        input = reply(&input, &response, borsh::to_vec(&pool).unwrap());
        assert_eq!(
            handle(&input).err(),
            Some(if case == 1 || case == 6 {
                Error::InvalidCustody
            } else {
                Error::InvalidState
            })
        );
    }
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.from = None;
    input.message = borsh::to_vec(&Message::ExecuteGraduationHook { creator_hint: None }).unwrap();
    assert_eq!(handle(&input).err(), Some(Error::InvalidTerms));
}

#[test]
fn fixed_graduation_policy_validates_creation_and_blocks_manual_before_any_noop() {
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
                launch_bps: 1,
                collateral_bps: 0,
            },
            GraduationPolicy {
                launch_bps: 0,
                collateral_bps: 10_000,
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
            let mut input = creation();
            input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
            let result = handle(&input);
            if policy.validate(hook.is_some()).is_err() {
                assert_eq!(result.err(), Some(Error::InvalidTerms));
            } else {
                assert_eq!(
                    factory_core::LbpLaunchState::decode(
                        result.unwrap().post_state.as_ref().unwrap()
                    )
                    .unwrap()
                    .graduation_policy,
                    policy
                );
            }
        }
    }
    for progress in [
        SettlementProgress::Unselected,
        SettlementProgress::ManualComplete,
        SettlementProgress::Graduated,
    ] {
        let (mut input, mut state) = completed();
        state.graduation_hook = Some(GraduationHook {
            program_id: id(9),
            configuration: vec![],
        });
        state.settlement_progress = progress;
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.message = borsh::to_vec(&Message::WithdrawFactoryProceeds {
            creator: id(42),
            now: 0,
        })
        .unwrap();
        assert_eq!(handle(&input).err(), Some(Error::InvalidTerms));
    }
}

#[test]
fn fixed_payout_and_allocation_bind_each_saved_graduation_percentage() {
    let (input, resolve, state, pool) =
        fixed_hook_resolve(factory_core::SettlementProgress::Unselected, 49);
    let resolved = settlement_callback(
        &input,
        &resolve,
        &lez_compat_core::authority::CurrentAuthority {
            role: state.creator_role_id,
            authority: id(99),
        },
    );
    let pending = handle(&resolved).unwrap();
    let receipt = factory_core::SettlementReceipt {
        factory: input.receiver,
        pool: Actor::new(state.pool_id, state.lbp_program_id),
        route: factory_core::SettlementRoute::Graduation,
        creator: id(99),
        adapter: pool.graduation_adapter,
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
                true,
            )
            .unwrap(),
        progress: factory_core::SettlementProgress::Graduated,
    };
    for case in 0..6 {
        let mut changed = receipt.clone();
        match case {
            0 => changed.eligible.launch_tokens += 1,
            1 => changed.eligible.collateral += 1,
            2 => changed.payouts.creator.launch_tokens += 1,
            3 => changed.payouts.creator.collateral += 1,
            4 => changed.payouts.adapter.launch_tokens += 1,
            _ => changed.payouts.adapter.collateral += 1,
        }
        assert_eq!(
            handle(&settlement_callback(
                &input,
                &pending,
                &factory_core::SettlementReply::Applied { receipt: changed }
            ))
            .err(),
            Some(Error::InvalidState)
        );
    }
    for launch in [true, false] {
        let mut changed = pool.clone();
        if launch {
            changed.graduation_policy.launch_bps += 1;
        } else {
            changed.graduation_policy.collateral_bps += 1;
        }
        assert_eq!(
            claim_allocation(&input, &state, &changed, 200, 200).err(),
            Some(Error::InvalidCustody)
        );
        let mut request = input.clone();
        request.from = None;
        request.pre_state = borsh::to_vec(&state).unwrap().into();
        request.message = borsh::to_vec(&Message::ExecuteGraduationHook {
            creator_hint: Some(id(99)),
        })
        .unwrap();
        let response = handle(&request).unwrap();
        request = reply(&request, &response, borsh::to_vec(&changed).unwrap());
        assert_eq!(handle(&request).err(), Some(Error::InvalidCustody));
    }
}
