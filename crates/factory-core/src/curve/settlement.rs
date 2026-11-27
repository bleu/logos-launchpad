//! Creator allocation, manual settlement and graduation for curve sales.
use super::*;

fn curve_payouts(
    state: &FactoryState,
    factory: Actor,
    mut response: Response,
    creator: AccountId,
    adapter: Option<AccountId>,
    payouts: SettlementPayouts,
) -> Response {
    let mut effects = vec![];
    response = reserve_payout(
        response,
        factory,
        state.namespace,
        state.launch_salt,
        state.token_definition_id,
        adapter.unwrap_or(creator),
        state.graduation_reserve,
        &mut effects,
    );
    for (recipient, amounts, launch_purpose, collateral_purpose) in [
        (
            creator,
            payouts.creator,
            PayoutPurposeV1::CreatorLaunch,
            PayoutPurposeV1::CreatorProceeds,
        ),
        (
            adapter.unwrap_or(creator),
            payouts.adapter,
            PayoutPurposeV1::AdapterLaunch,
            PayoutPurposeV1::AdapterCollateral,
        ),
    ] {
        for (definition, amount, purpose) in [
            (
                state.token_definition_id,
                amounts.launch_tokens,
                launch_purpose,
            ),
            (
                state.collateral_definition_id,
                amounts.collateral,
                collateral_purpose,
            ),
        ] {
            if amount != 0 {
                response = transfer(
                    response,
                    factory.account_id,
                    compute_factory_seed(state.namespace, state.launch_salt),
                    definition,
                    recipient,
                    amount,
                );
                effects.push(SettlementEffectV1::Payout {
                    definition: *definition.value(),
                    recipient: *recipient.value(),
                    purpose,
                    liability_discharged: amount,
                    tokens_transferred: if recipient == factory.account_id {
                        0
                    } else {
                        amount
                    },
                });
            }
        }
    }
    response.event(state.event(
        factory.account_id,
        SaleEventKindV1::Settlement {
            step: SettlementStepV1::Payout,
            route: Some(if adapter.is_some() {
                SettlementRouteV1::Graduation
            } else {
                SettlementRouteV1::Manual
            }),
            effects,
        },
    ))
}

pub(crate) fn begin_claim(input: &ReceiveInput, command: Message) -> Result<Response, Error> {
    let mut state = FactoryState::decode(&input.pre_state)?;
    state.validate(input.receiver)?;
    if state.creation_stage != CreationStage::Active {
        return Err(Error::CreationPending);
    }
    if state.creator_allocation_claimed {
        return Err(Error::AlreadyClaimed);
    }
    let Message::ClaimCreatorAllocation { creator, .. } = command else {
        return Err(Error::InvalidMessage);
    };
    let mut reads = vec![Read::new(
        Actor::new(state.pool_id, state.curve_program_id),
        &curve_core::Message::ReadState,
    )];
    if state.creator_allocation != 0 {
        reads.push(holding_read(
            state.creator_escrow_id,
            state.token_definition_id,
        ))
    }
    state.pending = Some(Pending {
        command,
        proof: true,
        reads,
        values: vec![],
    });
    Ok(state.response().send_sync(
        Actor::new(creator, AUTHORITY_PROGRAM_ACCOUNT_ID),
        &authority::Message::Check {
            role: state.creator_role_id,
            callback: input.receiver,
        },
    ))
}

pub(crate) fn begin_settlement(input: &ReceiveInput, command: Message) -> Result<Response, Error> {
    let mut state = FactoryState::decode(&input.pre_state)?;
    state.validate(input.receiver)?;
    if state.creation_stage != CreationStage::Active {
        return Err(Error::CreationPending);
    }
    let (creator, is_hook) = match command {
        Message::WithdrawFactoryProceeds { creator, .. } => (creator, false),
        Message::ExecuteGraduationHook { creator_hint } => {
            (creator_hint.ok_or(Error::Unauthorized)?, true)
        }
        _ => return Err(Error::InvalidMessage),
    };
    if creator == AccountId::default() {
        return Err(Error::Unauthorized);
    }
    let route = if is_hook {
        SettlementRoute::Graduation
    } else {
        SettlementRoute::Manual
    };
    if is_hook && state.graduation_hook.is_none() {
        return Err(Error::HookNotConfigured);
    }
    state
        .graduation_policy
        .validate_route(route, state.graduation_hook.is_some())?;
    state.settlement_stage.progress().ensure_available(route)?;
    state.pending = Some(Pending {
        command,
        proof: true,
        reads: vec![
            Read::new(
                Actor::new(state.pool_id, state.curve_program_id),
                &curve_core::Message::ReadState,
            ),
            holding_read(input.receiver.account_id, state.token_definition_id),
            holding_read(input.receiver.account_id, state.collateral_definition_id),
        ],
        values: vec![],
    });
    if is_hook
        && matches!(
            state.settlement_stage,
            SettlementStage::Withdrawn { .. } | SettlementStage::HookReady { .. }
        )
    {
        let program = state.graduation_hook.as_ref().unwrap().program_id;
        state.pending.as_mut().unwrap().reads.push(Read::new(
            Actor::new(
                compute_graduation_pda(program, input.receiver.account_id),
                program,
            ),
            &AdapterMessage::ReadState,
        ));
    }
    let response = state
        .response()
        .send_sync(
            Actor::new(state.collateral_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
            &token::Message::Initialize {
                owner: input.receiver.account_id,
            },
        )
        .send_sync(
            Actor::new(state.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
            &token::Message::Initialize { owner: creator },
        )
        .send_sync(
            Actor::new(state.collateral_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
            &token::Message::Initialize { owner: creator },
        );
    if is_hook {
        Ok(response.send_sync(
            Actor::new(state.creator_role_id, AUTHORITY_PROGRAM_ACCOUNT_ID),
            &authority::Message::Resolve,
        ))
    } else {
        Ok(response.send_sync(
            Actor::new(creator, AUTHORITY_PROGRAM_ACCOUNT_ID),
            &authority::Message::Check {
                role: state.creator_role_id,
                callback: input.receiver,
            },
        ))
    }
}

pub(super) fn finish_claim(
    input: &ReceiveInput,
    mut state: FactoryState,
    creator: AccountId,
    values: &[Vec<u8>],
) -> Result<Response, Error> {
    let saved = curve_core::PoolAccount::decode(&values[0]).map_err(|_| Error::InvalidState)?;
    validate_pool(&state, input.receiver, &saved)?;
    if !saved.funded {
        return Err(Error::CreationPending);
    }
    let expiry = saved
        .pool
        .ending_decision()
        .map_err(|_| Error::NotClosed)?
        .inclusion_bounds()
        .at_least;
    if state.creator_allocation != 0
        && balance(
            &values[1],
            state.creator_escrow_id,
            state.token_definition_id,
        )? < state.creator_allocation
    {
        return Err(Error::InvalidCustody);
    }
    state.creator_allocation_claimed = true;
    Ok(state.allocation_event(
        transfer(
            curve_core::with_time_bounds(state.response(), expiry, None)
                .map_err(|_| Error::InvalidTerms)?,
            state.creator_escrow_id,
            compute_escrow_seed(state.namespace, state.launch_salt),
            state.token_definition_id,
            creator,
            state.creator_allocation,
        ),
        input.receiver.account_id,
        creator,
    ))
}

pub(super) fn finish_payout(
    input: &ReceiveInput,
    mut state: FactoryState,
    command: Message,
    values: &[Vec<u8>],
) -> Result<Response, Error> {
    let (now, creator, route) = match command {
        Message::WithdrawFactoryProceeds { now, creator } => {
            (now, creator, SettlementRoute::Manual)
        }
        Message::ExecuteGraduationHook { creator_hint } => (
            0,
            creator_hint.ok_or(Error::Unauthorized)?,
            SettlementRoute::Graduation,
        ),
        _ => unreachable!(),
    };
    state
        .graduation_policy
        .validate_route(route, state.graduation_hook.is_some())?;
    let saved = curve_core::PoolAccount::decode(&values[0]).map_err(|_| Error::InvalidState)?;
    validate_pool(&state, input.receiver, &saved)?;
    if !saved.funded {
        return Err(Error::CreationPending);
    }
    let expiry = saved
        .pool
        .ending_decision()
        .map_err(|_| Error::NotClosed)?
        .inclusion_bounds()
        .at_least;
    let launch_balance = balance(
        &values[1],
        input.receiver.account_id,
        state.token_definition_id,
    )?;
    let collateral_balance = balance(
        &values[2],
        input.receiver.account_id,
        state.collateral_definition_id,
    )?;
    let (unsold, collateral_due) = match state.settlement_stage {
        SettlementStage::Unstarted => {
            state.settlement_stage = SettlementStage::Ready;
            return Ok(curve_core::with_time_bounds(state.response(), expiry, None)
                .map_err(|_| Error::InvalidTerms)?
                .event(state.event(
                    input.receiver.account_id,
                    SaleEventKindV1::Settlement {
                        step: SettlementStepV1::Prepared,
                        route: None,
                        effects: vec![],
                    },
                )));
        }
        SettlementStage::Ready => {
            state.settlement_stage = SettlementStage::Withdrawn {
                unsold: saved.pool.real_reserve0,
                collateral_due: saved.pool.real_reserve1,
            };
            return Ok(curve_core::with_time_bounds(state.response(), expiry, None)
                .map_err(|_| Error::InvalidTerms)?
                .send_sync(
                    Actor::new(state.pool_id, state.curve_program_id),
                    &curve_core::Message::WithdrawReserves { now },
                ));
        }
        SettlementStage::Withdrawn {
            unsold,
            collateral_due,
        }
        | SettlementStage::HookReady {
            unsold,
            collateral_due,
        } => (unsold, collateral_due),
        _ => return Err(Error::AlreadySettled),
    };
    if saved.pool.real_reserve0 != 0
        || saved.pool.real_reserve1 != 0
        || launch_balance < unsold
        || collateral_balance < collateral_due
    {
        return Err(Error::InvalidCustody);
    }
    let eligible = SettlementAmounts {
        launch_tokens: unsold,
        collateral: collateral_due,
    };
    let payouts =
        state
            .graduation_policy
            .payouts(eligible, route, state.graduation_hook.is_some())?;
    if route == SettlementRoute::Manual {
        state.settlement_stage = SettlementStage::Complete;
        let response = curve_core::with_time_bounds(state.response(), expiry, None)
            .map_err(|_| Error::InvalidTerms)?;
        return Ok(
            curve_payouts(&state, input.receiver, response, creator, None, payouts).event(
                state.event(
                    input.receiver.account_id,
                    SaleEventKindV1::Settlement {
                        step: SettlementStepV1::Completed,
                        route: Some(SettlementRouteV1::Manual),
                        effects: vec![],
                    },
                ),
            ),
        );
    }
    let hook = state
        .graduation_hook
        .as_ref()
        .ok_or(Error::HookNotConfigured)?;
    let program = hook.program_id;
    let custody = compute_graduation_pda(program, input.receiver.account_id);
    if [
        input.receiver.account_id,
        state.pool_id,
        state.creator_escrow_id,
        state.token_definition_id,
        state.collateral_definition_id,
        creator,
    ]
    .contains(&custody)
    {
        return Err(Error::InvalidCustody);
    }
    let adapter = AdapterState::decode(&values[3])?;
    if adapter.factory != input.receiver || adapter.receipt.is_some() {
        return Err(Error::InvalidCustody);
    }
    if adapter.accepted_intent != GraduationIntentKind::Curve {
        return Err(Error::AdapterIntentMismatch);
    }
    if matches!(state.settlement_stage, SettlementStage::Withdrawn { .. }) {
        state.settlement_stage = SettlementStage::HookReady {
            unsold,
            collateral_due,
        };
        return Ok(curve_core::with_time_bounds(state.response(), expiry, None)
            .map_err(|_| Error::InvalidTerms)?
            .event(state.event(
                input.receiver.account_id,
                SaleEventKindV1::Settlement {
                    step: SettlementStepV1::Prepared,
                    route: None,
                    effects: vec![],
                },
            ))
            .send_sync(
                Actor::new(state.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                &token::Message::Initialize { owner: custody },
            )
            .send_sync(
                Actor::new(state.collateral_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                &token::Message::Initialize { owner: custody },
            ));
    }
    let request = GraduationRequest {
        factory_program_id: input.receiver.program_account_id,
        namespace: state.namespace,
        launch_salt: state.launch_salt,
        token_definition_id: state.token_definition_id,
        collateral_definition_id: state.collateral_definition_id,
        intent: GraduationIntent::Curve {
            launch_tokens: payouts
                .adapter
                .launch_tokens
                .checked_add(state.graduation_reserve)
                .ok_or(Error::Overflow)?,
            collateral: payouts.adapter.collateral,
        },
        reserve_tokens: state.graduation_reserve,
        unsold_tokens: payouts.adapter.launch_tokens,
        configuration: hook.configuration.clone(),
    };
    state.pending = Some(Pending {
        command: Message::ExecuteGraduationHook {
            creator_hint: Some(creator),
        },
        proof: false,
        reads: vec![],
        values: vec![request.hash().to_vec()],
    });
    let response = curve_core::with_time_bounds(state.response(), expiry, None)
        .map_err(|_| Error::InvalidTerms)?;
    // The adapter receives no factory PDA grant.
    Ok(curve_payouts(
        &state,
        input.receiver,
        response,
        creator,
        Some(custody),
        payouts,
    )
    .send_sync(
        Actor::new(custody, program),
        &AdapterMessage::Graduate { request },
    ))
}

pub(super) fn finish_graduation(
    input: &ReceiveInput,
    mut state: FactoryState,
    pending: Pending,
) -> Result<Response, Error> {
    let program = state
        .graduation_hook
        .as_ref()
        .ok_or(Error::HookNotConfigured)?
        .program_id;
    if input.from
        != Some(Actor::new(
            compute_graduation_pda(program, input.receiver.account_id),
            program,
        ))
    {
        return Err(Error::Unauthorized);
    }
    let result: GraduationResult =
        borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    if result.factory != input.receiver.account_id
        || pending.values.as_slice() != [result.request_hash.to_vec()]
        || !matches!(state.settlement_stage, SettlementStage::HookReady { .. })
    {
        return Err(Error::InvalidState);
    }
    let mut progress = state.settlement_stage.progress();
    progress.finish_graduation()?;
    let SettlementStage::HookReady {
        unsold,
        collateral_due,
    } = state.settlement_stage
    else {
        return Err(Error::InvalidState);
    };
    let amounts = state
        .graduation_policy
        .payouts(
            SettlementAmounts {
                launch_tokens: unsold,
                collateral: collateral_due,
            },
            SettlementRoute::Graduation,
            true,
        )?
        .adapter;
    state.settlement_stage = SettlementStage::Graduated;
    Ok(state
        .response()
        .event(state.event(
            input.receiver.account_id,
            SaleEventKindV1::Settlement {
                step: SettlementStepV1::Completed,
                route: Some(SettlementRouteV1::Graduation),
                effects: vec![],
            },
        ))
        .event(
            state.event(
                input.receiver.account_id,
                SaleEventKindV1::Graduated {
                    adapter_program: *program.value(),
                    adapter_custody: *compute_graduation_pda(program, input.receiver.account_id)
                        .value(),
                    launch_tokens: amounts
                        .launch_tokens
                        .checked_add(state.graduation_reserve)
                        .ok_or(Error::Overflow)?,
                    collateral: amounts.collateral,
                },
            ),
        ))
}
