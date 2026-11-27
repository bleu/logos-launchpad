use super::*;

#[test]
fn lbp_graduation_late_faults_roll_back_every_effect_and_repaired_retry_pays_once() {
    for weighted in [false, true] {
        for fault in [
            "fee",
            "creator_launch",
            "creator_collateral",
            "adapter_launch",
            "adapter",
            "callback",
            "ack",
            "hash",
            "completed",
        ] {
            let configuration = match fault {
                "callback" => b"reject-callback".to_vec(),
                "ack" => b"corrupt-final-ack".to_vec(),
                "hash" => b"corrupt-request-hash".to_vec(),
                _ => vec![],
            };
            let mut env = lbp_graduation_env(weighted, true, configuration, lbp_kind(weighted));
            donate_lbp_surplus(&mut env);
            env.state = fixed_config(env.state, 100, NEXT_TREASURY, true);
            let creator = env.creator();
            let initial = fixed_pool(&env.state, env.pool);
            let collateral = balance(&env.state, creator, COLLATERAL);
            let supply = token_supply(&env.state, env.launch);
            let mut repair = None;
            let reason = match fault {
                "fee" | "creator_collateral" | "creator_launch" | "adapter_launch" | "adapter" => {
                    let owner = match fault {
                        "fee" => NEXT_TREASURY,
                        "creator_collateral" | "creator_launch" => creator,
                        _ => lbp_adapter(&env).account_id,
                    };
                    let asset = if matches!(fault, "creator_launch" | "adapter_launch") {
                        env.launch
                    } else {
                        COLLATERAL
                    };
                    let actor = holding(owner, asset);
                    let original = if matches!(fault, "adapter" | "adapter_launch") {
                        borsh::to_vec(&TokenState::Holding {
                            owner,
                            definition: asset,
                            kind: TokenKind::Fungible,
                            balance: 0,
                        })
                        .unwrap()
                    } else {
                        env.state
                            .get_account_by_id(actor.account_id)
                            .data
                            .actor_state(TOKEN_PROGRAM_ACCOUNT_ID)
                            .to_vec()
                    };
                    repair = Some((actor, original));
                    env.state =
                        env.state
                            .with_public_accounts([token_holding(owner, asset, u128::MAX)]);
                    "Overflow"
                }
                "completed" => {
                    env.state = env.state.with_programs_at(
                        [(
                            LBP,
                            Program::new(Cow::Borrowed(
                                launchpad_methods::LBP_COMPLETION_FAULT_ELF,
                            ))
                            .unwrap(),
                        )]
                        .map(|(address, program)| (address, program, true)),
                    );
                    "InvalidState"
                }
                "callback" => "callback rejected",
                _ => "InvalidState",
            };
            reject_lbp_factory(&mut env, lbp_hook_message(creator), 300_000, reason);
            assert_eq!(fixed_pool(&env.state, env.pool), initial);
            assert_eq!(
                lbp_launch(&env).settlement_progress,
                lbp_core::SettlementProgress::Unselected
            );
            assert!(lbp_adapter_state(&env).receipt.is_none());
            if let Some((actor, original)) = repair {
                env.state = env.state.with_public_accounts([account(actor, original)]);
            }
            // These program mappings exist only in this disposable fault fixture.
            if matches!(fault, "callback" | "ack" | "hash") {
                env.state = env.state.with_programs_at(
                    [(
                        LBP_HOOK_PROGRAM,
                        Program::new(Cow::Borrowed(launchpad_methods::GRADUATION_ADAPTER_ELF))
                            .unwrap(),
                    )]
                    .map(|(address, program)| (address, program, true)),
                );
            }
            if fault == "completed" {
                env.state = env.state.with_programs_at(
                    [(
                        LBP,
                        Program::new(Cow::Borrowed(launchpad_methods::LBP_ELF)).unwrap(),
                    )]
                    .map(|(address, program)| (address, program, true)),
                );
            }
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
            assert_lbp_graduated(&env, weighted, &initial, collateral, supply, true, false);
            reject_lbp_factory(
                &mut env,
                lbp_hook_message(creator),
                300_000,
                "AlreadySettled",
            );
            assert_lbp_graduated(&env, weighted, &initial, collateral, supply, true, false);
        }
    }
}

#[test]
fn lbp_callback_failure_rolls_back_both_destinations_and_forbids_manual_escape_until_adapter_repair()
 {
    for weighted in [false, true] {
        let mut env = lbp_graduation_env(
            weighted,
            true,
            b"reject-callback".to_vec(),
            lbp_kind(weighted),
        );
        env.state = fixed_config(env.state, 100, NEXT_TREASURY, true);
        let creator = env.creator();
        let initial = fixed_pool(&env.state, env.pool);
        let collateral = balance(&env.state, creator, COLLATERAL);
        let tokens = balance(&env.state, creator, env.launch);
        let supply = token_supply(&env.state, env.launch);
        assert!(initial.remaining_inventory > 0 && initial.recorded_collateral > 0);
        reject_lbp_factory(
            &mut env,
            lbp_hook_message(creator),
            300_000,
            "callback rejected",
        );
        reject_lbp_factory(
            &mut env,
            lbp_manual_message(weighted, creator),
            300_000,
            "InvalidTerms",
        );
        assert_eq!(balance(&env.state, creator, COLLATERAL), collateral);
        assert_eq!(balance(&env.state, creator, env.launch), tokens);
        assert!(lbp_adapter_state(&env).receipt.is_none());
        env.state = env.state.with_programs_at(
            ([(
                LBP_HOOK_PROGRAM,
                Program::new(Cow::Borrowed(launchpad_methods::GRADUATION_ADAPTER_ELF)).unwrap(),
            )])
            .map(|(address, program)| (address, program, true)),
        );
        env.execute(lbp_hook_message(creator), 300_000).unwrap();
        assert_lbp_graduated(&env, weighted, &initial, collateral, supply, false, false);
        assert_eq!(
            balance(&env.state, creator, env.launch),
            tokens + initial.remaining_inventory - initial.remaining_inventory / 4
        );
        reject_lbp_factory(
            &mut env,
            lbp_manual_message(weighted, creator),
            300_000,
            "InvalidTerms",
        );
    }
}

fn lbp_nested_configuration(target: Actor, instruction: impl borsh::BorshSerialize) -> Vec<u8> {
    let mut configuration = b"nested-call:".to_vec();
    configuration.extend_from_slice(target.account_id.value());
    configuration.extend_from_slice(target.program_account_id.value());
    configuration.extend(borsh::to_vec(&instruction).unwrap());
    configuration
}

#[test]
fn lbp_nested_adapter_calls_cannot_reenter_pending_factory_or_pool_and_retry_pays_once() {
    for weighted in [false, true] {
        let salt = [24; 32];
        let factory = factory_core::compute_factory_pda(NAMESPACE, LBP_FACTORY_PROGRAM, salt);
        let launch = factory_core::compute_definition_pda(NAMESPACE, LBP_FACTORY_PROGRAM, salt);
        let pool = lbp_core::compute_pool_pda(NAMESPACE, LBP, launch, COLLATERAL, factory);
        let creator = AccountId::from(&PublicKey::new_from_private_key(
            &PrivateKey::try_new([23; 32]).unwrap(),
        ));
        let mut attempts = vec![];
        for instruction in [
            lbp_hook_message(creator),
            lbp_manual_message(weighted, creator),
            lbp_allocation_message(weighted, creator),
            factory_core::Message::ReadState,
            factory_core::Message::SetPaused {
                creator,
                paused: false,
            },
        ] {
            attempts.push((
                lbp_nested_configuration(Actor::new(factory, LBP_FACTORY_PROGRAM), instruction),
                "InvalidMessage",
            ));
        }
        attempts.push((
            lbp_nested_configuration(
                Actor::new(factory, LBP_FACTORY_PROGRAM),
                factory_core::GraduationResult {
                    factory,
                    request_hash: [0; 32],
                },
            ),
            "InvalidState",
        ));
        attempts.push((
            lbp_nested_configuration(
                Actor::new(factory, LBP_FACTORY_PROGRAM),
                lbp_core::SettlementReply::Completed {
                    receipt_hash: [0; 32],
                    progress: lbp_core::SettlementProgress::Graduated,
                },
            ),
            "InvalidMessage",
        ));
        attempts.push((
            lbp_nested_configuration(
                Actor::new(factory, LBP_FACTORY_PROGRAM),
                lbp_core::SettlementReply::Applied {
                    receipt: lbp_core::SettlementReceipt {
                        factory: Actor::new(factory, LBP_FACTORY_PROGRAM),
                        pool: Actor::new(pool, LBP),
                        route: lbp_core::SettlementRoute::Graduation,
                        creator,
                        adapter: Some(Actor::new(
                            factory_core::compute_graduation_pda(LBP_HOOK_PROGRAM, factory),
                            LBP_HOOK_PROGRAM,
                        )),
                        eligible: lbp_core::SettlementAmounts {
                            launch_tokens: 0,
                            collateral: 0,
                        },
                        payouts: factory_core::SettlementPayouts::default(),
                        progress: lbp_core::SettlementProgress::Graduated,
                    },
                },
            ),
            "InvalidMessage",
        ));
        for instruction in [
            lbp_core::Message::ReadState,
            lbp_core::Message::CollectFees,
            lbp_core::Message::SetPaused { paused: false },
            lbp_core::Message::Settle {
                route: lbp_core::SettlementRoute::Manual,
                creator,
                adapter: None,
            },
            lbp_core::Message::FinalizeSettlement {
                receipt_hash: [0; 32],
            },
        ] {
            attempts.push((
                lbp_nested_configuration(Actor::new(pool, LBP), instruction),
                "Unauthorized",
            ));
        }
        for (configuration, reason) in attempts {
            let mut env =
                lbp_graduation_env(weighted, true, configuration.clone(), lbp_kind(weighted));
            env.state = fixed_config(env.state, 100, NEXT_TREASURY, true);
            let initial = fixed_pool(&env.state, env.pool);
            let collateral = balance(&env.state, creator, COLLATERAL);
            let supply = token_supply(&env.state, env.launch);
            reject_lbp_factory(&mut env, lbp_hook_message(creator), 300_000, reason);
            assert_eq!(fixed_pool(&env.state, env.pool), initial);
            assert_eq!(
                lbp_launch(&env).settlement_progress,
                lbp_core::SettlementProgress::Unselected
            );
            assert!(lbp_adapter_state(&env).receipt.is_none());
            env.state = env.state.with_programs_at(
                [(
                    LBP_HOOK_PROGRAM,
                    Program::new(Cow::Borrowed(launchpad_methods::GRADUATION_ADAPTER_ELF)).unwrap(),
                )]
                .map(|(address, program)| (address, program, true)),
            );
            env.execute(lbp_hook_message(creator), 300_000).unwrap();
            assert_lbp_graduated(&env, weighted, &initial, collateral, supply, false, false);
            assert_eq!(
                lbp_adapter_state(&env).receipt.unwrap().configuration,
                configuration
            );
            reject_lbp_factory(
                &mut env,
                lbp_hook_message(creator),
                300_000,
                "AlreadySettled",
            );
        }
    }
}

#[test]
fn signed_curve_failed_callback_restores_both_payouts_and_retry_uses_the_current_creator() {
    use lez_compat_core::authority;
    let mut env = curve_factory_env(
        true,
        b"reject-callback".to_vec(),
        test_graduation_policy(true),
    );
    let old_creator = env.creator();
    for _ in 0..3 {
        env.execute(lbp_hook_message(old_creator), 300_000).unwrap();
    }
    assert_eq!(
        saved_curve_factory(&env).settlement_stage,
        factory_core::SettlementStage::HookReady {
            unsold: 3_000,
            collateral_due: 112
        }
    );
    let old_tokens = balance(&env.state, old_creator, env.launch);
    let old_collateral = balance(&env.state, old_creator, COLLATERAL);
    reject_lbp_factory(
        &mut env,
        lbp_hook_message(old_creator),
        300_000,
        "callback rejected",
    );
    reject_lbp_factory(
        &mut env,
        lbp_manual_message(false, old_creator),
        300_000,
        "InvalidTerms",
    );
    let next_key = PrivateKey::try_new([95; 32]).unwrap();
    let next_creator = AccountId::from(&PublicKey::new_from_private_key(&next_key));
    let role = saved_curve_factory(&env).creator_role_id;
    env.actors.extend([
        Actor::new(next_creator, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
        Actor::new(next_creator, TOKEN_PROGRAM_ACCOUNT_ID),
        holding(next_creator, env.launch),
        holding(next_creator, COLLATERAL),
    ]);
    let old_key = PrivateKey::try_new([23; 32]).unwrap();
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
    reject_lbp_factory(
        &mut env,
        lbp_hook_message(old_creator),
        300_000,
        "Unauthorized",
    );
    for creator_hint in [None, Some(AccountId::default())] {
        reject_lbp_factory(
            &mut env,
            factory_core::Message::ExecuteGraduationHook { creator_hint },
            300_000,
            "Unauthorized",
        );
    }
    env.state = env.state.with_programs_at(
        ([(
            LBP_HOOK_PROGRAM,
            Program::new(Cow::Borrowed(launchpad_methods::GRADUATION_ADAPTER_ELF)).unwrap(),
        )])
        .map(|(address, program)| (address, program, true)),
    );
    env.key = next_key;
    env.execute(lbp_hook_message(next_creator), 300_000)
        .unwrap();
    assert_eq!(balance(&env.state, next_creator, env.launch), 2_250);
    assert_eq!(balance(&env.state, next_creator, COLLATERAL), 45);
    assert_eq!(balance(&env.state, old_creator, env.launch), old_tokens);
    assert_eq!(balance(&env.state, old_creator, COLLATERAL), old_collateral);
    assert_eq!(
        balance(&env.state, lbp_adapter(&env).account_id, env.launch),
        750
    );
    assert_eq!(
        balance(&env.state, lbp_adapter(&env).account_id, COLLATERAL),
        67
    );
    assert_eq!(token_supply(&env.state, env.launch), 4_200);
}
