use super::*;

pub(super) fn seeded_state(
    alice: AccountId,
    bob: AccountId,
    close_timestamp: Option<u64>,
) -> (V03State, AccountId) {
    seeded_state_with_sell_fee(alice, bob, close_timestamp, 0)
}

pub(super) fn seeded_state_with_sell_fee(
    alice: AccountId,
    bob: AccountId,
    close_timestamp: Option<u64>,
    sell_fee_bps: u16,
) -> (V03State, AccountId) {
    let pool = curve_core::compute_pool_pda(NAMESPACE, CURVE, LAUNCH, COLLATERAL, OWNER);
    let curve = Program::new(Cow::Borrowed(launchpad_methods::CURVE_ELF)).unwrap();
    let token = Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_TOKEN_ELF)).unwrap();
    let pool_state = PoolAccount {
        buy_policy: curve_core::BuyPolicy::Open,
        trade_control: curve_core::TradeControl::Unrestricted,
        allowed_input_token: Some(pool::TokenSide::Token1),
        funded: true,
        closure_observed: false,
        block_allocation: Default::default(),
        namespace: NAMESPACE,
        token0_definition_id: LAUNCH,
        token1_definition_id: COLLATERAL,
        owner: OWNER,
        owner_program: None,
        pool: pool::Pool::create(pool::PoolCreateParams {
            token0_amount: 4_000,
            token1_amount: 0,
            virtual_reserve0: 10_000,
            virtual_reserve1: 1_000,
            ending_policy: if close_timestamp.is_some() {
                curve_core::EndingPolicy::Either
            } else {
                curve_core::EndingPolicy::Depletion
            },
            sell_fee_bps,
            close_timestamp,
            close_on_depletion: Some(pool::TokenSide::Token0),
        })
        .unwrap(),
    };
    // Seeded funded state does not test sale setup.
    let config = borsh::to_vec(&(
        *b"LZNSCONF",
        3u16,
        NAMESPACE,
        0u16,
        AccountId::new([12; 32]),
        (AccountId::new([13; 32]), false),
    ))
    .unwrap();
    curve_core::Config::decode(&config).unwrap();
    let mut accounts = vec![
        account(Actor::new(pool, CURVE), borsh::to_vec(&pool_state).unwrap()),
        account(
            Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
            config,
        ),
    ];
    for (owner, definition, balance) in [
        (pool, LAUNCH, 4_000),
        (pool, COLLATERAL, 0),
        (alice, LAUNCH, 0),
        (alice, COLLATERAL, 200),
        (bob, LAUNCH, 0),
        (bob, COLLATERAL, 200),
    ] {
        accounts.push(account(
            holding(owner, definition),
            borsh::to_vec(&TokenState::Holding {
                definition,
                owner,
                kind: TokenKind::Fungible,
                balance,
            })
            .unwrap(),
        ));
    }
    (
        V03State::new()
            .with_programs_at(
                [
                    (CURVE, curve),
                    (TOKEN_PROGRAM_ACCOUNT_ID, token),
                    (clock_core::clock_account_id(), programs::clock()),
                ]
                .map(|(address, program)| (address, program, true)),
            )
            .with_public_accounts(accounts),
        pool,
    )
}

pub(super) fn buy_transaction(
    state: &V03State,
    pool: AccountId,
    key: &PrivateKey,
    max_amount_in: u128,
    now: u64,
) -> PublicTransaction {
    let buyer = AccountId::from(&PublicKey::new_from_private_key(key));
    let actors = vec![
        Actor::new(buyer, CURVE),
        Actor::new(pool, CURVE),
        Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
        Actor::new(buyer, TOKEN_PROGRAM_ACCOUNT_ID),
        Actor::new(pool, TOKEN_PROGRAM_ACCOUNT_ID),
        holding(pool, LAUNCH),
        holding(pool, COLLATERAL),
        holding(buyer, LAUNCH),
        holding(buyer, COLLATERAL),
        Actor::new(
            clock_core::CLOCK_PROGRAM_ACCOUNT_ID,
            clock_core::clock_account_id(),
        ),
    ];
    let message = Message::try_new(
        Actor::new(buyer, CURVE),
        actors,
        BTreeMap::from([(buyer, state.get_account_by_id(buyer).nonce)]),
        CurveMessage::Execute {
            pool,
            instruction: Box::new(CurveMessage::SwapExactOutput {
                membership: None,
                amount_out: 1_000,
                max_amount_in,
                token_in: COLLATERAL,
                now,
            }),
        },
    )
    .unwrap();
    let witnesses = WitnessSet::for_message(&message, &[key]);
    PublicTransaction::new(message, witnesses)
}

pub(super) fn buy(
    state: &mut V03State,
    pool: AccountId,
    key: &PrivateKey,
    max_amount_in: u128,
) -> Result<(), lee::error::LeeError> {
    let transaction = buy_transaction(state, pool, key, max_amount_in, 1);
    state
        .transition_from_public_transaction(&transaction, 1, 0)
        .map(|_| ())
}

pub(super) fn curve_factory_env(
    hook: bool,
    configuration: Vec<u8>,
    policy_terms: factory_core::GraduationPolicy,
) -> FixedFactoryEnv {
    curve_factory_env_with_adapter(
        hook,
        configuration,
        policy_terms,
        launchpad_methods::GRADUATION_FAULT_ADAPTER_ELF,
    )
}

fn curve_factory_env_with_adapter(
    hook: bool,
    configuration: Vec<u8>,
    policy_terms: factory_core::GraduationPolicy,
    adapter_elf: &'static [u8],
) -> FixedFactoryEnv {
    curve_factory_env_with_reserve(hook, configuration, policy_terms, adapter_elf, 0)
}

pub(super) fn curve_factory_env_with_reserve(
    hook: bool,
    configuration: Vec<u8>,
    policy_terms: factory_core::GraduationPolicy,
    adapter_elf: &'static [u8],
    graduation_reserve: u128,
) -> FixedFactoryEnv {
    const FACTORY: AccountId = AccountId::new([17; 32]);
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
    let salt = [24; 32];
    let factory = factory_core::compute_factory_pda(NAMESPACE, FACTORY, salt);
    let launch = factory_core::compute_definition_pda(NAMESPACE, FACTORY, salt);
    let mint = factory_core::compute_mint_pda(NAMESPACE, FACTORY, salt);
    let escrow = factory_core::compute_escrow_pda(NAMESPACE, FACTORY, salt);
    let role = factory_core::compute_curve_creator_role_pda(NAMESPACE, FACTORY, salt);
    let pool = curve_core::compute_pool_pda(NAMESPACE, CURVE, launch, COLLATERAL, factory);
    let (policy, path) = allowlist_for(pool, creator);
    let mut state = fixed_config(
        V03State::new()
            .with_programs_at(
                ([
                    (
                        FACTORY,
                        Program::new(Cow::Borrowed(launchpad_methods::FACTORY_ELF)).unwrap(),
                    ),
                    (
                        CURVE,
                        Program::new(Cow::Borrowed(launchpad_methods::CURVE_ELF)).unwrap(),
                    ),
                    (
                        TOKEN_PROGRAM_ACCOUNT_ID,
                        Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_TOKEN_ELF)).unwrap(),
                    ),
                    (
                        factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
                        Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_AUTHORITY_ELF))
                            .unwrap(),
                    ),
                ])
                .map(|(address, program)| (address, program, true)),
            )
            .with_public_accounts([
                account(
                    Actor::new(COLLATERAL, TOKEN_PROGRAM_ACCOUNT_ID),
                    borsh::to_vec(&TokenState::Definition {
                        name: "Collateral".into(),
                        kind: TokenKind::Fungible,
                        supply: 10_000,
                        metadata: None,
                    })
                    .unwrap(),
                ),
                account(
                    holding(creator, COLLATERAL),
                    borsh::to_vec(&TokenState::Holding {
                        owner: creator,
                        definition: COLLATERAL,
                        kind: TokenKind::Fungible,
                        balance: 1_000,
                    })
                    .unwrap(),
                ),
            ]),
        0,
        TREASURY,
        false,
    );
    let mut actors = vec![
        Actor::new(factory, FACTORY),
        Actor::new(pool, CURVE),
        Actor::new(role, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
        Actor::new(creator, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
        Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
        Actor::new(launch, TOKEN_PROGRAM_ACCOUNT_ID),
        Actor::new(COLLATERAL, TOKEN_PROGRAM_ACCOUNT_ID),
    ];
    for owner in [factory, mint, escrow, pool, creator, role, launch] {
        actors.extend([
            Actor::new(owner, FACTORY),
            Actor::new(owner, CURVE),
            Actor::new(owner, TOKEN_PROGRAM_ACCOUNT_ID),
            holding(owner, launch),
            holding(owner, COLLATERAL),
        ]);
    }
    let execute = |state: &mut V03State, instruction: factory_core::Message| {
        let mut message = Message::try_new(
            Actor::new(factory, FACTORY),
            actors.clone(),
            BTreeMap::from([(creator, state.get_account_by_id(creator).nonce)]),
            instruction,
        )
        .unwrap();
        message.admission_evidence = vec![
            lee::PublicAccountEvidence::Pda {
                program: FACTORY,
                seed: factory_core::compute_factory_seed(NAMESPACE, salt),
            },
            lee::PublicAccountEvidence::Pda {
                program: CURVE,
                seed: curve_core::compute_pool_pda_seed(NAMESPACE, launch, COLLATERAL, factory),
            },
        ];
        for owner in [factory, mint, escrow, pool, creator] {
            for definition in [launch, COLLATERAL] {
                message
                    .admission_evidence
                    .push(lee::PublicAccountEvidence::Pda {
                        program: TOKEN_PROGRAM_ACCOUNT_ID,
                        seed: token::holding_seed(owner, definition),
                    });
            }
        }
        let witnesses = WitnessSet::for_message(&message, &[&key]);
        state
            .transition_from_public_transaction(&PublicTransaction::new(message, witnesses), 1, 0)
            .unwrap();
    };
    execute(
        &mut state,
        factory_core::Message::CreateFactoryPool {
            buy_policy: policy.clone(),
            pause_policy: curve_core::PausePolicy::Disabled,
            ending_policy: curve_core::EndingPolicy::Scheduled,
            trade_mode: 0,
            sell_fee_bps: 0,
            per_block_token_limit: None,
            graduation_hook_program_id: hook.then_some(LBP_HOOK_PROGRAM),
            graduation_hook_configuration: configuration,
            namespace: NAMESPACE,
            launch_salt: salt,
            name: "Curve".into(),
            uri: "https://example.test/curve".into(),
            graduation_reserve,
            sale_reserve: 4_000,
            graduation_policy: policy_terms,
            creator_allocation: 200,
            virtual_token_reserve: 10_000,
            virtual_collateral_reserve: 1_000,
            end_timestamp: Some(300),
            curve_program_id: CURVE,
            collateral_definition_id: COLLATERAL,
            creator,
            now: 0,
        },
    );
    let stored = factory_core::FactoryState::decode(
        state.get_account_by_id(factory).data.actor_state(FACTORY),
    )
    .unwrap();
    assert_eq!(stored.creation_stage, factory_core::CreationStage::Active);
    assert_eq!(stored.buy_policy, policy);
    assert_eq!(stored.ending_policy, curve_core::EndingPolicy::Scheduled);
    assert_eq!(stored.sell_fee_bps, 0u16);
    actors.extend([
        clock_core::clock_actor(),
        holding(TREASURY, COLLATERAL),
        holding(NEXT_TREASURY, COLLATERAL),
    ]);
    state = clock_state(state, 100_000)
        .with_programs_at(
            ([(
                LBP_HOOK_PROGRAM,
                Program::new(Cow::Borrowed(adapter_elf)).unwrap(),
            )])
            .map(|(address, program)| (address, program, true)),
        )
        .with_public_accounts([
            token_holding(TREASURY, COLLATERAL, 0),
            token_holding(NEXT_TREASURY, COLLATERAL, 0),
        ]);
    let mut env = FixedFactoryEnv {
        state,
        factory,
        pool,
        launch,
        escrow,
        key,
        actors,
        salt,
        path,
    };
    if hook {
        initialize_lbp_adapter(&mut env, factory_core::GraduationIntentKind::Curve);
    }
    let creator = env.creator();
    let mut message = Message::try_new(
        Actor::new(creator, CURVE),
        env.actors.clone(),
        BTreeMap::from([(creator, env.state.get_account_by_id(creator).nonce)]),
        CurveMessage::Execute {
            pool,
            instruction: Box::new(CurveMessage::SwapExactOutput {
                membership: Some(env.path.clone()),
                amount_out: 1_000,
                max_amount_in: 112,
                token_in: COLLATERAL,
                now: 100,
            }),
        },
    )
    .unwrap();
    message
        .admission_evidence
        .push(lee::PublicAccountEvidence::Pda {
            program: TOKEN_PROGRAM_ACCOUNT_ID,
            seed: token::holding_seed(creator, launch),
        });
    let witnesses = WitnessSet::for_message(&message, &[&env.key]);
    env.state
        .transition_from_public_transaction(&PublicTransaction::new(message, witnesses), 1, 100_000)
        .unwrap();
    env.state = clock_state(env.state, 300_000);
    env
}

pub(super) fn saved_curve_factory(env: &FixedFactoryEnv) -> factory_core::FactoryState {
    factory_core::FactoryState::decode(
        env.state
            .get_account_by_id(env.factory)
            .data
            .actor_state(LBP_FACTORY_PROGRAM),
    )
    .unwrap()
}

pub(super) fn curve_factory_setup_case(
    ending_policy: curve_core::EndingPolicy,
    sell_fee_bps: u16,
    buy_after_setup: bool,
) {
    const FACTORY: AccountId = AccountId::new([17; 32]);
    let key = PrivateKey::try_new([21; 32]).unwrap();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
    let salt = [24; 32];
    let factory = factory_core::compute_factory_pda(NAMESPACE, FACTORY, salt);
    let launch = factory_core::compute_definition_pda(NAMESPACE, FACTORY, salt);
    let mint = factory_core::compute_mint_pda(NAMESPACE, FACTORY, salt);
    let escrow = factory_core::compute_escrow_pda(NAMESPACE, FACTORY, salt);
    let role = factory_core::compute_curve_creator_role_pda(NAMESPACE, FACTORY, salt);
    let pool = curve_core::compute_pool_pda(NAMESPACE, CURVE, launch, COLLATERAL, factory);
    let (policy, path) = allowlist_for(pool, creator);
    let mut state = fixed_config(
        V03State::new()
            .with_programs_at(
                [
                    (
                        FACTORY,
                        Program::new(Cow::Borrowed(launchpad_methods::FACTORY_ELF)).unwrap(),
                    ),
                    (
                        CURVE,
                        Program::new(Cow::Borrowed(launchpad_methods::CURVE_ELF)).unwrap(),
                    ),
                    (
                        TOKEN_PROGRAM_ACCOUNT_ID,
                        Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_TOKEN_ELF)).unwrap(),
                    ),
                    (
                        factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
                        Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_AUTHORITY_ELF))
                            .unwrap(),
                    ),
                ]
                .map(|(address, program)| (address, program, true)),
            )
            .with_public_accounts([
                account(
                    Actor::new(COLLATERAL, TOKEN_PROGRAM_ACCOUNT_ID),
                    borsh::to_vec(&TokenState::Definition {
                        name: "Collateral".into(),
                        kind: TokenKind::Fungible,
                        supply: 10_000,
                        metadata: None,
                    })
                    .unwrap(),
                ),
                account(
                    holding(creator, COLLATERAL),
                    borsh::to_vec(&TokenState::Holding {
                        owner: creator,
                        definition: COLLATERAL,
                        kind: TokenKind::Fungible,
                        balance: 1_000,
                    })
                    .unwrap(),
                ),
            ]),
        0,
        TREASURY,
        false,
    );
    let mut actors = vec![
        Actor::new(factory, FACTORY),
        Actor::new(pool, CURVE),
        Actor::new(role, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
        Actor::new(creator, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
        Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
        Actor::new(launch, TOKEN_PROGRAM_ACCOUNT_ID),
        Actor::new(COLLATERAL, TOKEN_PROGRAM_ACCOUNT_ID),
    ];
    for owner in [factory, mint, escrow, pool, creator, role, launch] {
        actors.extend([
            Actor::new(owner, FACTORY),
            Actor::new(owner, CURVE),
            Actor::new(owner, TOKEN_PROGRAM_ACCOUNT_ID),
            holding(owner, launch),
            holding(owner, COLLATERAL),
        ]);
    }
    let mut admission_evidence = vec![
        lee::PublicAccountEvidence::Pda {
            program: FACTORY,
            seed: factory_core::compute_factory_seed(NAMESPACE, salt),
        },
        lee::PublicAccountEvidence::Pda {
            program: CURVE,
            seed: curve_core::compute_pool_pda_seed(NAMESPACE, launch, COLLATERAL, factory),
        },
    ];
    for owner in [factory, mint, escrow, pool, creator] {
        for definition in [launch, COLLATERAL] {
            admission_evidence.push(lee::PublicAccountEvidence::Pda {
                program: TOKEN_PROGRAM_ACCOUNT_ID,
                seed: token::holding_seed(owner, definition),
            });
        }
    }
    let sale_reserve = 4_000;
    let (graduation_reserve, creator_allocation) = (100, 200);
    let create = factory_core::Message::CreateFactoryPool {
        buy_policy: policy.clone(),
        pause_policy: curve_core::PausePolicy::Disabled,
        ending_policy,
        trade_mode: u8::from(sell_fee_bps > 0),
        sell_fee_bps,
        per_block_token_limit: None,
        graduation_hook_program_id: None,
        graduation_hook_configuration: vec![],
        namespace: NAMESPACE,
        launch_salt: salt,
        name: "Curve".into(),
        uri: "https://example.test/curve".into(),
        graduation_reserve,
        sale_reserve,
        graduation_policy: factory_core::GraduationPolicy::default(),
        creator_allocation,
        virtual_token_reserve: 10_000,
        virtual_collateral_reserve: 1_000,
        end_timestamp: if ending_policy == curve_core::EndingPolicy::Depletion {
            None
        } else {
            Some(400)
        },
        curve_program_id: CURVE,
        collateral_definition_id: COLLATERAL,
        creator,
        now: 1,
    };
    let factory_actor = Actor::new(factory, FACTORY);
    const CYCLE_BUDGET: u64 = 10_000_000;
    let nonce = state.get_account_by_id(creator).nonce;
    let mut message = Message::try_new_with_fees(
        factory_actor,
        actors.clone(),
        BTreeMap::from([(creator, nonce)]),
        create,
        lee::FeeDeclaration::new(creator, CYCLE_BUDGET, 0, 100_000_000),
    )
    .unwrap();
    message.admission_evidence = admission_evidence;
    let witnesses = WitnessSet::for_message(&message, &[&key]);
    let transaction = PublicTransaction::new(message, witnesses);
    let (charge, result) = lee::ValidatedStateDiff::from_public_transaction_metered(
        &transaction,
        &state,
        1,
        1_000,
        CYCLE_BUDGET,
    );
    assert!(
        charge.revert.is_none(),
        "setup reverted after {} cycles: {:?}",
        charge.cycles,
        charge.revert
    );
    state.apply_state_diff(result.unwrap()).unwrap();
    let factory_state = factory_core::FactoryState::decode(
        state.get_account_by_id(factory).data.actor_state(FACTORY),
    )
    .unwrap();
    assert_eq!(
        factory_state.creation_stage,
        factory_core::CreationStage::Active
    );
    assert_eq!(factory_state.creator_role_id, role);
    assert_eq!(factory_state.buy_policy, policy);
    let stored =
        PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(CURVE)).unwrap();
    assert!(stored.funded);
    assert_eq!(stored.pool.lifecycle, pool::PoolLifecycle::Open);
    assert_eq!(stored.pool.real_reserve0, sale_reserve);
    assert_eq!(balance(&state, pool, launch), sale_reserve);
    assert_eq!(balance(&state, pool, COLLATERAL), 0);
    assert_eq!(balance(&state, factory, launch), 0);
    assert_eq!(balance(&state, mint, launch), graduation_reserve);
    assert_eq!(balance(&state, escrow, launch), creator_allocation);
    assert_eq!(
        token_supply(&state, launch),
        sale_reserve + graduation_reserve + creator_allocation
    );
    assert_eq!(state.get_account_by_id(creator).nonce.0, nonce.0 + 1);
    eprintln!(
        "single public curve setup: {} cycles, {} cycles left, {} signed transaction bytes (without LEZ envelope tag)",
        charge.cycles,
        CYCLE_BUDGET - charge.cycles,
        borsh::object_length(&transaction).unwrap()
    );
    assert_eq!(stored.pool.ending_policy, ending_policy);
    assert_eq!(stored.pool.sell_fee_bps, sell_fee_bps);
    if !buy_after_setup {
        return;
    }
    let mut message = Message::try_new(
        Actor::new(creator, CURVE),
        actors,
        BTreeMap::from([(creator, state.get_account_by_id(creator).nonce)]),
        CurveMessage::Execute {
            pool,
            instruction: Box::new(CurveMessage::SwapExactInput {
                amount_in: 50,
                min_amount_out: 1,
                token_in: COLLATERAL,
                now: 1,
                membership: Some(path),
            }),
        },
    )
    .unwrap();
    message
        .admission_evidence
        .push(lee::PublicAccountEvidence::Pda {
            program: TOKEN_PROGRAM_ACCOUNT_ID,
            seed: token::holding_seed(creator, launch),
        });
    let witnesses = WitnessSet::for_message(&message, &[&key]);
    let transaction = PublicTransaction::new(message, witnesses);
    let (diff, charge) = lee::ValidatedStateDiff::from_public_transaction_with_cycle_budget(
        &transaction,
        &state,
        1,
        0,
        10_000_000,
    )
    .unwrap();
    eprintln!("created curve allowlist buy cycles: {}", charge.cycles);
    state.apply_state_diff(diff).unwrap();
    assert_eq!(balance(&state, creator, COLLATERAL), 950);
    assert!(balance(&state, creator, launch) > 0);
    assert_eq!(balance(&state, pool, COLLATERAL), 50);
}
