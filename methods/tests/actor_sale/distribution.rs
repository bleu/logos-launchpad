use super::*;
use distribution_adapter::{Configuration, Message as DistributionMessage, Rule, State, VERSION};
use lez_compat_core::authority;

const RECIPIENT_A: AccountId = AccountId::new([180; 32]);
const RECIPIENT_B: AccountId = AccountId::new([181; 32]);

fn configuration() -> Configuration {
    Configuration {
        version: VERSION,
        launch_rules: vec![
            Rule {
                destination: RECIPIENT_A,
                bps: 5000,
            },
            Rule {
                destination: RECIPIENT_B,
                bps: 3000,
            },
            Rule {
                destination: RECIPIENT_A,
                bps: 2000,
            },
        ],
        collateral_rules: vec![
            Rule {
                destination: RECIPIENT_B,
                bps: 6000,
            },
            Rule {
                destination: RECIPIENT_A,
                bps: 4000,
            },
        ],
    }
}
fn env(kind: factory_core::GraduationIntentKind, zero: bool) -> FixedFactoryEnv {
    env_configuration(kind, zero, configuration())
}
fn env_configuration(
    kind: factory_core::GraduationIntentKind,
    zero: bool,
    config: Configuration,
) -> FixedFactoryEnv {
    env_configuration_with_reserve(kind, zero, config, 0)
}
fn env_configuration_with_reserve(
    kind: factory_core::GraduationIntentKind,
    zero: bool,
    config: Configuration,
    graduation_reserve: u128,
) -> FixedFactoryEnv {
    let policy = if zero {
        factory_core::GraduationPolicy::default()
    } else {
        test_graduation_policy(true)
    };
    let bytes = borsh::to_vec(&config).unwrap();
    let mut env = if kind == factory_core::GraduationIntentKind::Curve {
        curve_factory_env_with_reserve(
            true,
            bytes,
            policy,
            launchpad_methods::DISTRIBUTION_ADAPTER_ELF,
            graduation_reserve,
        )
    } else {
        lbp_graduation_env_with_reserve(
            kind == factory_core::GraduationIntentKind::Weighted,
            true,
            bytes,
            kind,
            policy,
            launchpad_methods::DISTRIBUTION_ADAPTER_ELF,
            graduation_reserve,
        )
    };
    for (side, rules) in [&config.launch_rules, &config.collateral_rules]
        .into_iter()
        .enumerate()
    {
        for rule in rules {
            env.actors
                .push(holding(rule.destination, [env.launch, COLLATERAL][side]));
        }
    }
    let mut unique = std::collections::HashSet::new();
    env.actors.retain(|actor| unique.insert(*actor));
    env
}
fn adapter_state(env: &FixedFactoryEnv) -> State {
    State::decode(
        env.state
            .get_account_by_id(lbp_adapter(env).account_id)
            .data
            .actor_state(LBP_HOOK_PROGRAM),
    )
    .unwrap()
}
fn call(
    env: &mut FixedFactoryEnv,
    adapter_call: bool,
) -> Result<Vec<lee_core::program::TransactionEvent>, lee::error::LeeError> {
    // An unrelated signer pays. No creator authority is required for either operation.
    let key = PrivateKey::try_new([96; 32]).unwrap();
    let signer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let receiver = if adapter_call {
        lbp_adapter(env)
    } else {
        Actor::new(env.factory, LBP_FACTORY_PROGRAM)
    };
    let instruction = if adapter_call {
        borsh::to_vec(&DistributionMessage::DistributeRemaining).unwrap()
    } else {
        borsh::to_vec(&lbp_hook_message(env.creator())).unwrap()
    };
    let mut message = Message::try_new(
        receiver,
        env.actors.clone(),
        BTreeMap::from([(signer, env.state.get_account_by_id(signer).nonce)]),
        (),
    )
    .unwrap();
    message.execution.root.message = instruction;
    let curve = env.actors.contains(&Actor::new(env.pool, CURVE));
    message.admission_evidence = vec![
        lee::PublicAccountEvidence::Pda {
            program: LBP_FACTORY_PROGRAM,
            seed: factory_core::compute_factory_seed(NAMESPACE, env.salt),
        },
        lee::PublicAccountEvidence::Pda {
            program: LBP_HOOK_PROGRAM,
            seed: factory_core::compute_graduation_seed(env.factory),
        },
        lee::PublicAccountEvidence::Pda {
            program: if curve { CURVE } else { LBP },
            seed: if curve {
                curve_core::compute_pool_pda_seed(NAMESPACE, env.launch, COLLATERAL, env.factory)
            } else {
                lbp_core::compute_pool_pda_seed(NAMESPACE, env.launch, COLLATERAL, env.factory)
            },
        },
    ];
    for owner in [
        env.factory,
        env.pool,
        env.creator(),
        TREASURY,
        NEXT_TREASURY,
        lbp_adapter(env).account_id,
        RECIPIENT_A,
        RECIPIENT_B,
        factory_core::compute_mint_pda(NAMESPACE, LBP_FACTORY_PROGRAM, env.salt),
    ] {
        for definition in [env.launch, COLLATERAL] {
            message
                .admission_evidence
                .push(lee::PublicAccountEvidence::Pda {
                    program: TOKEN_PROGRAM_ACCOUNT_ID,
                    seed: token::holding_seed(owner, definition),
                });
        }
    }
    let configuration = adapter_state(env).configuration;
    for (side, rules) in [&configuration.launch_rules, &configuration.collateral_rules]
        .into_iter()
        .enumerate()
    {
        for rule in rules {
            message
                .admission_evidence
                .push(lee::PublicAccountEvidence::Pda {
                    program: TOKEN_PROGRAM_ACCOUNT_ID,
                    seed: token::holding_seed(rule.destination, [env.launch, COLLATERAL][side]),
                });
        }
    }
    let witnesses = WitnessSet::for_message(&message, &[&key]);
    env.state.transition_from_public_transaction(
        &PublicTransaction::new(message, witnesses),
        1,
        300_000,
    )
}
fn graduate(env: &mut FixedFactoryEnv) {
    for _ in 0..5 {
        call(env, false).unwrap();
        if adapter_state(env).receipt.is_some() {
            return;
        }
    }
    panic!("graduation did not finish");
}
fn donate(env: &mut FixedFactoryEnv, asset: AccountId, amount: u128) {
    let creator = env.creator();
    let custody = lbp_adapter(env).account_id;
    let key = PrivateKey::try_new([23; 32]).unwrap();
    lbp_signed_call(
        env,
        Actor::new(creator, TOKEN_PROGRAM_ACCOUNT_ID),
        token::Message::Transfer {
            definition: asset,
            recipient: custody,
            amount,
        },
        &[&key],
        vec![lee::PublicAccountEvidence::Pda {
            program: TOKEN_PROGRAM_ACCOUNT_ID,
            seed: token::holding_seed(custody, asset),
        }],
    )
    .unwrap();
}
fn maybe_balance(env: &FixedFactoryEnv, owner: AccountId, asset: AccountId) -> u128 {
    let bytes = env
        .state
        .get_account_by_id(holding(owner, asset).account_id)
        .data
        .actor_state(TOKEN_PROGRAM_ACCOUNT_ID)
        .to_vec();
    if bytes.is_empty() {
        0
    } else {
        balance(&env.state, owner, asset)
    }
}

#[test]
fn distribution_full_balances_all_engines_and_permissionless_later_deposits() {
    for kind in [
        factory_core::GraduationIntentKind::Curve,
        factory_core::GraduationIntentKind::FixedPrice,
        factory_core::GraduationIntentKind::Weighted,
    ] {
        let mut env = env(kind, false);
        let before = env.state.clone();
        assert!(call(&mut env, true).is_err());
        assert!(env.state == before);
        let launch = env.launch;
        donate(&mut env, launch, 7);
        donate(&mut env, COLLATERAL, 11);
        let supply = token_supply(&env.state, launch);
        graduate(&mut env);
        let saved = adapter_state(&env);
        let request = saved.receipt.clone().unwrap();
        let (tokens, collateral) = request.intent.amounts();
        assert_eq!(saved.graduation_amounts, [tokens + 7, collateral + 11]);
        assert_eq!(saved.distribution_count, 1);
        assert!(saved.is_idle());
        for asset in [launch, COLLATERAL] {
            assert_eq!(balance(&env.state, lbp_adapter(&env).account_id, asset), 0);
        }
        for payout in configuration().plan(saved.graduation_amounts).unwrap() {
            // Repeated destinations are aggregated below.
            assert!(
                maybe_balance(
                    &env,
                    payout.destination,
                    [launch, COLLATERAL][payout.asset as usize]
                ) >= payout.amount
            );
        }
        for (side, asset) in [launch, COLLATERAL].into_iter().enumerate() {
            for owner in [RECIPIENT_A, RECIPIENT_B] {
                let expected: u128 = configuration()
                    .plan(saved.graduation_amounts)
                    .unwrap()
                    .iter()
                    .filter(|p| p.asset as usize == side && p.destination == owner)
                    .map(|p| p.amount)
                    .sum();
                assert_eq!(maybe_balance(&env, owner, asset), expected);
            }
        }
        assert_eq!(token_supply(&env.state, launch), supply);
        let graduated = env.state.clone();
        assert!(call(&mut env, false).is_err());
        assert!(env.state == graduated);
        donate(&mut env, launch, 101);
        donate(&mut env, COLLATERAL, 9);
        let pool_before = env.state.get_account_by_id(env.pool).clone();
        let factory_before = env.state.get_account_by_id(env.factory).clone();
        let previous = [
            maybe_balance(&env, RECIPIENT_A, launch),
            maybe_balance(&env, RECIPIENT_B, launch),
        ];
        let events = call(&mut env, true).unwrap();
        let event = events
            .iter()
            .find(|event| event.event.selector == distribution_adapter::DISTRIBUTION_EVENT_SELECTOR)
            .unwrap();
        let event: distribution_adapter::DistributionEvent =
            borsh::from_slice(&event.event.data).unwrap();
        assert!(!event.graduation);
        assert_eq!(event.amounts, [101, 9]);
        assert_eq!(maybe_balance(&env, RECIPIENT_A, launch) - previous[0], 71);
        assert_eq!(maybe_balance(&env, RECIPIENT_B, launch) - previous[1], 30);
        assert_eq!(adapter_state(&env).receipt, Some(request));
        assert_eq!(
            adapter_state(&env).graduation_amounts,
            saved.graduation_amounts
        );
        assert_eq!(env.state.get_account_by_id(env.pool), pool_before);
        assert_eq!(env.state.get_account_by_id(env.factory), factory_before);
        call(&mut env, true).unwrap();
        assert_eq!(adapter_state(&env).distribution_count, 3);
        assert_eq!(maybe_balance(&env, RECIPIENT_A, launch) - previous[0], 71);
    }
}

#[test]
fn distribution_zero_requests_include_donations_and_zero_balances_skip_recipient_creation() {
    for donations in [false, true] {
        let mut env = env(factory_core::GraduationIntentKind::FixedPrice, true);
        if donations {
            donate(&mut env, COLLATERAL, 11);
        }
        graduate(&mut env);
        let state = adapter_state(&env);
        assert_eq!(state.receipt.as_ref().unwrap().intent.amounts(), (0, 0));
        assert_eq!(
            state.graduation_amounts,
            [0, if donations { 11 } else { 0 }]
        );
        assert!(
            env.state
                .get_account_by_id(holding(RECIPIENT_A, env.launch).account_id)
                .data
                .actor_state(TOKEN_PROGRAM_ACCOUNT_ID)
                .is_empty()
        );
        assert_eq!(
            maybe_balance(&env, RECIPIENT_A, COLLATERAL),
            if donations { 5 } else { 0 }
        );
    }
}

#[test]
fn distribution_failures_restore_balances_receipts_progress_and_nonce_then_retry() {
    for kind in [
        factory_core::GraduationIntentKind::Curve,
        factory_core::GraduationIntentKind::FixedPrice,
        factory_core::GraduationIntentKind::Weighted,
    ] {
        for callback_fault in [false, true] {
            let mut env = env_configuration_with_reserve(kind, false, configuration(), 120);
            if kind == factory_core::GraduationIntentKind::Curve {
                for _ in 0..3 {
                    call(&mut env, false).unwrap();
                }
            }
            let (program, fault, repaired) = if callback_fault {
                (
                    LBP_HOOK_PROGRAM,
                    launchpad_methods::DISTRIBUTION_FAULT_ADAPTER_ELF,
                    launchpad_methods::DISTRIBUTION_ADAPTER_ELF,
                )
            } else {
                (
                    TOKEN_PROGRAM_ACCOUNT_ID,
                    launchpad_methods::DISTRIBUTION_FAULT_TOKEN_ELF,
                    lez_compat_methods::COMPAT_TOKEN_ELF,
                )
            };
            env.state = env
                .state
                .with_named_programs([(program, Program::new(Cow::Borrowed(fault)).unwrap())]);
            let before = env.state.clone();
            let result = call(&mut env, false);
            assert!(
                result.is_err(),
                "fault did not reject: {kind:?}, callback={callback_fault}"
            );
            assert!(
                env.state == before,
                "failed graduation changed full runtime"
            );
            assert!(adapter_state(&env).receipt.is_none());
            let before = env.state.clone();
            assert!(call(&mut env, true).is_err());
            assert!(env.state == before);
            env.state = env
                .state
                .with_named_programs([(program, Program::new(Cow::Borrowed(repaired)).unwrap())]);
            graduate(&mut env);
            assert_eq!(adapter_state(&env).distribution_count, 1);
            let launch = env.launch;
            donate(&mut env, launch, 101);
            env.state = env.state.with_named_programs([(
                TOKEN_PROGRAM_ACCOUNT_ID,
                Program::new(Cow::Borrowed(
                    launchpad_methods::DISTRIBUTION_FAULT_TOKEN_ELF,
                ))
                .unwrap(),
            )]);
            let before = env.state.clone();
            assert!(call(&mut env, true).is_err());
            assert!(env.state == before);
            assert_eq!(adapter_state(&env).distribution_count, 1);
            env.state = env.state.with_named_programs([(
                TOKEN_PROGRAM_ACCOUNT_ID,
                Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_TOKEN_ELF)).unwrap(),
            )]);
            call(&mut env, true).unwrap();
            assert_eq!(adapter_state(&env).distribution_count, 2);
        }
    }
}

#[test]
fn distribution_eight_rules_per_asset_prepare_fresh_recipients_and_conserve_balances() {
    let rules = |offset: u8| {
        (0u8..8)
            .map(|i| Rule {
                destination: AccountId::new([offset + i; 32]),
                bps: 1250,
            })
            .collect()
    };
    let config = Configuration {
        version: VERSION,
        launch_rules: rules(180),
        collateral_rules: rules(190),
    };
    let mut env = env_configuration(
        factory_core::GraduationIntentKind::FixedPrice,
        false,
        config.clone(),
    );
    graduate(&mut env);
    let state = adapter_state(&env);
    let plan = config.plan(state.graduation_amounts).unwrap();
    assert_eq!(plan.len(), 16);
    for payout in plan {
        assert_eq!(
            maybe_balance(
                &env,
                payout.destination,
                [env.launch, COLLATERAL][payout.asset as usize]
            ),
            payout.amount
        );
    }
}

#[test]
fn distribution_destination_preparation_failure_rolls_back_new_holdings_and_retries() {
    let mut env = env(factory_core::GraduationIntentKind::FixedPrice, false);
    let launch = env.launch;
    env.state = env.state.with_public_accounts([account(
        holding(RECIPIENT_B, launch),
        borsh::to_vec(&TokenState::Holding {
            owner: RECIPIENT_B,
            definition: COLLATERAL,
            kind: TokenKind::Fungible,
            balance: 0,
        })
        .unwrap(),
    )]);
    let before = env.state.clone();
    assert!(call(&mut env, false).is_err());
    assert!(env.state == before);
    assert!(
        env.state
            .get_account_by_id(holding(RECIPIENT_A, launch).account_id)
            .data
            .actor_state(TOKEN_PROGRAM_ACCOUNT_ID)
            .is_empty()
    );
    assert!(adapter_state(&env).receipt.is_none());
    env.state = env
        .state
        .with_public_accounts([token_holding(RECIPIENT_B, launch, 0)]);
    graduate(&mut env);
    assert_eq!(adapter_state(&env).distribution_count, 1);
}

#[test]
fn graduation_reserve_stays_outside_trading_and_pays_only_recorded_amount_all_engines() {
    for kind in [
        factory_core::GraduationIntentKind::Curve,
        factory_core::GraduationIntentKind::FixedPrice,
        factory_core::GraduationIntentKind::Weighted,
    ] {
        let baseline = env(kind, false);
        let mut reserved = env_configuration_with_reserve(kind, false, configuration(), 120);
        let reserve_owner =
            factory_core::compute_mint_pda(NAMESPACE, LBP_FACTORY_PROGRAM, reserved.salt);
        assert_eq!(
            balance(&reserved.state, reserve_owner, reserved.launch),
            120
        );
        if kind == factory_core::GraduationIntentKind::Curve {
            assert_eq!(
                baseline
                    .state
                    .get_account_by_id(baseline.pool)
                    .data
                    .actor_state(CURVE),
                reserved
                    .state
                    .get_account_by_id(reserved.pool)
                    .data
                    .actor_state(CURVE)
            );
        } else {
            assert_eq!(
                fixed_pool(&baseline.state, baseline.pool),
                fixed_pool(&reserved.state, reserved.pool)
            );
        }
        let supply = token_supply(&reserved.state, reserved.launch);
        assert_eq!(supply, token_supply(&baseline.state, baseline.launch) + 120);
        // Curve fixtures buy launch tokens. Release the separate allocation in the
        // other engines before donating.
        if kind != factory_core::GraduationIntentKind::Curve {
            let creator = reserved.creator();
            let factory = reserved.factory;
            let key = PrivateKey::try_new([23; 32]).unwrap();
            lbp_signed_call(
                &mut reserved,
                Actor::new(factory, LBP_FACTORY_PROGRAM),
                lbp_allocation_message(
                    kind == factory_core::GraduationIntentKind::Weighted,
                    creator,
                ),
                &[&key],
                vec![],
            )
            .unwrap();
        }
        let creator = reserved.creator();
        let launch = reserved.launch;
        let key = PrivateKey::try_new([23; 32]).unwrap();
        lbp_signed_call(
            &mut reserved,
            Actor::new(creator, TOKEN_PROGRAM_ACCOUNT_ID),
            token::Message::Transfer {
                definition: launch,
                recipient: reserve_owner,
                amount: 7,
            },
            &[&key],
            vec![lee::PublicAccountEvidence::Pda {
                program: TOKEN_PROGRAM_ACCOUNT_ID,
                seed: token::holding_seed(reserve_owner, launch),
            }],
        )
        .unwrap();
        graduate(&mut reserved);
        let saved = adapter_state(&reserved);
        let request = saved.receipt.unwrap();
        assert_eq!(request.reserve_tokens, 120);
        assert_eq!(
            request.reserve_tokens + request.unsold_tokens,
            request.intent.amounts().0
        );
        assert_eq!(balance(&reserved.state, reserve_owner, launch), 7);
        assert_eq!(token_supply(&reserved.state, launch), supply);
        let recipients = maybe_balance(&reserved, RECIPIENT_A, launch)
            + maybe_balance(&reserved, RECIPIENT_B, launch);
        assert_eq!(recipients, request.intent.amounts().0);
        assert!(call(&mut reserved, false).is_err());
        assert_eq!(balance(&reserved.state, reserve_owner, launch), 7);
    }
}

#[test]
fn graduation_reserve_is_paid_with_zero_adapter_percentages_and_no_collateral() {
    let mut reserved = lbp_graduation_env_with_reserve(
        false,
        false,
        borsh::to_vec(&configuration()).unwrap(),
        factory_core::GraduationIntentKind::FixedPrice,
        factory_core::GraduationPolicy::default(),
        launchpad_methods::DISTRIBUTION_ADAPTER_ELF,
        120,
    );
    for owner in [RECIPIENT_A, RECIPIENT_B] {
        for definition in [reserved.launch, COLLATERAL] {
            reserved.actors.push(holding(owner, definition));
        }
    }
    graduate(&mut reserved);
    let request = adapter_state(&reserved).receipt.unwrap();
    assert_eq!(request.reserve_tokens, 120);
    assert_eq!(request.unsold_tokens, 0);
    assert_eq!(request.intent.amounts(), (120, 0));
    assert_eq!(maybe_balance(&reserved, RECIPIENT_A, reserved.launch), 84);
    assert_eq!(maybe_balance(&reserved, RECIPIENT_B, reserved.launch), 36);
}

#[test]
fn graduation_reserve_manual_settlement_returns_to_creator_once_all_engines() {
    for kind in [
        factory_core::GraduationIntentKind::Curve,
        factory_core::GraduationIntentKind::FixedPrice,
        factory_core::GraduationIntentKind::Weighted,
    ] {
        let mut reserved = match kind {
            factory_core::GraduationIntentKind::Curve => curve_factory_env_with_reserve(
                false,
                vec![],
                factory_core::GraduationPolicy::default(),
                launchpad_methods::GRADUATION_ADAPTER_ELF,
                120,
            ),
            factory_core::GraduationIntentKind::FixedPrice => fixed_factory_env_with_reserve(
                Some(300),
                factory_core::CreatorAllocationRelease::AfterEnd,
                curve_core::PausePolicy::Disabled,
                lbp_core::EndingPolicy::Either,
                None,
                lbp_core::TradeMode::BuyOnly,
                0,
                factory_core::GraduationPolicy::default(),
                120,
            ),
            factory_core::GraduationIntentKind::Weighted => {
                let (state, factory, pool, launch, escrow, key, actors, salt) =
                    weighted_factory_env_with_reserve(
                        false,
                        None,
                        lbp_core::TradeMode::BuyOnly,
                        0,
                        factory_core::GraduationPolicy::default(),
                        120,
                    );
                let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
                let (_, path) = allowlist_for(pool, creator);
                FixedFactoryEnv {
                    state,
                    factory,
                    pool,
                    launch,
                    escrow,
                    key,
                    actors,
                    salt,
                    path,
                }
            }
        };
        reserved.state = clock_state(reserved.state, 300_000);
        let old_creator = reserved.creator();
        let old_key = PrivateKey::try_new([23; 32]).unwrap();
        let next_key = PrivateKey::try_new([95; 32]).unwrap();
        let next_creator = AccountId::from(&PublicKey::new_from_private_key(&next_key));
        let role = if kind == factory_core::GraduationIntentKind::Curve {
            saved_curve_factory(&reserved).creator_role_id
        } else {
            lbp_launch(&reserved).creator_role_id
        };
        reserved.actors.extend([
            Actor::new(next_creator, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
            Actor::new(next_creator, TOKEN_PROGRAM_ACCOUNT_ID),
            holding(next_creator, reserved.launch),
            holding(next_creator, COLLATERAL),
        ]);
        lbp_signed_call(
            &mut reserved,
            Actor::new(old_creator, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
            authority::Message::Transfer {
                role,
                successor: next_creator,
            },
            &[&old_key, &next_key],
            vec![],
        )
        .unwrap();
        let before_rejected = reserved.state.clone();
        assert!(
            reserved
                .execute(
                    lbp_manual_message(
                        kind == factory_core::GraduationIntentKind::Weighted,
                        old_creator
                    ),
                    300_000
                )
                .is_err()
        );
        assert!(reserved.state == before_rejected);
        reserved.key = next_key;
        let creator = reserved.creator();
        let launch = reserved.launch;
        let custody = factory_core::compute_mint_pda(NAMESPACE, LBP_FACTORY_PROGRAM, reserved.salt);
        let before = maybe_balance(&reserved, creator, launch);
        let inventory = if kind == factory_core::GraduationIntentKind::Curve {
            3000
        } else {
            800
        };
        for _ in 0..5 {
            reserved
                .execute(
                    lbp_manual_message(
                        kind == factory_core::GraduationIntentKind::Weighted,
                        creator,
                    ),
                    300_000,
                )
                .unwrap();
            if balance(&reserved.state, custody, launch) == 0 {
                break;
            }
        }
        assert_eq!(balance(&reserved.state, custody, launch), 0);
        assert_eq!(
            maybe_balance(&reserved, creator, launch),
            before + inventory + 120
        );
        let before_retry = maybe_balance(&reserved, creator, launch);
        let _ = reserved.execute(
            lbp_manual_message(
                kind == factory_core::GraduationIntentKind::Weighted,
                creator,
            ),
            300_000,
        );
        assert_eq!(maybe_balance(&reserved, creator, launch), before_retry);
    }
}

#[test]
fn graduation_reserve_survives_fixed_price_sellout() {
    let mut reserved = env_configuration_with_reserve(
        factory_core::GraduationIntentKind::FixedPrice,
        false,
        configuration(),
        120,
    );
    let creator = reserved.creator();
    reserved.state = reserved
        .state
        .with_public_accounts([token_holding(creator, COLLATERAL, 5000)]);
    let remaining = fixed_pool(&reserved.state, reserved.pool).remaining_inventory;
    reserved.buy(remaining, 5000);
    assert_eq!(
        fixed_pool(&reserved.state, reserved.pool).remaining_inventory,
        0
    );
    graduate(&mut reserved);
    let request = adapter_state(&reserved).receipt.unwrap();
    assert_eq!(request.reserve_tokens, 120);
    assert_eq!(request.unsold_tokens, 0);
    assert_eq!(request.intent.amounts().0, 120);
}

#[test]
fn graduation_reserve_debit_failure_rolls_back_other_payouts_and_can_retry() {
    for kind in [
        factory_core::GraduationIntentKind::Curve,
        factory_core::GraduationIntentKind::FixedPrice,
        factory_core::GraduationIntentKind::Weighted,
    ] {
        let mut reserved = env_configuration_with_reserve(kind, false, configuration(), 120);
        if kind == factory_core::GraduationIntentKind::Curve {
            for _ in 0..3 {
                call(&mut reserved, false).unwrap();
            }
        }
        let custody = factory_core::compute_mint_pda(NAMESPACE, LBP_FACTORY_PROGRAM, reserved.salt);
        let launch = reserved.launch;
        reserved.state = reserved
            .state
            .with_public_accounts([token_holding(custody, launch, 119)]);
        let before = reserved.state.clone();
        assert!(call(&mut reserved, false).is_err());
        assert!(reserved.state == before);
        assert!(adapter_state(&reserved).receipt.is_none());
        reserved.state = reserved
            .state
            .with_public_accounts([token_holding(custody, launch, 120)]);
        graduate(&mut reserved);
        assert_eq!(balance(&reserved.state, custody, launch), 0);
        assert_eq!(
            adapter_state(&reserved).receipt.unwrap().reserve_tokens,
            120
        );
    }
}
