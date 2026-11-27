use super::*;

pub(super) fn event_selector(name: &str) -> [u8; 8] {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    Impl::hash_bytes(name.as_bytes()).as_bytes()[..8]
        .try_into()
        .unwrap()
}

pub(super) fn pool_subject(pool: AccountId) -> EventSubjectV1 {
    EventSubjectV1::Pool {
        pool: *pool.value(),
    }
}

pub(super) fn launch_subject(factory: AccountId, pool: AccountId) -> EventSubjectV1 {
    EventSubjectV1::Launch {
        factory: *factory.value(),
        pool: *pool.value(),
    }
}

pub(super) fn sale_events(
    events: &[TransactionEvent],
    program: AccountId,
    subject: EventSubjectV1,
) -> Vec<SaleEventV1> {
    events
        .iter()
        .filter(|event| {
            event.account_id == program
                && event.event.selector == event_selector(SaleEventV1::SELECTOR_NAME)
        })
        .map(|event| borsh::from_slice::<SaleEventV1>(&event.event.data).unwrap())
        .filter(|event| event.subject == subject)
        .inspect(|event| {
            assert_eq!(event.version, SaleEventV1::VERSION);
            assert_eq!(event.namespace, *NAMESPACE.value());
        })
        .collect()
}

pub(super) fn sale_event_kinds(
    events: &[TransactionEvent],
    program: AccountId,
    subject: EventSubjectV1,
) -> Vec<SaleEventKindV1> {
    sale_events(events, program, subject)
        .into_iter()
        .map(|event| event.kind)
        .collect()
}

fn expected_lbp_settlement_events(
    initial: &lbp_core::PoolAccount,
    creator: AccountId,
    treasury: AccountId,
    adapter: Option<AccountId>,
) -> Vec<SaleEventKindV1> {
    let mut events = vec![];
    if !initial.closure_observed {
        let (reason, bounds) = if matches!(
            initial.ending_policy,
            lbp_core::EndingPolicy::Depletion | lbp_core::EndingPolicy::Either
        ) && initial.remaining_inventory == 0
        {
            (
                ClosureReasonV1::Depleted {
                    definition: *initial.token_definition_id.value(),
                },
                EventTimeBoundsV1 {
                    at_least: None,
                    before: None,
                },
            )
        } else {
            let end = initial.end_timestamp.unwrap();
            (
                ClosureReasonV1::ScheduledEnd { timestamp: end },
                EventTimeBoundsV1 {
                    at_least: Some(end),
                    before: None,
                },
            )
        };
        events.push(SaleEventKindV1::Closed {
            reason,
            inclusion_bounds: bounds,
        });
    }
    if initial.accrued_fees != 0 {
        events.push(SaleEventKindV1::FeesCollected {
            definition: *COLLATERAL.value(),
            treasury: *treasury.value(),
            liability_discharged: initial.accrued_fees,
            tokens_transferred: if treasury
                == lbp_core::compute_pool_pda(
                    initial.namespace,
                    LBP,
                    initial.token_definition_id,
                    COLLATERAL,
                    initial.factory_owner,
                ) {
                0
            } else {
                initial.accrued_fees
            },
        });
    }
    let route = if adapter.is_some() {
        SettlementRouteV1::Graduation
    } else {
        SettlementRouteV1::Manual
    };
    let adapter_launch = if adapter.is_some() {
        initial.remaining_inventory * u128::from(initial.graduation_policy.launch_bps) / 10_000
    } else {
        0
    };
    let adapter_collateral = if adapter.is_some() {
        initial.recorded_collateral * u128::from(initial.graduation_policy.collateral_bps) / 10_000
    } else {
        0
    };
    for (definition, amount, recipient, purpose) in [
        (
            initial.token_definition_id,
            initial.remaining_inventory - adapter_launch,
            creator,
            PayoutPurposeV1::CreatorLaunch,
        ),
        (
            COLLATERAL,
            initial.recorded_collateral - adapter_collateral,
            creator,
            PayoutPurposeV1::CreatorProceeds,
        ),
        (
            initial.token_definition_id,
            adapter_launch,
            adapter.unwrap_or(creator),
            PayoutPurposeV1::AdapterLaunch,
        ),
        (
            COLLATERAL,
            adapter_collateral,
            adapter.unwrap_or(creator),
            PayoutPurposeV1::AdapterCollateral,
        ),
    ] {
        if amount != 0 {
            events.push(SaleEventKindV1::Settlement {
                step: SettlementStepV1::Payout,
                route: Some(route),
                effects: vec![SettlementEffectV1::Payout {
                    definition: *definition.value(),
                    recipient: *recipient.value(),
                    purpose,
                    liability_discharged: amount,
                    tokens_transferred: amount,
                }],
            });
        }
    }
    events.push(SaleEventKindV1::Settlement {
        step: SettlementStepV1::Completed,
        route: Some(route),
        effects: vec![],
    });
    events
}

pub(super) fn assert_lbp_settlement_events(
    events: &[TransactionEvent],
    pool: AccountId,
    initial: &lbp_core::PoolAccount,
    creator: AccountId,
    treasury: AccountId,
    adapter: Option<AccountId>,
) {
    let decoded = sale_events(events, LBP, pool_subject(pool));
    let engine = if matches!(initial.mode, lbp_core::PoolMode::Weighted { .. }) {
        PricingEngineV1::Weighted
    } else {
        PricingEngineV1::FixedPrice
    };
    assert!(decoded.iter().all(|event| event.engine == engine));
    assert_eq!(
        decoded
            .into_iter()
            .map(|event| event.kind)
            .collect::<Vec<_>>(),
        expected_lbp_settlement_events(initial, creator, treasury, adapter)
    );
}

pub(super) fn assert_lbp_graduated_event(
    events: &[TransactionEvent],
    env: &FixedFactoryEnv,
    weighted: bool,
    initial: &lbp_core::PoolAccount,
) {
    let launch_tokens =
        initial.remaining_inventory * u128::from(initial.graduation_policy.launch_bps) / 10_000;
    let collateral =
        initial.recorded_collateral * u128::from(initial.graduation_policy.collateral_bps) / 10_000;
    let decoded = sale_events(
        events,
        LBP_FACTORY_PROGRAM,
        launch_subject(env.factory, env.pool),
    );
    assert!(decoded.iter().all(|event| event.engine
        == if weighted {
            PricingEngineV1::Weighted
        } else {
            PricingEngineV1::FixedPrice
        }));
    assert_eq!(
        decoded
            .into_iter()
            .map(|event| event.kind)
            .collect::<Vec<_>>(),
        vec![SaleEventKindV1::Graduated {
            adapter_program: *LBP_HOOK_PROGRAM.value(),
            adapter_custody: *lbp_adapter(env).account_id.value(),
            launch_tokens,
            collateral,
        }]
    );
    let relevant: Vec<_> = events
        .iter()
        .filter(|event| event.event.selector == event_selector(SaleEventV1::SELECTOR_NAME))
        .collect();
    assert_eq!(relevant.last().unwrap().account_id, LBP_FACTORY_PROGRAM);
    assert!(matches!(
        borsh::from_slice::<SaleEventV1>(&relevant.last().unwrap().event.data)
            .unwrap()
            .kind,
        SaleEventKindV1::Graduated { .. }
    ));
}

pub(super) fn curve_event_env(end: Option<u64>) -> (V03State, AccountId, PrivateKey) {
    curve_event_env_with_sell_fee(end, 0)
}

pub(super) fn curve_event_env_with_sell_fee(
    end: Option<u64>,
    sell_fee_bps: u16,
) -> (V03State, AccountId, PrivateKey) {
    let key = PrivateKey::try_new([21; 32]).unwrap();
    let owner = AccountId::from(&PublicKey::new_from_private_key(&key));
    let bob = AccountId::from(&PublicKey::new_from_private_key(
        &PrivateKey::try_new([22; 32]).unwrap(),
    ));
    let (state, original_pool) = seeded_state_with_sell_fee(owner, bob, end, sell_fee_bps);
    let mut pool_state = PoolAccount::decode(
        state
            .get_account_by_id(original_pool)
            .data
            .actor_state(CURVE),
    )
    .unwrap();
    pool_state.owner = owner;
    pool_state.allowed_input_token = None;
    pool_state.trade_control = curve_core::TradeControl::Running;
    let pool = curve_core::compute_pool_pda(NAMESPACE, CURVE, LAUNCH, COLLATERAL, owner);
    let role = curve_core::compute_pool_owner_role(NAMESPACE, pool, CURVE);
    let state = state
        .with_programs_at(
            [(
                factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
                Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_AUTHORITY_ELF)).unwrap(),
            )]
            .map(|(address, program)| (address, program, true)),
        )
        .with_public_accounts([
            account(Actor::new(pool, CURVE), borsh::to_vec(&pool_state).unwrap()),
            seeded_role(role, owner),
            token_holding(pool, LAUNCH, 4_000),
            token_holding(pool, COLLATERAL, 0),
            token_holding(owner, LAUNCH, 0),
            token_holding(owner, COLLATERAL, 2_000),
            token_holding(TREASURY, COLLATERAL, 0),
            token_holding(NEXT_TREASURY, COLLATERAL, 0),
        ]);
    (fixed_config(state, 1_000, TREASURY, false), pool, key)
}

pub(super) fn curve_execute_events_at(
    state: &mut V03State,
    pool: AccountId,
    key: &PrivateKey,
    instruction: CurveMessage,
    timestamp: u64,
) -> Result<Vec<TransactionEvent>, lee::error::LeeError> {
    let owner = AccountId::from(&PublicKey::new_from_private_key(key));
    let collect = matches!(instruction, CurveMessage::CollectFees);
    let mut actors = vec![
        Actor::new(owner, CURVE),
        Actor::new(pool, CURVE),
        Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
        Actor::new(
            curve_core::compute_pool_owner_role(NAMESPACE, pool, CURVE),
            factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
        ),
        Actor::new(owner, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
        clock_core::clock_actor(),
    ];
    for account in [owner, pool, TREASURY, NEXT_TREASURY] {
        actors.extend([
            Actor::new(account, TOKEN_PROGRAM_ACCOUNT_ID),
            holding(account, LAUNCH),
            holding(account, COLLATERAL),
        ]);
    }
    let nonces = if collect {
        BTreeMap::new()
    } else {
        BTreeMap::from([(owner, state.get_account_by_id(owner).nonce)])
    };
    let instruction = if collect {
        instruction
    } else {
        CurveMessage::Execute {
            pool,
            instruction: Box::new(instruction),
        }
    };
    let mut message = Message::try_new(
        Actor::new(if collect { pool } else { owner }, CURVE),
        actors,
        nonces,
        instruction,
    )
    .unwrap();
    message.admission_evidence = vec![
        lee::PublicAccountEvidence::Pda {
            program: CURVE,
            seed: curve_core::compute_pool_pda_seed(NAMESPACE, LAUNCH, COLLATERAL, owner),
        },
        lee::PublicAccountEvidence::Pda {
            program: CURVE,
            seed: curve_core::compute_pool_owner_role_seed(NAMESPACE, pool),
        },
    ];
    for account in [owner, pool, TREASURY, NEXT_TREASURY] {
        for definition in [LAUNCH, COLLATERAL] {
            message
                .admission_evidence
                .push(lee::PublicAccountEvidence::Pda {
                    program: TOKEN_PROGRAM_ACCOUNT_ID,
                    seed: token::holding_seed(account, definition),
                });
        }
    }
    let keys = [key];
    let witnesses = WitnessSet::for_message(&message, if collect { &[] } else { &keys });
    state.transition_from_public_transaction(
        &PublicTransaction::new(message, witnesses),
        1,
        timestamp,
    )
}
