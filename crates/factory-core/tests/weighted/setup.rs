use super::*;

#[test]
fn weighted_creation_validates_terms_and_keeps_a_separate_mode() {
    for case in 0..9 {
        let mut value = inputs();
        match case {
            0 => value.collateral_seed = 0,
            1 => value.token_weight_start = lbp_core::MIN_WEIGHT - 1,
            2 => value.token_weight_end = lbp_core::WEIGHT_TOTAL - lbp_core::MIN_WEIGHT + 1,
            3 => value.end_timestamp = value.start_timestamp,
            4 => value.creator = AccountId::default(),
            5 => value.name.clear(),
            6 => value.uri.clear(),
            7 => value.creator_allocation = u128::MAX,
            _ => {
                value.collateral_definition_id =
                    factory_core::compute_definition_pda(value.namespace, id(7), value.launch_salt)
            }
        }
        assert_eq!(
            handle(&creation(value)).err(),
            Some(if case == 7 {
                Error::Overflow
            } else {
                Error::InvalidTerms
            })
        );
    }
    let (_, response, setup) = minted(CreatorAllocationRelease::AfterEnd);
    let state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(matches!(
        state.mode,
        LbpSetupMode::Weighted {
            collateral_seed: 100,
            ..
        }
    ));
    assert_eq!(state.total_supply, 1000);
    setup.assert_rejects_external_continuation(&response);
    let stale = setup.continuation(&response, LbpCreationStage::Allocated);
    assert_eq!(handle(&stale).err(), Some(Error::StaleStage));
    let mut request = setup.continuation(&response, LbpCreationStage::Minted);
    request.message = borsh::to_vec(&Message::ContinueFixedPriceCreation {
        expected_stage: LbpCreationStage::Minted,
        creator: id(42),
        now: 100,
    })
    .unwrap();
    assert_eq!(handle(&request).err(), Some(Error::InvalidState));
}

#[test]
fn weighted_final_setup_requires_creator_seed_and_keeps_surplus_separate() {
    for release in [
        CreatorAllocationRelease::AfterEnd,
        CreatorAllocationRelease::AfterSetup,
    ] {
        let (_, previous, pool, setup) = prepared(release);
        let state = LbpLaunchState::decode(previous.post_state.as_ref().unwrap()).unwrap();
        let extra = |seed_balance, vault_balance| {
            vec![
                pool.clone(),
                holding(state.pool_id, state.token_definition_id, 0),
                holding(state.pool_id, state.collateral_definition_id, vault_balance),
                holding(id(42), state.collateral_definition_id, seed_balance),
            ]
        };
        assert_eq!(
            advance(&setup, &previous, extra(99, 41)).err(),
            Some(Error::InvalidCustody)
        );
        assert_eq!(
            advance(&setup, &previous, extra(100, u128::MAX)).err(),
            Some(Error::Overflow)
        );
        let response = advance(&setup, &previous, extra(100, 41)).unwrap();
        if release == CreatorAllocationRelease::AfterEnd {
            assert!(response.events.is_empty());
        } else {
            assert!(matches!(
                &launch_events(&response)[..],
                [SaleEventV1 {
                    kind: SaleEventKindV1::AllocationReleased {
                        amount: 200,
                        release: AllocationReleaseV1::AfterSetup,
                        ..
                    },
                    ..
                }]
            ));
        }
        let next = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(next.creation_stage, LbpCreationStage::Complete);
        assert_eq!(
            next.creator_allocation_released,
            release == CreatorAllocationRelease::AfterSetup
        );
        assert!(response.sync_messages.iter().any(|call| call.to
            == Actor::new(id(42), factory_core::TOKEN_PROGRAM_ACCOUNT_ID)
            && borsh::from_slice::<lez_compat_core::token::Message>(&call.message).unwrap()
                == lez_compat_core::token::Message::Transfer {
                    definition: state.collateral_definition_id,
                    recipient: state.pool_id,
                    amount: 100,
                }));
        assert!(response.sync_messages.iter().any(|call| call.to
            == Actor::new(state.pool_id, state.lbp_program_id)
            && borsh::from_slice::<lbp_core::Message>(&call.message).unwrap()
                == lbp_core::Message::ActivateWeighted));
        let mut changed = lbp_core::PoolAccount::decode(&pool).unwrap();
        if let lbp_core::PoolMode::Weighted {
            collateral_seed, ..
        } = &mut changed.mode
        {
            *collateral_seed += 1;
        }
        let mut wrong = extra(100, 41);
        wrong[0] = borsh::to_vec(&changed).unwrap();
        assert_eq!(
            advance(&setup, &previous, wrong).err(),
            Some(Error::InvalidState)
        );
    }
}

fn allowlisted_inputs() -> WeightedInputs {
    let mut terms = inputs();
    let factory = factory_core::compute_factory_pda(terms.namespace, id(7), terms.launch_salt);
    let pool = lbp_core::compute_pool_pda(
        terms.namespace,
        terms.lbp_program_id,
        factory_core::compute_definition_pda(terms.namespace, id(7), terms.launch_salt),
        terms.collateral_definition_id,
        factory,
    );
    terms.buy_policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *terms.namespace.value(),
        sale_policy_id: *pool.value(),
        root: [9; 32],
    });
    terms
}

#[test]
fn weighted_creation_saves_the_initial_bound_buy_policy() {
    let terms = allowlisted_inputs();
    let response = handle(&creation(terms.clone())).unwrap();
    assert_eq!(
        LbpLaunchState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        terms.buy_policy
    );
}

#[test]
fn weighted_policy_is_immutable_through_setup_funding() {
    let terms = allowlisted_inputs();
    let policy = terms.buy_policy.clone();
    let (_, prepared, bytes, setup) = prepared_with_terms(terms);
    let state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    assert_eq!(pool.buy_policy, policy);
    for case in 0..3 {
        let mut changed = pool.clone();
        match case {
            0 => changed.buy_policy = curve_core::BuyPolicy::Open,
            1 => {
                let curve_core::BuyPolicy::Allowlist(policy) = &mut changed.buy_policy else {
                    unreachable!()
                };
                policy.root = [99; 32];
            }
            _ => {}
        }
        let result = advance(
            &setup,
            &prepared,
            vec![
                borsh::to_vec(&changed).unwrap(),
                holding(state.pool_id, state.token_definition_id, 0),
                holding(state.pool_id, state.collateral_definition_id, 0),
                holding(id(42), state.collateral_definition_id, 100),
            ],
        );
        if case < 2 {
            assert_eq!(result.err(), Some(Error::InvalidState), "case {case}");
        } else {
            let response = result.unwrap();
            let complete = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
            assert_eq!(complete.buy_policy, policy);
            assert_eq!(complete.creation_stage, LbpCreationStage::Complete);
        }
    }
}

#[test]
fn weighted_creation_rejects_wrong_policy_version_namespace_or_pool() {
    for case in 0..3 {
        let mut terms = allowlisted_inputs();
        let curve_core::BuyPolicy::Allowlist(policy) = &mut terms.buy_policy else {
            unreachable!()
        };
        match case {
            0 => policy.version = 2,
            1 => policy.namespace = [99; 32],
            _ => policy.sale_policy_id = [99; 32],
        }
        assert_eq!(
            handle(&creation(terms)).err(),
            Some(Error::InvalidTerms),
            "case {case}"
        );
    }
}

#[test]
fn weighted_creation_requires_a_supported_ending_and_a_valid_schedule() {
    use factory_core::EndingPolicy;
    for ending in [
        EndingPolicy::Manual,
        EndingPolicy::Scheduled,
        EndingPolicy::Depletion,
        EndingPolicy::Either,
    ] {
        let mut terms = inputs();
        terms.ending_policy = ending;
        let result = handle(&creation(terms));
        if matches!(ending, EndingPolicy::Scheduled | EndingPolicy::Either) {
            assert!(result.is_ok());
        } else {
            assert_eq!(result.err(), Some(Error::InvalidTerms));
        }
    }
    for case in 0..2 {
        let mut terms = inputs();
        if case == 0 {
            terms.start_timestamp = u64::MAX;
        } else {
            terms.end_timestamp = u64::MAX;
        }
        assert_eq!(handle(&creation(terms)).err(), Some(Error::InvalidTerms));
    }
}

#[test]
fn weighted_policy_choices_survive_setup_and_pool_mismatches_reject_funding() {
    let (_, prepared, bytes, setup) = prepared(CreatorAllocationRelease::AfterEnd);
    let state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    assert_eq!(state.ending_policy, factory_core::EndingPolicy::Scheduled);
    assert_eq!(pool.ending_policy, state.ending_policy);
    for case in 0..3 {
        let mut changed = pool.clone();
        match case {
            0 => changed.ending_policy = factory_core::EndingPolicy::Manual,
            1 => changed.ending_policy = factory_core::EndingPolicy::Depletion,
            _ => {}
        }
        let result = advance(
            &setup,
            &prepared,
            vec![
                borsh::to_vec(&changed).unwrap(),
                holding(state.pool_id, state.token_definition_id, 0),
                holding(state.pool_id, state.collateral_definition_id, 0),
                holding(id(42), state.collateral_definition_id, 100),
            ],
        );
        if case < 2 {
            assert_eq!(result.err(), Some(Error::InvalidCustody));
        } else {
            let complete =
                LbpLaunchState::decode(result.unwrap().post_state.as_ref().unwrap()).unwrap();
            assert_eq!(complete.creation_stage, LbpCreationStage::Complete);
            assert_eq!(complete.ending_policy, state.ending_policy);
        }
    }
}

#[test]
fn weighted_malformed_saved_policies_cannot_resume_or_forward_payouts() {
    let input = creation(inputs());
    let response = handle(&input).unwrap();
    let setup = Creation::new(&input, &response);
    let state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    for case in 0..3 {
        let mut changed = state.clone();
        match case {
            0 => changed.ending_policy = factory_core::EndingPolicy::Manual,
            1 => changed.ending_policy = factory_core::EndingPolicy::Depletion,
            _ => changed.end_timestamp = None,
        }
        let bytes = borsh::to_vec(&changed).unwrap();
        assert_eq!(
            LbpLaunchState::decode(&bytes).err(),
            Some(Error::InvalidState)
        );
        let mut next = setup.namespace_reply(&response, config());
        next.pre_state = bytes.into();
        assert_eq!(handle(&next).err(), Some(Error::InvalidState));
        changed.creation_stage = LbpCreationStage::Complete;
        next.pre_state = borsh::to_vec(&changed).unwrap().into();
        next.message =
            borsh::to_vec(&Message::WithdrawWeightedProceeds { creator: id(42) }).unwrap();
        assert_eq!(handle(&next).err(), Some(Error::InvalidState));
    }
}

#[test]
fn weighted_created_event_names_the_launch_and_original_seed_once() {
    let input = creation(inputs());
    let response = handle(&input).unwrap();
    let state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        launch_events(&response),
        vec![SaleEventV1::new(
            *state.namespace.value(),
            PricingEngineV1::Weighted,
            EventSubjectV1::Launch {
                factory: *input.receiver.account_id.value(),
                pool: *state.pool_id.value()
            },
            SaleEventKindV1::Created {
                creator: *id(42).value(),
                creator_role: *state.creator_role_id.value(),
                token_definition: *state.token_definition_id.value(),
                collateral_definition: *state.collateral_definition_id.value(),
                graduation_reserve: state.graduation_reserve,
                sale_inventory: 800,
                creator_allocation: 200,
                graduation_policy: state.graduation_policy,
                collateral_seed: 100,
                start_timestamp: Some(150),
                end_timestamp: Some(300),
                policies: EventPoliciesV1 {
                    ending: state.ending_policy,
                    pause: state.pause_policy,
                    sell_fee_bps: state.sell_fee_bps
                }
            }
        )]
    );
    let (_, minted, _) = minted(CreatorAllocationRelease::AfterEnd);
    assert!(minted.events.is_empty());
    let mut replay = input;
    replay.pre_state = minted.post_state.unwrap();
    assert_eq!(handle(&replay).err(), Some(Error::AlreadyInitialized));
}

#[test]
fn weighted_trade_mode_survives_setup_and_rejects_substitution() {
    for mode in [
        factory_core::TradeMode::BuyOnly,
        factory_core::TradeMode::BuyAndSell,
    ] {
        let mut terms = inputs();
        terms.trade_mode = mode;
        let (_, prepared, bytes, setup) = prepared_with_terms(terms);
        let state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(state.trade_mode, mode);
        let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
        assert_eq!(pool.trade_mode, mode);
        for pool_mode in [
            factory_core::TradeMode::BuyOnly,
            factory_core::TradeMode::BuyAndSell,
        ] {
            let mut pool = pool.clone();
            pool.trade_mode = pool_mode;
            let result = advance(
                &setup,
                &prepared,
                vec![
                    borsh::to_vec(&pool).unwrap(),
                    holding(state.pool_id, state.token_definition_id, 0),
                    holding(state.pool_id, state.collateral_definition_id, 0),
                    holding(id(42), state.collateral_definition_id, 100),
                ],
            );
            if pool_mode != mode {
                assert_eq!(result.err(), Some(Error::InvalidCustody));
            } else {
                let complete =
                    LbpLaunchState::decode(result.unwrap().post_state.as_ref().unwrap()).unwrap();
                assert_eq!(complete.trade_mode, mode);
                assert_eq!(complete.creation_stage, LbpCreationStage::Complete);
            }
        }
    }
}

#[test]
fn weighted_sell_fee_survives_setup_and_rejects_substitution() {
    for rate in [0, 125, 9999] {
        let mut terms = inputs();
        terms.sell_fee_bps = rate;
        let (_, prepared, bytes, setup) = prepared_with_terms(terms);
        let state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(state.sell_fee_bps, rate);
        let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
        assert_eq!(pool.sell_fee_bps, rate);
        for pool_rate in [0, 125, 9999] {
            let mut pool = pool.clone();
            pool.sell_fee_bps = pool_rate;
            let result = advance(
                &setup,
                &prepared,
                vec![
                    borsh::to_vec(&pool).unwrap(),
                    holding(state.pool_id, state.token_definition_id, 0),
                    holding(state.pool_id, state.collateral_definition_id, 0),
                    holding(id(42), state.collateral_definition_id, 100),
                ],
            );
            if pool_rate != rate {
                assert_eq!(result.err(), Some(Error::InvalidCustody));
            } else {
                let complete =
                    LbpLaunchState::decode(result.unwrap().post_state.as_ref().unwrap()).unwrap();
                assert_eq!(complete.sell_fee_bps, rate);
                assert_eq!(complete.creation_stage, LbpCreationStage::Complete);
            }
        }
    }
}

#[test]
fn weighted_funding_rejects_either_substituted_graduation_percentage() {
    let mut terms = inputs();
    terms.graduation_hook = Some(factory_core::GraduationHook {
        program_id: id(9),
        configuration: vec![],
    });
    terms.graduation_policy = factory_core::GraduationPolicy {
        launch_bps: 2500,
        collateral_bps: 6000,
    };
    let (_, prepared, bytes, setup) = prepared_with_terms(terms.clone());
    let state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    assert_eq!(pool.graduation_policy, terms.graduation_policy);
    for launch in [true, false] {
        let mut changed = pool.clone();
        if launch {
            changed.graduation_policy.launch_bps += 1;
        } else {
            changed.graduation_policy.collateral_bps += 1;
        }
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
    }
}
