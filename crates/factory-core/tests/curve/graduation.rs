use super::*;

fn hook_ready() -> (ReceiveInput, Response, curve_core::PoolAccount) {
    let (input, withdrawn, mut saved) = withdrawn_for_route(true, 53);
    saved.pool.real_reserve0 = 0;
    saved.pool.real_reserve1 = 0;
    let (mut input, response) = settlement(&input, &withdrawn, saved.clone(), 817, 70, true);
    input = reply(
        &input,
        &response,
        adapter_state(input.receiver, response.post_state.as_ref().unwrap()),
    );
    let response = handle(&input).unwrap();
    (input, response, saved)
}

#[test]
fn hook_preparation_saves_both_liabilities_and_forbids_manual_override() {
    let (mut input, response, _) = hook_ready();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.settlement_stage,
        SettlementStage::HookReady {
            unsold: 800,
            collateral_due: 53
        }
    );
    assert!(
        token_calls(&response)
            .iter()
            .all(|call| matches!(call, token::Message::Initialize { .. }))
    );
    input.pre_state = response.post_state.unwrap();
    input.from = None;
    input.message = borsh::to_vec(&Message::WithdrawFactoryProceeds {
        creator: id(42),
        now: 400,
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(Error::InvalidTerms));
}

#[test]
fn hook_finalization_conserves_both_assets_and_binds_the_callback_hash() {
    let (input, ready, saved) = hook_ready();
    let (mut input, response) = settlement(&input, &ready, saved, 817, 70, true);
    input = reply(
        &input,
        &response,
        adapter_state(input.receiver, response.post_state.as_ref().unwrap()),
    );
    let response = handle(&input).unwrap();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.settlement_stage,
        SettlementStage::HookReady {
            unsold: 800,
            collateral_due: 53
        }
    );
    let custody = compute_graduation_pda(id(9), input.receiver.account_id);
    let calls = token_calls(&response);
    for (definition, adapter, creator, eligible) in [
        (state.token_definition_id, 200, 600, 800),
        (state.collateral_definition_id, 17, 36, 53),
    ] {
        assert!(calls.contains(&token::Message::Transfer {
            definition,
            recipient: custody,
            amount: adapter
        }));
        assert!(calls.contains(&token::Message::Transfer {
            definition,
            recipient: id(42),
            amount: creator
        }));
        assert_eq!(adapter + creator, eligible);
    }
    assert_eq!(calls.len(), 4);
    assert!(
        response
            .sync_messages
            .iter()
            .filter(|m| m.to.program_account_id == TOKEN_PROGRAM_ACCOUNT_ID)
            .all(|m| m.pda_seeds.contains(&compute_factory_seed(id(40), [3; 32])))
    );
    let effects: Vec<_> = launch_events(&response)
        .into_iter()
        .flat_map(|event| match event.kind {
            SaleEventKindV1::Settlement { effects, .. } => effects,
            _ => vec![],
        })
        .collect();
    for (definition, expected) in [
        (state.token_definition_id, 800),
        (state.collateral_definition_id, 53),
    ] {
        let total: u128 = effects
            .iter()
            .filter_map(|effect| match effect {
                SettlementEffectV1::Payout {
                    definition: actual,
                    liability_discharged,
                    tokens_transferred,
                    ..
                } if actual == definition.value() => {
                    assert_eq!(liability_discharged, tokens_transferred);
                    Some(*tokens_transferred)
                }
                _ => None,
            })
            .sum();
        assert_eq!(total, expected);
    }
    let callback = response.sync_messages.last().unwrap();
    assert_eq!(callback.to, Actor::new(custody, id(9)));
    assert!(callback.pda_seeds.is_empty());
    let AdapterMessage::Graduate { request } = borsh::from_slice(&callback.message).unwrap() else {
        panic!("graduate")
    };
    assert_eq!(
        request.intent,
        GraduationIntent::Curve {
            launch_tokens: 200,
            collateral: 17
        }
    );
    assert_eq!(request.configuration, b"route");
    input.pre_state = response.post_state.unwrap();
    input.from = Some(Actor::new(custody, id(9)));
    let valid = borsh::to_vec(&GraduationResult {
        factory: input.receiver.account_id,
        request_hash: request.hash(),
    })
    .unwrap();
    for intent in [
        GraduationIntent::Curve {
            launch_tokens: 201,
            collateral: 17,
        },
        GraduationIntent::Curve {
            launch_tokens: 200,
            collateral: 18,
        },
    ] {
        let mut changed = request.clone();
        changed.intent = intent;
        input.message = borsh::to_vec(&GraduationResult {
            factory: input.receiver.account_id,
            request_hash: changed.hash(),
        })
        .unwrap();
        assert_eq!(handle(&input).err(), Some(Error::InvalidState));
    }
    input.message = valid.clone();
    input.from = Some(Actor::new(custody, id(99)));
    assert_eq!(handle(&input).err(), Some(Error::Unauthorized));
    input.from = Some(Actor::new(custody, id(9)));
    input.message = borsh::to_vec(&Message::ReadState).unwrap();
    assert_eq!(handle(&input).err(), Some(Error::InvalidMessage));
    input.message = valid;
    let complete = handle(&input).unwrap();
    assert_eq!(
        FactoryState::decode(complete.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_stage,
        SettlementStage::Graduated
    );
    assert!(matches!(
        &launch_events(&complete)[..],
        [
            SaleEventV1 {
                kind: SaleEventKindV1::Settlement {
                    step: SettlementStepV1::Completed,
                    route: Some(SettlementRouteV1::Graduation),
                    ..
                },
                ..
            },
            SaleEventV1 {
                kind: SaleEventKindV1::Graduated {
                    launch_tokens: 200,
                    collateral: 17,
                    ..
                },
                ..
            }
        ]
    ));
}

#[test]
fn configured_adapter_blocks_manual_settlement_at_every_stage_even_with_zero_shares() {
    for stage in [
        SettlementStage::Unstarted,
        SettlementStage::Ready,
        SettlementStage::Withdrawn {
            unsold: 0,
            collateral_due: 0,
        },
        SettlementStage::HookReady {
            unsold: 0,
            collateral_due: 0,
        },
        SettlementStage::Complete,
        SettlementStage::Graduated,
    ] {
        let (mut input, active) = active_for_route(true);
        let mut state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
        state.settlement_stage = stage;
        state.graduation_policy = GraduationPolicy {
            launch_bps: 0,
            collateral_bps: 0,
        };
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.from = None;
        input.message = borsh::to_vec(&Message::WithdrawFactoryProceeds {
            creator: id(42),
            now: 400,
        })
        .unwrap();
        assert_eq!(
            handle(&input).err(),
            Some(Error::InvalidTerms),
            "stage {stage:?}"
        );
    }
}

#[test]
fn hook_requires_a_saved_adapter_and_manual_completion_cannot_reopen_it() {
    for stage in [
        SettlementStage::Unstarted,
        SettlementStage::Ready,
        SettlementStage::Withdrawn {
            unsold: 0,
            collateral_due: 0,
        },
        SettlementStage::Complete,
        SettlementStage::Graduated,
    ] {
        let (mut input, active) = active();
        let mut state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
        state.settlement_stage = stage;
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.from = None;
        input.message = borsh::to_vec(&Message::ExecuteGraduationHook {
            creator_hint: Some(id(42)),
        })
        .unwrap();
        assert_eq!(handle(&input).err(), Some(Error::HookNotConfigured));
    }
}

#[test]
fn graduation_resolves_and_binds_the_current_creator_hint_before_custody_reads() {
    let (input, ready, saved) = hook_ready();
    let mut request = input.clone();
    request.pre_state = ready.post_state.clone().unwrap();
    request.from = None;
    for hint in [None, Some(AccountId::default())] {
        request.message =
            borsh::to_vec(&Message::ExecuteGraduationHook { creator_hint: hint }).unwrap();
        assert_eq!(handle(&request).err(), Some(Error::Unauthorized));
    }
    request.message = borsh::to_vec(&Message::ExecuteGraduationHook {
        creator_hint: Some(id(99)),
    })
    .unwrap();
    let resolving = handle(&request).unwrap();
    let valid = resolved(&request, &resolving, id(99));
    for (sender, role, creator) in [
        (
            Actor::new(id(99), AUTHORITY_PROGRAM_ACCOUNT_ID),
            compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
            id(99),
        ),
        (
            Actor::new(
                compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
                id(99),
            ),
            compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
            id(99),
        ),
        (
            Actor::new(
                compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
                AUTHORITY_PROGRAM_ACCOUNT_ID,
            ),
            id(99),
            id(99),
        ),
        (
            Actor::new(
                compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
                AUTHORITY_PROGRAM_ACCOUNT_ID,
            ),
            compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
            id(42),
        ),
    ] {
        let mut forged = valid.clone();
        forged.from = Some(sender);
        forged.message = borsh::to_vec(&authority::CurrentAuthority {
            role,
            authority: creator,
        })
        .unwrap();
        assert_eq!(handle(&forged).err(), Some(Error::Unauthorized));
    }
    let (mut input, response) = settlement_as(&input, &ready, saved, 817, 70, true, id(99));
    input = reply(
        &input,
        &response,
        adapter_state(input.receiver, response.post_state.as_ref().unwrap()),
    );
    let response = handle(&input).unwrap();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    let transfers = token_calls(&response);
    assert!(transfers.contains(&token::Message::Transfer {
        definition: state.token_definition_id,
        recipient: id(99),
        amount: 600
    }));
    assert!(transfers.contains(&token::Message::Transfer {
        definition: state.collateral_definition_id,
        recipient: id(99),
        amount: 36
    }));
    assert!(!transfers.iter().any(
        |call| matches!(call, token::Message::Transfer { recipient, .. } if *recipient == id(42))
    ));
}

#[test]
fn curve_zero_balances_finalize_once_on_the_configured_route() {
    for hook in [false, true] {
        let (input, active) = active_for_route(hook);
        let mut state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
        state.settlement_stage = SettlementStage::Withdrawn {
            unsold: 0,
            collateral_due: 0,
        };
        let mut previous = active;
        previous.post_state = Some(borsh::to_vec(&state).unwrap().into());
        let mut saved = pool_state(&state, input.receiver, true);
        saved.pool.real_reserve0 = 0;
        saved.pool.real_reserve1 = 0;
        let (mut input, mut response) = settlement(&input, &previous, saved.clone(), 17, 19, hook);
        if hook {
            input = reply(
                &input,
                &response,
                adapter_state(input.receiver, response.post_state.as_ref().unwrap()),
            );
            response = handle(&input).unwrap();
            let (next, pending) = settlement(&input, &response, saved, 17, 19, true);
            input = reply(
                &next,
                &pending,
                adapter_state(next.receiver, pending.post_state.as_ref().unwrap()),
            );
            response = handle(&input).unwrap();
            assert!(token_calls(&response).is_empty());
            let callback = response.sync_messages.last().unwrap();
            let AdapterMessage::Graduate { request } =
                borsh::from_slice(&callback.message).unwrap()
            else {
                panic!("graduate")
            };
            assert_eq!(
                request.intent,
                GraduationIntent::Curve {
                    launch_tokens: 0,
                    collateral: 0
                }
            );
            input.from = Some(callback.to);
            input.message = borsh::to_vec(&GraduationResult {
                factory: input.receiver.account_id,
                request_hash: request.hash(),
            })
            .unwrap();
            input.pre_state = response.post_state.unwrap();
            response = handle(&input).unwrap();
        }
        assert!(token_calls(&response).is_empty());
        let stage = FactoryState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_stage;
        assert_eq!(
            stage,
            if hook {
                SettlementStage::Graduated
            } else {
                SettlementStage::Complete
            }
        );
        input.pre_state = response.post_state.unwrap();
        input.from = None;
        input.message = borsh::to_vec(&if hook {
            Message::ExecuteGraduationHook {
                creator_hint: Some(id(42)),
            }
        } else {
            Message::WithdrawFactoryProceeds {
                creator: id(42),
                now: 400,
            }
        })
        .unwrap();
        assert_eq!(handle(&input).err(), Some(Error::AlreadySettled));
    }
}

#[test]
fn one_step_adapter_initialization_selects_curve_and_rejects_reinitialization() {
    let (input, response) = active_for_route(true);
    let factory = input.receiver;
    let state = adapter_state(factory, response.post_state.as_ref().unwrap());
    let decoded = AdapterState::decode(&state).unwrap();
    assert_eq!(decoded.accepted_intent, GraduationIntentKind::Curve);
    assert_eq!(decoded.factory, factory);
    assert!(decoded.receipt.is_none());
    let input = ReceiveInput {
        receiver: Actor::new(compute_graduation_pda(id(9), factory.account_id), id(9)),
        from: None,
        is_authorized: false,
        pre_state: state.into(),
        message: borsh::to_vec(&AdapterMessage::Initialize {
            factory_program_id: id(7),
            namespace: id(40),
            launch_salt: [3; 32],
            accepted_intent: GraduationIntentKind::FixedPrice,
        })
        .unwrap(),
    };
    assert_eq!(
        handle_adapter(&input).err(),
        Some(Error::AlreadyInitialized)
    );
}
