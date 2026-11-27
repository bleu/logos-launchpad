use super::*;

#[test]
fn curve_factory_single_public_setup_metered() {
    curve_factory_setup_case(curve_core::EndingPolicy::Either, 777, false);
}

#[test]
fn weighted_factory_single_public_setup_rolls_back_failed_funding() {
    use factory_core::{
        CreatorAllocationRelease, LbpCreationStage, LbpLaunchState, WeightedInputs,
    };
    const FACTORY: AccountId = AccountId::new([17; 32]);
    for (release, ending_policy) in [
        (
            CreatorAllocationRelease::AfterEnd,
            lbp_core::EndingPolicy::Scheduled,
        ),
        (
            CreatorAllocationRelease::AfterSetup,
            lbp_core::EndingPolicy::Scheduled,
        ),
        (
            CreatorAllocationRelease::AfterEnd,
            lbp_core::EndingPolicy::Either,
        ),
    ] {
        let key = PrivateKey::try_new([23; 32]).unwrap();
        let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
        let salt = [24; 32];
        let factory = factory_core::compute_factory_pda(NAMESPACE, FACTORY, salt);
        let launch = factory_core::compute_definition_pda(NAMESPACE, FACTORY, salt);
        let mint = factory_core::compute_mint_pda(NAMESPACE, FACTORY, salt);
        let escrow = factory_core::compute_escrow_pda(NAMESPACE, FACTORY, salt);
        let role = factory_core::compute_creator_role_pda(NAMESPACE, FACTORY, salt);
        let pool = lbp_core::compute_pool_pda(NAMESPACE, LBP, launch, COLLATERAL, factory);
        let (policy, path) = allowlist_for(pool, creator);
        let authority = factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID;
        let mut state = V03State::new()
            .with_programs_at(
                [
                    (
                        FACTORY,
                        Program::new(Cow::Borrowed(launchpad_methods::FACTORY_ELF)).unwrap(),
                    ),
                    (
                        LBP,
                        Program::new(Cow::Borrowed(launchpad_methods::LBP_ELF)).unwrap(),
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
                        authority,
                        Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_AUTHORITY_ELF))
                            .unwrap(),
                    ),
                    (clock_core::clock_account_id(), programs::clock()),
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
                        balance: 99,
                    })
                    .unwrap(),
                ),
            ]);
        state = fixed_config(clock_state(state, 100_000), 100, TREASURY, false);
        let mut actors = vec![
            Actor::new(factory, FACTORY),
            Actor::new(pool, LBP),
            Actor::new(role, authority),
            Actor::new(creator, authority),
            Actor::new(launch, TOKEN_PROGRAM_ACCOUNT_ID),
            Actor::new(COLLATERAL, TOKEN_PROGRAM_ACCOUNT_ID),
            Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
            Actor::new(
                clock_core::CLOCK_PROGRAM_ACCOUNT_ID,
                clock_core::clock_account_id(),
            ),
        ];
        for owner in [factory, mint, escrow, pool, creator, role, launch] {
            actors.push(Actor::new(owner, FACTORY));
            actors.push(Actor::new(owner, TOKEN_PROGRAM_ACCOUNT_ID));
            actors.push(holding(owner, launch));
            actors.push(holding(owner, COLLATERAL));
        }
        let signed_create = |state: &V03State, instruction| {
            factory_transaction(
                state,
                factory,
                &actors,
                &key,
                salt,
                launch,
                pool,
                instruction,
            )
        };
        let execute_at = |state: &mut V03State, instruction, timestamp| {
            let transaction = signed_create(state, instruction);
            state.transition_from_public_transaction(&transaction, 1, timestamp)
        };
        let creation = factory_core::Message::CreateWeightedLbp {
            inputs: WeightedInputs {
                per_block_token_limit: Some(800),
                buy_policy: policy.clone(),
                pause_policy: curve_core::PausePolicy::AuthorityControlled,
                ending_policy,
                trade_mode: lbp_core::TradeMode::BuyOnly,
                sell_fee_bps: 0,
                graduation_policy: factory_core::GraduationPolicy::default(),
                graduation_hook: None,
                namespace: NAMESPACE,
                launch_salt: salt,
                name: "Weighted".into(),
                uri: "https://example.test/weighted".into(),
                graduation_reserve: 50,
                sale_inventory: 800,
                creator_allocation: 200,
                collateral_seed: 100,
                token_weight_start: 95 * lbp_core::WEIGHT_TOTAL / 100 + 1,
                token_weight_end: lbp_core::WEIGHT_TOTAL / 2,
                start_timestamp: 150,
                end_timestamp: 300,
                allocation_release: release,
                namespace_program_id: CURVE,
                lbp_program_id: LBP,
                collateral_definition_id: COLLATERAL,
                creator,
            },
        };
        let funded = state
            .clone()
            .with_public_accounts([token_holding(creator, COLLATERAL, 100)]);
        for timestamp in [150_001, 299_999, 300_000] {
            let mut rejected = funded.clone();
            assert!(matches!(
                execute_at(&mut rejected, creation.clone(), timestamp),
                Err(lee::error::LeeError::OutOfValidityWindow)
            ));
            assert!(rejected == funded, "late creation changed state or nonce");
        }
        // The saved-clock check also runs when inclusion alone would permit setup.
        let mut future_clock = clock_state(state.clone(), 150_001);
        let unchanged = future_clock.clone();
        assert!(
            matches!(execute_at(&mut future_clock, creation.clone(), 150_000),
            Err(lee::error::LeeError::ProgramExecutionFailed(message)) if message.contains("InvalidTerms"))
        );
        assert!(
            future_clock == unchanged,
            "rejected clock changed creation effects or nonce"
        );
        let nonce = state.get_account_by_id(creator).nonce;
        let transaction = signed_create(&state, creation.clone());
        let (charge, diff) = lee::ValidatedStateDiff::from_public_transaction_metered(
            &transaction,
            &state,
            1,
            100_000,
            10_000_000,
        );
        assert!(
            matches!(
                &charge.revert,
                Some(message) if message.contains("InvalidCustody")
            ),
            "expected insufficient creator seed, got {:?}",
            charge.revert
        );
        let mut signer = state.get_account_by_id(creator);
        signer.nonce.0 += 1;
        let expected = state.clone().with_public_accounts([(creator, signer)]);
        assert!(state.apply_state_diff(diff.unwrap()).unwrap().is_empty());
        assert!(
            state == expected,
            "failed creation committed application changes"
        );

        // A donation cannot replace any part of the creator's seed.
        let mut donated = state
            .clone()
            .with_public_accounts([token_holding(pool, COLLATERAL, 100)]);
        let before = donated.clone();
        assert!(matches!(
            execute_at(&mut donated, creation.clone(), 100_000),
            Err(lee::error::LeeError::ProgramExecutionFailed(message)) if message.contains("InvalidCustody")
        ));
        assert!(donated == before);

        state = state.with_public_accounts([token_holding(creator, COLLATERAL, 100)]);
        if release == CreatorAllocationRelease::AfterSetup {
            let mut rejected =
                state
                    .clone()
                    .with_public_accounts([token_holding(creator, launch, u128::MAX)]);
            let unchanged = rejected.clone();
            assert!(execute_at(&mut rejected, creation.clone(), 100_000).is_err());
            assert!(
                rejected == unchanged,
                "failed allocation credit changed creation effects"
            );
        }
        state = clock_state(state, 150_000);
        let transaction = signed_create(&state, creation);
        let (diff, charge) = lee::ValidatedStateDiff::from_public_transaction_with_cycle_budget(
            &transaction,
            &state,
            1,
            150_000,
            10_000_000,
        )
        .unwrap();
        state.apply_state_diff(diff).unwrap();
        assert_eq!(state.get_account_by_id(creator).nonce.0, nonce.0 + 2);
        eprintln!(
            "single public weighted setup: {} cycles, {} signed transaction bytes",
            charge.cycles,
            borsh::object_length(&transaction).unwrap()
        );
        let launch_state =
            LbpLaunchState::decode(state.get_account_by_id(factory).data.actor_state(FACTORY))
                .unwrap();
        assert_eq!(launch_state.creation_stage, LbpCreationStage::Complete);
        assert_eq!(launch_state.buy_policy, policy);
        assert_eq!(launch_state.ending_policy, ending_policy);
        assert_eq!(launch_state.per_block_token_limit, Some(800));
        assert_eq!(balance(&state, pool, launch), 800);
        assert_eq!(balance(&state, pool, COLLATERAL), 100);
        assert_eq!(balance(&state, mint, launch), 50);
        assert_eq!(balance(&state, creator, COLLATERAL), 0);
        if release == CreatorAllocationRelease::AfterSetup {
            assert_eq!(balance(&state, creator, launch), 200);
        } else {
            assert!(
                state
                    .get_account_by_id(holding(creator, launch).account_id)
                    .data
                    .actor_state(TOKEN_PROGRAM_ACCOUNT_ID)
                    .is_empty()
            );
        }
        assert_eq!(
            balance(&state, escrow, launch),
            if release == CreatorAllocationRelease::AfterEnd {
                200
            } else {
                0
            }
        );
        let sale = fixed_pool(&state, pool);
        assert!(sale.funded);
        assert_eq!(sale.buy_policy, policy);
        assert_eq!(sale.ending_policy, ending_policy);
        assert_eq!(sale.block_allocation.limit, Some(800));
        assert!(matches!(
            sale.mode,
            lbp_core::PoolMode::Weighted {
                pricing_collateral: 100,
                ..
            }
        ));
        state = clock_state(state, 150_000).with_public_accounts([account(
            holding(creator, COLLATERAL),
            borsh::to_vec(&TokenState::Holding {
                owner: creator,
                definition: COLLATERAL,
                kind: TokenKind::Fungible,
                balance: 1000,
            })
            .unwrap(),
        )]);
        let transaction = fixed_transaction(
            &state,
            pool,
            &key,
            lbp_core::Message::BuyWeightedExactInput {
                membership: Some(path.clone()),
                amount_in: 50,
                min_amount_out: 1,
            },
        );
        let (diff, charge) = lee::ValidatedStateDiff::from_public_transaction_with_cycle_budget(
            &transaction,
            &state,
            2,
            150_000,
            10_000_000,
        )
        .unwrap();
        eprintln!(
            "created weighted allowlist exact-input cycles (upstream clock): {}",
            charge.cycles
        );
        state.apply_state_diff(diff).unwrap();
        let remaining_before_output = balance(&state, pool, launch);
        let creator_before_output = balance(&state, creator, launch);
        assert!(remaining_before_output < 800);
        assert_eq!(balance(&state, creator, COLLATERAL), 950);
        assert_eq!(fixed_pool(&state, pool).buy_policy, policy);
        let transaction = fixed_transaction(
            &state,
            pool,
            &key,
            lbp_core::Message::BuyWeightedExactOutput {
                membership: Some(path),
                amount_out: 10,
                max_amount_in: 100,
            },
        );
        let (diff, charge) = lee::ValidatedStateDiff::from_public_transaction_with_cycle_budget(
            &transaction,
            &state,
            2,
            150_000,
            10_000_000,
        )
        .unwrap();
        eprintln!(
            "created weighted allowlist buy cycles (upstream clock): {}",
            charge.cycles
        );
        state.apply_state_diff(diff).unwrap();
        assert_eq!(balance(&state, pool, launch), remaining_before_output - 10);
        assert_eq!(balance(&state, creator, launch), creator_before_output + 10);
    }
}

#[test]
fn factory_creation_rejects_invalid_ending_matrix_without_state_changes() {
    use curve_core::{EndingPolicy, PausePolicy};
    const FACTORY: AccountId = AccountId::new([17; 32]);
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
    let salt = [24; 32];
    let factory = factory_core::compute_factory_pda(NAMESPACE, FACTORY, salt);
    let launch = factory_core::compute_definition_pda(NAMESPACE, FACTORY, salt);
    let mint = factory_core::compute_mint_pda(NAMESPACE, FACTORY, salt);
    let escrow = factory_core::compute_escrow_pda(NAMESPACE, FACTORY, salt);
    let authority = factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID;
    let mut state = fixed_config(
        clock_state(
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
                            LBP,
                            Program::new(Cow::Borrowed(launchpad_methods::LBP_ELF)).unwrap(),
                        ),
                        (
                            TOKEN_PROGRAM_ACCOUNT_ID,
                            Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_TOKEN_ELF))
                                .unwrap(),
                        ),
                        (
                            authority,
                            Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_AUTHORITY_ELF))
                                .unwrap(),
                        ),
                        (clock_core::clock_account_id(), programs::clock()),
                    ]
                    .map(|(address, program)| (address, program, true)),
                )
                .with_public_accounts([
                    account(
                        Actor::new(COLLATERAL, TOKEN_PROGRAM_ACCOUNT_ID),
                        borsh::to_vec(&TokenState::Definition {
                            name: "Collateral".into(),
                            kind: TokenKind::Fungible,
                            supply: 1_000,
                            metadata: None,
                        })
                        .unwrap(),
                    ),
                    token_holding(creator, COLLATERAL, 1_000),
                ]),
            100_000,
        ),
        333,
        TREASURY,
        false,
    );
    for engine in [CURVE, LBP] {
        let pool = curve_core::compute_pool_pda(NAMESPACE, engine, launch, COLLATERAL, factory);
        let mut actors = vec![
            Actor::new(factory, FACTORY),
            Actor::new(pool, engine),
            Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
            Actor::new(COLLATERAL, TOKEN_PROGRAM_ACCOUNT_ID),
            Actor::new(launch, TOKEN_PROGRAM_ACCOUNT_ID),
            Actor::new(creator, authority),
            Actor::new(
                factory_core::compute_creator_role_pda(NAMESPACE, FACTORY, salt),
                authority,
            ),
            Actor::new(
                factory_core::compute_curve_creator_role_pda(NAMESPACE, FACTORY, salt),
                authority,
            ),
            clock_core::clock_actor(),
        ];
        for owner in [factory, mint, escrow, pool, creator, launch] {
            actors.extend([
                Actor::new(owner, FACTORY),
                Actor::new(owner, engine),
                Actor::new(owner, TOKEN_PROGRAM_ACCOUNT_ID),
                holding(owner, launch),
                holding(owner, COLLATERAL),
            ]);
        }
        let mut commands = Vec::new();
        if engine == CURVE {
            for (ending_policy, end_timestamp) in [
                (EndingPolicy::Manual, None),
                (EndingPolicy::Depletion, Some(200)),
                (EndingPolicy::Scheduled, None),
                (EndingPolicy::Either, None),
            ] {
                commands.push(factory_core::Message::CreateFactoryPool {
                    buy_policy: curve_core::BuyPolicy::Open,
                    pause_policy: PausePolicy::Disabled,
                    ending_policy,
                    trade_mode: 0,
                    sell_fee_bps: 0,
                    per_block_token_limit: None,
                    graduation_hook_program_id: None,
                    graduation_hook_configuration: vec![],
                    namespace: NAMESPACE,
                    launch_salt: salt,
                    name: "Curve".into(),
                    uri: "https://example.test/curve".into(),
                    graduation_reserve: 0,
                    sale_reserve: 800,
                    graduation_policy: factory_core::GraduationPolicy::default(),
                    creator_allocation: 200,
                    virtual_token_reserve: 10_000,
                    virtual_collateral_reserve: 1_000,
                    end_timestamp,
                    curve_program_id: CURVE,
                    collateral_definition_id: COLLATERAL,
                    creator,
                    now: 100,
                });
            }
        } else {
            for (ending_policy, end_timestamp) in [
                (EndingPolicy::Manual, None),
                (EndingPolicy::Depletion, Some(200)),
                (EndingPolicy::Scheduled, None),
                (EndingPolicy::Either, None),
            ] {
                commands.push(factory_core::Message::CreateFixedPriceLbp {
                    inputs: factory_core::FixedPriceInputs {
                        per_block_token_limit: None,
                        buy_policy: curve_core::BuyPolicy::Open,
                        pause_policy: PausePolicy::Disabled,
                        ending_policy,
                        trade_mode: lbp_core::TradeMode::BuyOnly,
                        sell_fee_bps: 0,
                        graduation_policy: factory_core::GraduationPolicy::default(),
                        graduation_hook: None,
                        namespace: NAMESPACE,
                        launch_salt: salt,
                        name: "Fixed".into(),
                        uri: "https://example.test/fixed".into(),
                        graduation_reserve: 0,
                        sale_inventory: 800,
                        creator_allocation: 200,
                        price: lbp_core::FixedPrice {
                            collateral_units: 3,
                            token_units: 2,
                        },
                        start: factory_core::SaleStart::Now { timestamp: 100 },
                        end_timestamp,
                        allocation_release: factory_core::CreatorAllocationRelease::AfterEnd,
                        namespace_program_id: CURVE,
                        lbp_program_id: LBP,
                        collateral_definition_id: COLLATERAL,
                        creator,
                        now: 100,
                    },
                });
            }
            for ending_policy in [EndingPolicy::Depletion, EndingPolicy::Manual] {
                commands.push(factory_core::Message::CreateWeightedLbp {
                    inputs: factory_core::WeightedInputs {
                        per_block_token_limit: None,
                        buy_policy: curve_core::BuyPolicy::Open,
                        pause_policy: PausePolicy::Disabled,
                        ending_policy,
                        trade_mode: lbp_core::TradeMode::BuyOnly,
                        sell_fee_bps: 0,
                        graduation_policy: factory_core::GraduationPolicy::default(),
                        graduation_hook: None,
                        namespace: NAMESPACE,
                        launch_salt: salt,
                        name: "Weighted".into(),
                        uri: "https://example.test/weighted".into(),
                        graduation_reserve: 0,
                        sale_inventory: 800,
                        creator_allocation: 200,
                        collateral_seed: 100,
                        token_weight_start: lbp_core::WEIGHT_TOTAL / 2,
                        token_weight_end: lbp_core::WEIGHT_TOTAL / 2,
                        start_timestamp: 150,
                        end_timestamp: 300,
                        allocation_release: factory_core::CreatorAllocationRelease::AfterEnd,
                        namespace_program_id: CURVE,
                        lbp_program_id: LBP,
                        collateral_definition_id: COLLATERAL,
                        creator,
                    },
                });
            }
        }
        for command in commands {
            let before = state.clone();
            let nonce = state.get_account_by_id(creator).nonce;
            let mut message = Message::try_new(
                Actor::new(factory, FACTORY),
                actors.clone(),
                BTreeMap::from([(creator, nonce)]),
                command,
            )
            .unwrap();
            message
                .admission_evidence
                .push(lee::PublicAccountEvidence::Pda {
                    program: FACTORY,
                    seed: factory_core::compute_factory_seed(NAMESPACE, salt),
                });
            let witnesses = WitnessSet::for_message(&message, &[&key]);
            let rejected = state.transition_from_public_transaction(
                &PublicTransaction::new(message, witnesses),
                1,
                100_000,
            );
            assert!(
                matches!(&rejected, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("InvalidTerms")),
                "expected policy rejection before creation, got {rejected:?}",
            );
            assert!(
                state == before,
                "invalid policy creation changed runtime state"
            );
            assert_eq!(state.get_account_by_id(creator).nonce, nonce);
        }
    }
}

#[test]
fn curve_short_setup_is_atomic_and_enforces_exclusive_expiry() {
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
    let mut state = fixed_config(
        V03State::new()
            .with_programs_at([
                (
                    FACTORY,
                    Program::new(Cow::Borrowed(launchpad_methods::FACTORY_ELF)).unwrap(),
                    true,
                ),
                (
                    CURVE,
                    Program::new(Cow::Borrowed(launchpad_methods::CURVE_ELF)).unwrap(),
                    true,
                ),
                (
                    TOKEN_PROGRAM_ACCOUNT_ID,
                    Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_TOKEN_ELF)).unwrap(),
                    true,
                ),
                (
                    factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
                    Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_AUTHORITY_ELF)).unwrap(),
                    true,
                ),
            ])
            .with_public_accounts([account(
                Actor::new(COLLATERAL, TOKEN_PROGRAM_ACCOUNT_ID),
                borsh::to_vec(&TokenState::Definition {
                    name: "Collateral".into(),
                    kind: TokenKind::Fungible,
                    supply: 1000,
                    metadata: None,
                })
                .unwrap(),
            )]),
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
    let create = factory_core::Message::CreateFactoryPool {
        buy_policy: curve_core::BuyPolicy::Open,
        pause_policy: curve_core::PausePolicy::Disabled,
        ending_policy: curve_core::EndingPolicy::Scheduled,
        trade_mode: 1,
        sell_fee_bps: 0,
        per_block_token_limit: None,
        graduation_hook_program_id: None,
        graduation_hook_configuration: vec![],
        namespace: NAMESPACE,
        launch_salt: salt,
        name: "Curve".into(),
        uri: "https://example.test/curve".into(),
        sale_reserve: 800,
        graduation_reserve: 0,
        graduation_policy: factory_core::GraduationPolicy::default(),
        creator_allocation: 0,
        virtual_token_reserve: 1000,
        virtual_collateral_reserve: 100,
        end_timestamp: Some(500),
        curve_program_id: CURVE,
        collateral_definition_id: COLLATERAL,
        creator,
        now: 100,
    };
    let before = state.clone();
    let result = factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        create.clone(),
        500_000,
    );
    assert!(matches!(
        result,
        Err(lee::error::LeeError::OutOfValidityWindow)
    ));
    assert!(state == before);
    factory_execute_at(
        &mut state, factory, &actors, &key, salt, launch, pool, create, 499_999,
    )
    .unwrap();
    let stored =
        PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(CURVE)).unwrap();
    assert!(stored.funded);
    assert_eq!(stored.pool.lifecycle, pool::PoolLifecycle::Open);
    assert_eq!(balance(&state, pool, launch), 800);
    let before = state.clone();
    let result = factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::ContinueCreation { creator, now: 100 },
        499_999,
    );
    assert!(
        matches!(result, Err(lee::error::LeeError::ProgramExecutionFailed(message)) if message.contains("Unauthorized"))
    );
    assert!(state == before);
    for _ in 0..3 {
        factory_execute_at(
            &mut state,
            factory,
            &actors,
            &key,
            salt,
            launch,
            pool,
            factory_core::Message::WithdrawFactoryProceeds { creator, now: 500 },
            500_000,
        )
        .unwrap();
    }
    assert_eq!(balance(&state, creator, launch), 800);
}
