use super::*;

#[test]
fn lbp_graduation_splits_both_assets_with_both_allocation_orders_while_paused_and_frozen() {
    for weighted in [false, true] {
        for allocation_first in [false, true] {
            let mut env = lbp_graduation_env(
                weighted,
                true,
                b"saved-lbp-route".to_vec(),
                lbp_kind(weighted),
            );
            donate_lbp_surplus(&mut env);
            let creator = env.creator();
            env.execute(
                factory_core::Message::SetPaused {
                    creator,
                    paused: true,
                },
                299_000,
            )
            .unwrap();
            env.state = fixed_config(
                env.state,
                if weighted { 100 } else { 333 },
                NEXT_TREASURY,
                true,
            );
            let initial = fixed_pool(&env.state, env.pool);
            let collateral = balance(&env.state, creator, COLLATERAL);
            let tokens = balance(&env.state, creator, env.launch);
            let supply = token_supply(&env.state, env.launch);
            if allocation_first {
                env.execute(lbp_allocation_message(weighted, creator), 300_000)
                    .unwrap();
            }
            let caller = PrivateKey::try_new([96; 32]).unwrap();
            factory_execute_at(
                &mut env.state,
                env.factory,
                &env.actors,
                &caller,
                env.salt,
                env.launch,
                env.pool,
                lbp_hook_message(creator),
                300_000,
            )
            .unwrap();
            assert_lbp_graduated(
                &env,
                weighted,
                &initial,
                collateral,
                supply,
                true,
                allocation_first,
            );
            env.execute(lbp_allocation_message(weighted, creator), 300_000)
                .unwrap();
            assert_lbp_graduated(&env, weighted, &initial, collateral, supply, true, true);
            assert_eq!(
                balance(&env.state, creator, env.launch),
                tokens + 200 + initial.remaining_inventory - initial.remaining_inventory / 4
            );
            reject_lbp_factory(
                &mut env,
                lbp_hook_message(creator),
                300_000,
                "AlreadySettled",
            );
            reject_lbp_factory(
                &mut env,
                lbp_manual_message(weighted, creator),
                300_000,
                "InvalidTerms",
            );
            assert_lbp_graduated(&env, weighted, &initial, collateral, supply, true, true);
        }
    }
}

#[test]
fn lbp_graduation_requires_the_inclusive_scheduled_end_before_any_effect() {
    for weighted in [false, true] {
        let mut env = lbp_graduation_env(weighted, true, vec![], lbp_kind(weighted));
        let creator = env.creator();
        let before = env.state.clone();
        let nonce = env.state.get_account_by_id(creator).nonce;
        let result = env.execute(lbp_hook_message(creator), 299_999);
        assert!(
            matches!(result, Err(lee::error::LeeError::OutOfValidityWindow)),
            "{result:?}"
        );
        assert!(env.state == before);
        assert_eq!(env.state.get_account_by_id(creator).nonce, nonce);
        env.execute(lbp_hook_message(creator), 300_000).unwrap();
        assert_eq!(
            lbp_launch(&env).settlement_progress,
            lbp_core::SettlementProgress::Graduated
        );
    }
}

#[test]
fn lbp_graduation_pays_both_remainders_to_the_current_creator_after_role_transfer() {
    for weighted in [false, true] {
        use lez_compat_core::authority;
        let mut env = lbp_graduation_env(weighted, true, vec![], lbp_kind(weighted));
        let old_creator = env.creator();
        let old_key = PrivateKey::try_new([23; 32]).unwrap();
        let next_key = PrivateKey::try_new([95; 32]).unwrap();
        let next_creator = AccountId::from(&PublicKey::new_from_private_key(&next_key));
        let role = lbp_launch(&env).creator_role_id;
        env.actors.extend([
            Actor::new(next_creator, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
            Actor::new(next_creator, TOKEN_PROGRAM_ACCOUNT_ID),
            holding(next_creator, COLLATERAL),
            holding(next_creator, env.launch),
        ]);
        lbp_signed_call(
            &mut env,
            Actor::new(old_creator, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
            authority::Message::Transfer {
                role,
                successor: next_creator,
            },
            &[&old_key, &next_key],
            vec![],
        )
        .unwrap();
        let old_tokens = balance(&env.state, old_creator, env.launch);
        let old_collateral = balance(&env.state, old_creator, COLLATERAL);
        reject_lbp_factory(
            &mut env,
            lbp_hook_message(old_creator),
            300_000,
            "Unauthorized",
        );
        reject_lbp_factory(
            &mut env,
            lbp_manual_message(weighted, old_creator),
            300_000,
            "InvalidTerms",
        );
        env.key = next_key;
        env.state = fixed_config(env.state, 100, NEXT_TREASURY, true);
        let initial = fixed_pool(&env.state, env.pool);
        let supply = token_supply(&env.state, env.launch);
        let caller = PrivateKey::try_new([96; 32]).unwrap();
        let events = factory_execute_events_at(
            &mut env.state,
            env.factory,
            &env.actors,
            &caller,
            env.salt,
            env.launch,
            env.pool,
            lbp_hook_message(next_creator),
            300_000,
        )
        .unwrap();
        assert_lbp_settlement_events(
            &events,
            env.pool,
            &initial,
            next_creator,
            NEXT_TREASURY,
            Some(lbp_adapter(&env).account_id),
        );
        assert_lbp_graduated_event(&events, &env, weighted, &initial);
        assert_lbp_graduated(&env, weighted, &initial, 0, supply, false, false);
        assert_eq!(balance(&env.state, old_creator, COLLATERAL), old_collateral);
        assert_eq!(
            balance(&env.state, next_creator, COLLATERAL),
            initial.recorded_collateral - initial.recorded_collateral * 6 / 10
        );
        reject_lbp_factory(
            &mut env,
            lbp_manual_message(weighted, old_creator),
            300_000,
            "InvalidTerms",
        );
        assert_eq!(balance(&env.state, old_creator, env.launch), old_tokens);
        assert_eq!(
            balance(&env.state, next_creator, env.launch),
            initial.remaining_inventory - initial.remaining_inventory / 4
        );
    }
}

#[test]
fn lbp_configured_adapter_rejects_manual_settlement_before_and_after_graduation() {
    for weighted in [false, true] {
        let mut env = lbp_graduation_env(weighted, false, vec![], lbp_kind(weighted));
        let creator = env.creator();
        reject_lbp_factory(
            &mut env,
            lbp_manual_message(weighted, creator),
            300_000,
            "InvalidTerms",
        );
        env.execute(lbp_hook_message(creator), 300_000).unwrap();
        reject_lbp_factory(
            &mut env,
            lbp_manual_message(weighted, creator),
            300_000,
            "InvalidTerms",
        );
    }
}

#[test]
fn lbp_no_buyers_graduation_splits_inventory_and_weighted_seed_without_burning() {
    for weighted in [false, true] {
        let mut env = lbp_graduation_env(weighted, false, vec![], lbp_kind(weighted));
        env.state = fixed_config(env.state, 100, NEXT_TREASURY, true);
        let creator = env.creator();
        let initial = fixed_pool(&env.state, env.pool);
        let collateral = balance(&env.state, creator, COLLATERAL);
        let supply = token_supply(&env.state, env.launch);
        let events = env
            .execute_events(lbp_hook_message(creator), 300_000)
            .unwrap();
        assert_lbp_settlement_events(
            &events,
            env.pool,
            &initial,
            creator,
            NEXT_TREASURY,
            Some(lbp_adapter(&env).account_id),
        );
        assert_lbp_graduated_event(&events, &env, weighted, &initial);
        assert_eq!(
            balance(&env.state, lbp_adapter(&env).account_id, env.launch),
            200
        );
        assert_eq!(balance(&env.state, creator, env.launch), 600);
        assert_eq!(
            balance(&env.state, lbp_adapter(&env).account_id, COLLATERAL),
            if weighted { 60 } else { 0 }
        );
        assert_eq!(
            balance(&env.state, creator, COLLATERAL),
            collateral + if weighted { 40 } else { 0 }
        );
        assert_lbp_graduated(&env, weighted, &initial, collateral, supply, false, false);
        reject_lbp_factory(
            &mut env,
            lbp_hook_message(creator),
            300_000,
            "AlreadySettled",
        );
        reject_lbp_factory(
            &mut env,
            lbp_manual_message(weighted, creator),
            300_000,
            "InvalidTerms",
        );
        assert_lbp_graduated(&env, weighted, &initial, collateral, supply, false, false);
    }
}

fn reject_lbp_adapter_reinitialization(
    env: &mut FixedFactoryEnv,
    accepted_intent: factory_core::GraduationIntentKind,
) {
    let before = env.state.clone();
    let adapter = lbp_adapter(env);
    let salt = env.salt;
    let result = lbp_signed_call(
        env,
        adapter,
        factory_core::AdapterMessage::Initialize {
            factory_program_id: LBP_FACTORY_PROGRAM,
            namespace: NAMESPACE,
            launch_salt: salt,
            accepted_intent,
        },
        &[],
        vec![],
    );
    assert!(
        matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("AlreadyInitialized")),
        "{result:?}"
    );
    assert!(
        env.state == before,
        "adapter reinitialization changed state"
    );
}

#[test]
fn lbp_adapter_initialization_is_immutable_before_and_after_graduation() {
    for weighted in [false, true] {
        let mut env = lbp_graduation_env(weighted, true, vec![], lbp_kind(weighted));
        assert_eq!(lbp_adapter_state(&env).accepted_intent, lbp_kind(weighted));
        for accepted in [
            lbp_kind(weighted),
            lbp_kind(!weighted),
            factory_core::GraduationIntentKind::Curve,
        ] {
            reject_lbp_adapter_reinitialization(&mut env, accepted);
        }
        let creator = env.creator();
        env.execute(lbp_hook_message(creator), 300_000).unwrap();
        let receipt = lbp_adapter_state(&env).receipt.unwrap();
        for accepted in [
            lbp_kind(weighted),
            lbp_kind(!weighted),
            factory_core::GraduationIntentKind::Curve,
        ] {
            reject_lbp_adapter_reinitialization(&mut env, accepted);
        }
        assert_eq!(lbp_adapter_state(&env).receipt, Some(receipt));
    }
}

#[test]
fn lbp_graduation_rejects_malicious_wrong_kind_adapter_state_and_cannot_change_its_intent() {
    for weighted in [false, true] {
        for accepted in [
            lbp_kind(!weighted),
            factory_core::GraduationIntentKind::Curve,
        ] {
            let mut env = lbp_graduation_env(weighted, true, vec![], lbp_kind(weighted));
            let adapter = lbp_adapter(&env);
            let mut malicious = lbp_adapter_state(&env);
            malicious.accepted_intent = accepted;
            env.state = env
                .state
                .with_public_accounts([account(adapter, borsh::to_vec(&malicious).unwrap())]);
            let creator = env.creator();
            reject_lbp_factory(
                &mut env,
                lbp_hook_message(creator),
                300_000,
                "AdapterIntentMismatch",
            );
            for replacement in [accepted, lbp_kind(weighted)] {
                reject_lbp_adapter_reinitialization(&mut env, replacement);
            }
            assert_eq!(lbp_adapter_state(&env).accepted_intent, accepted);
        }
    }
}

#[test]
fn lbp_graduation_rejects_missing_or_wrong_creator_hints_including_unrelated_roles() {
    for weighted in [false, true] {
        let mut env = lbp_graduation_env(weighted, true, vec![], lbp_kind(weighted));
        for hint in [None, Some(AccountId::default())] {
            reject_lbp_factory(
                &mut env,
                factory_core::Message::ExecuteGraduationHook { creator_hint: hint },
                300_000,
                "InvalidTerms",
            );
        }
        for (seed, role) in [
            (97, None),
            (
                98,
                Some(factory_core::compute_creator_role_pda(
                    NAMESPACE,
                    LBP_FACTORY_PROGRAM,
                    [25; 32],
                )),
            ),
            (99, Some(curve_core::compute_admin_role(NAMESPACE, CURVE))),
        ] {
            let key = PrivateKey::try_new([seed; 32]).unwrap();
            let caller = AccountId::from(&PublicKey::new_from_private_key(&key));
            if let Some(role) = role {
                env.state = env.state.with_public_accounts([seeded_role(role, caller)]);
                env.actors
                    .push(Actor::new(role, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID));
            }
            env.actors.push(Actor::new(
                caller,
                factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
            ));
            reject_lbp_factory(&mut env, lbp_hook_message(caller), 300_000, "Unauthorized");
        }
    }
}

#[test]
fn lbp_graduation_rejects_saved_adapter_and_pool_custody_mismatches_without_effects() {
    for weighted in [false, true] {
        for fault in [
            "adapter",
            "owner",
            "definition",
            "kind",
            "factory",
            "progress",
            "policy",
        ] {
            let mut env = lbp_graduation_env(weighted, true, vec![], lbp_kind(weighted));
            let creator = env.creator();
            let adapter = lbp_adapter(&env);
            let reason;
            if matches!(fault, "adapter" | "progress" | "policy") {
                let mut pool = fixed_pool(&env.state, env.pool);
                if fault == "adapter" {
                    pool.graduation_adapter =
                        Some(Actor::new(adapter.account_id, AccountId::new([93; 32])));
                    reason = "InvalidCustody";
                } else if fault == "policy" {
                    pool.graduation_policy.launch_bps += 1;
                    reason = "InvalidCustody";
                } else {
                    pool.settlement_progress = lbp_core::SettlementProgress::ManualComplete;
                    pool.recorded_collateral = 0;
                    pool.accrued_fees = 0;
                    reason = "InvalidState";
                }
                env.state = env.state.with_public_accounts([account(
                    Actor::new(env.pool, LBP),
                    borsh::to_vec(&pool).unwrap(),
                )]);
            } else if fault == "factory" {
                let mut saved = lbp_adapter_state(&env);
                saved.factory = Actor::new(AccountId::new([93; 32]), LBP_FACTORY_PROGRAM);
                env.state = env
                    .state
                    .with_public_accounts([account(adapter, borsh::to_vec(&saved).unwrap())]);
                reason = "InvalidCustody";
            } else {
                let bytes = borsh::to_vec(&TokenState::Holding {
                    owner: if fault == "owner" {
                        env.pool
                    } else {
                        adapter.account_id
                    },
                    definition: if fault == "definition" {
                        env.launch
                    } else {
                        COLLATERAL
                    },
                    kind: if fault == "kind" {
                        TokenKind::NftMaster
                    } else {
                        TokenKind::Fungible
                    },
                    balance: 0,
                })
                .unwrap();
                env.state = env.state.with_public_accounts([account(
                    holding(adapter.account_id, COLLATERAL),
                    bytes,
                )]);
                reason = "WrongDefinition";
            }
            reject_lbp_factory(&mut env, lbp_hook_message(creator), 300_000, reason);
        }
    }
}

#[test]
fn lbp_graduation_without_a_saved_hook_rejects_before_any_custody_change() {
    for weighted in [false, true] {
        let mut env = if weighted {
            let (state, factory, pool, launch, escrow, key, actors, salt) = weighted_factory_env();
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
        } else {
            fixed_settlement_env(Some(200))
        };
        env.state = clock_state(env.state, 300_000);
        let creator = env.creator();
        reject_lbp_factory(
            &mut env,
            lbp_hook_message(creator),
            300_000,
            "HookNotConfigured",
        );
    }
}

#[test]
fn lbp_public_callers_cannot_select_a_route_or_forge_pool_finalization() {
    for weighted in [false, true] {
        let mut env = lbp_graduation_env(weighted, true, vec![], lbp_kind(weighted));
        let creator = env.creator();
        for (instruction, expected) in [
            (
                lbp_core::Message::Settle {
                    route: lbp_core::SettlementRoute::Graduation,
                    creator,
                    adapter: Some(lbp_adapter(&env)),
                },
                "Unauthorized",
            ),
            (
                lbp_core::Message::Settle {
                    route: lbp_core::SettlementRoute::Manual,
                    creator,
                    adapter: None,
                },
                "Unauthorized",
            ),
            (
                lbp_core::Message::FinalizeSettlement {
                    receipt_hash: [0; 32],
                },
                "Unauthorized",
            ),
        ] {
            let before = env.state.clone();
            let nonce = env.state.get_account_by_id(creator).nonce;
            let result = fixed_execute_at(&mut env.state, env.pool, &env.key, instruction, 300_000);
            assert!(
                matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains(expected)),
                "{result:?}"
            );
            assert!(env.state == before);
            assert_eq!(env.state.get_account_by_id(creator).nonce, nonce);
        }
    }
}

#[test]
fn signed_curve_graduation_splits_unsold_and_collateral_without_minting_a_reserve_or_burning() {
    let mut env = curve_factory_env(true, vec![], test_graduation_policy(true));
    donate_lbp_surplus(&mut env);
    let creator = env.creator();
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let factory = env.factory;
    for (definition, amount) in [(env.launch, 3), (COLLATERAL, 5)] {
        lbp_signed_call(
            &mut env,
            Actor::new(creator, TOKEN_PROGRAM_ACCOUNT_ID),
            token::Message::Transfer {
                definition,
                recipient: factory,
                amount,
            },
            &[&key],
            vec![lee::PublicAccountEvidence::Pda {
                program: TOKEN_PROGRAM_ACCOUNT_ID,
                seed: token::holding_seed(factory, definition),
            }],
        )
        .unwrap();
    }
    let tokens = balance(&env.state, creator, env.launch);
    let collateral = balance(&env.state, creator, COLLATERAL);
    assert_eq!(token_supply(&env.state, env.launch), 4_200);
    assert_eq!(
        saved_curve_factory(&env).graduation_policy,
        test_graduation_policy(true)
    );
    reject_lbp_factory(
        &mut env,
        lbp_manual_message(false, creator),
        300_000,
        "InvalidTerms",
    );
    for _ in 0..4 {
        env.execute(lbp_hook_message(creator), 300_000).unwrap();
    }
    assert_eq!(
        saved_curve_factory(&env).settlement_stage,
        factory_core::SettlementStage::Graduated
    );
    let adapter = lbp_adapter(&env).account_id;
    assert_eq!(balance(&env.state, adapter, env.launch), 750);
    assert_eq!(balance(&env.state, adapter, COLLATERAL), 67);
    assert_eq!(balance(&env.state, creator, env.launch), tokens + 2_250);
    assert_eq!(balance(&env.state, creator, COLLATERAL), collateral + 45);
    assert_eq!(token_supply(&env.state, env.launch), 4_200);
    assert_eq!(balance(&env.state, env.escrow, env.launch), 200);
    assert_eq!(balance(&env.state, env.pool, env.launch), 7);
    assert_eq!(balance(&env.state, env.pool, COLLATERAL), 41);
    assert_eq!(balance(&env.state, env.factory, env.launch), 3);
    assert_eq!(balance(&env.state, env.factory, COLLATERAL), 5);
    assert_eq!(
        lbp_adapter_state(&env).receipt.unwrap().intent,
        factory_core::GraduationIntent::Curve {
            launch_tokens: 750,
            collateral: 67
        }
    );
    reject_lbp_factory(
        &mut env,
        lbp_manual_message(false, creator),
        300_000,
        "InvalidTerms",
    );
    reject_lbp_factory(
        &mut env,
        lbp_hook_message(creator),
        300_000,
        "AlreadySettled",
    );
}

#[test]
fn signed_curve_manual_settlement_pays_both_assets_and_keeps_the_unsold_snapshot() {
    let mut env = curve_factory_env(false, vec![], factory_core::GraduationPolicy::default());
    let creator = env.creator();
    let tokens = balance(&env.state, creator, env.launch);
    let collateral = balance(&env.state, creator, COLLATERAL);
    for _ in 0..2 {
        env.execute(lbp_manual_message(false, creator), 300_000)
            .unwrap();
    }
    assert_eq!(
        saved_curve_factory(&env).settlement_stage,
        factory_core::SettlementStage::Withdrawn {
            unsold: 3_000,
            collateral_due: 112
        }
    );
    env.execute(lbp_manual_message(false, creator), 300_000)
        .unwrap();
    assert_eq!(
        saved_curve_factory(&env).settlement_stage,
        factory_core::SettlementStage::Complete
    );
    assert_eq!(balance(&env.state, creator, env.launch), tokens + 3_000);
    assert_eq!(balance(&env.state, creator, COLLATERAL), collateral + 112);
    assert_eq!(token_supply(&env.state, env.launch), 4_200);
}

#[test]
fn signed_lbp_independent_zero_and_single_asset_policies_still_require_the_adapter() {
    for weighted in [false, true] {
        for (launch_bps, collateral_bps) in [(0, 0), (10_000, 0), (0, 10_000), (10_000, 10_000)] {
            let policy = factory_core::GraduationPolicy {
                launch_bps,
                collateral_bps,
            };
            let mut env =
                lbp_graduation_env_with_policy(weighted, false, vec![], lbp_kind(weighted), policy);
            let creator = env.creator();
            let collateral = balance(&env.state, creator, COLLATERAL);
            let initial = fixed_pool(&env.state, env.pool);
            assert_eq!(initial.graduation_policy, policy);
            assert_eq!(lbp_launch(&env).graduation_policy, policy);
            reject_lbp_factory(
                &mut env,
                lbp_manual_message(weighted, creator),
                300_000,
                "InvalidTerms",
            );
            env.state = fixed_config(env.state, 100, NEXT_TREASURY, true);
            env.execute(lbp_hook_message(creator), 300_000).unwrap();
            assert_lbp_graduated(&env, weighted, &initial, collateral, 1_000, false, false);
            assert_eq!(
                balance(&env.state, creator, env.launch),
                if launch_bps == 0 { 800 } else { 0 }
            );
            reject_lbp_factory(
                &mut env,
                lbp_manual_message(weighted, creator),
                300_000,
                "InvalidTerms",
            );
        }
    }
}

#[test]
fn signed_fixed_depletion_graduates_zero_unsold_with_a_full_launch_share() {
    let mut env = lbp_graduation_env_with_policy(
        false,
        false,
        vec![],
        lbp_kind(false),
        factory_core::GraduationPolicy {
            launch_bps: 10_000,
            collateral_bps: 2_500,
        },
    );
    let creator = env.creator();
    env.state = clock_state(
        env.state
            .with_public_accounts([token_holding(creator, COLLATERAL, 2_000)]),
        100_000,
    );
    env.buy(800, 2_000);
    let tokens = balance(&env.state, creator, env.launch);
    let collateral = balance(&env.state, creator, COLLATERAL);
    assert_eq!(fixed_pool(&env.state, env.pool).remaining_inventory, 0);
    env.execute(lbp_hook_message(creator), 100_000).unwrap();
    assert_eq!(
        balance(&env.state, lbp_adapter(&env).account_id, env.launch),
        0
    );
    assert_eq!(
        balance(&env.state, lbp_adapter(&env).account_id, COLLATERAL),
        300
    );
    assert_eq!(balance(&env.state, creator, env.launch), tokens);
    assert_eq!(balance(&env.state, creator, COLLATERAL), collateral + 900);
    assert_eq!(token_supply(&env.state, env.launch), 1_000);
    assert_eq!(
        lbp_launch(&env).settlement_progress,
        lbp_core::SettlementProgress::Graduated
    );
}

#[test]
fn signed_wrong_kind_adapter_initialization_rolls_back_then_correct_kind_succeeds() {
    for weighted in [false, true] {
        let mut env = lbp_graduation_env(weighted, false, vec![], lbp_kind(weighted));
        let adapter = lbp_adapter(&env);
        env.state = env
            .state
            .with_public_accounts([(adapter.account_id, Account::default())]);
        let salt = env.salt;
        let evidence = vec![lee::PublicAccountEvidence::Pda {
            program: LBP_HOOK_PROGRAM,
            seed: factory_core::compute_graduation_seed(env.factory),
        }];
        let before = env.state.clone();
        let result = lbp_signed_call(
            &mut env,
            adapter,
            factory_core::AdapterMessage::Initialize {
                factory_program_id: LBP_FACTORY_PROGRAM,
                namespace: NAMESPACE,
                launch_salt: salt,
                accepted_intent: lbp_kind(!weighted),
            },
            &[],
            evidence.clone(),
        );
        assert!(
            matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("AdapterIntentMismatch")),
            "{result:?}"
        );
        assert!(
            env.state == before,
            "wrong-kind initialization changed runtime state"
        );
        lbp_signed_call(
            &mut env,
            adapter,
            factory_core::AdapterMessage::Initialize {
                factory_program_id: LBP_FACTORY_PROGRAM,
                namespace: NAMESPACE,
                launch_salt: salt,
                accepted_intent: lbp_kind(weighted),
            },
            &[],
            evidence,
        )
        .unwrap();
        assert_eq!(lbp_adapter_state(&env).accepted_intent, lbp_kind(weighted));
        reject_lbp_adapter_reinitialization(&mut env, lbp_kind(weighted));
    }
}
