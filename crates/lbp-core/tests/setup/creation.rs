use super::fixture::{
    activation, definition, exact_in, funded, holding, id, namespace_state, prepare, prepared,
    reply, state_of,
};
use lbp_core::{FixedPrice, Message, compute_pool_pda, handle};
use lee_core::account::{AccountId, Actor};

#[test]
fn preparation_checks_the_saved_namespace_through_its_actor() {
    let input = prepare();
    let response = handle(&input).unwrap();
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(curve_core::compute_config_pda(id(40), id(6)), id(6))
    );
    assert_eq!(
        borsh::from_slice::<curve_core::Message>(&response.sync_messages[0].message).unwrap(),
        curve_core::Message::ReadNamespace { namespace: id(40) }
    );
}

#[test]
fn setup_accepts_only_the_saved_factory_program_and_pda() {
    for from in [
        None,
        Some(Actor::new(id(99), id(7))),
        Some(Actor::new(prepare().from.unwrap().account_id, id(99))),
    ] {
        let mut input = prepare();
        input.from = from;
        assert!(matches!(handle(&input), Err(lbp_core::Error::Unauthorized)));
    }
}

#[test]
fn preparation_requires_fresh_canonical_pool_and_positive_distinct_terms() {
    for case in 0..8 {
        let mut input = prepare();
        let Message::PrepareFixedPrice { mut inputs } = borsh::from_slice(&input.message).unwrap()
        else {
            unreachable!()
        };
        let expected = match case {
            0 => {
                input.pre_state = vec![1].into();
                lbp_core::Error::AlreadyInitialized
            }
            1 => {
                input.receiver.account_id = id(99);
                lbp_core::Error::WrongAccount
            }
            2 => {
                inputs.sale_inventory = 0;
                lbp_core::Error::InvalidTerms
            }
            3 => {
                inputs.price.collateral_units = 0;
                lbp_core::Error::InvalidTerms
            }
            4 => {
                inputs.price.token_units = 0;
                lbp_core::Error::InvalidTerms
            }
            5 => {
                inputs.end_timestamp = Some(100);
                lbp_core::Error::InvalidTerms
            }
            6 => {
                inputs.collateral_definition_id = inputs.token_definition_id;
                lbp_core::Error::InvalidTerms
            }
            _ => {
                inputs.namespace = AccountId::default();
                lbp_core::Error::InvalidTerms
            }
        };
        input.message = borsh::to_vec(&Message::PrepareFixedPrice { inputs }).unwrap();
        assert!(
            matches!(handle(&input),Err(e) if e==expected),
            "case {case}"
        );
    }
}

#[test]
fn namespace_reply_must_come_from_the_saved_program_and_config_address() {
    let input = prepare();
    let response = handle(&input).unwrap();
    let source = response.sync_messages[0].to;
    for from in [
        Actor::new(id(99), source.program_account_id),
        Actor::new(source.account_id, id(99)),
    ] {
        assert!(matches!(
            handle(&reply(&input, &response, from, namespace_state())),
            Err(lbp_core::Error::Unauthorized)
        ));
    }
    let accepted = handle(&reply(&input, &response, source, namespace_state())).unwrap();
    assert_eq!(
        accepted.sync_messages[0].to,
        Actor::new(id(41), curve_core::TOKEN_PROGRAM_ACCOUNT_ID)
    );
}

#[test]
fn preparation_saves_pending_terms_and_initializes_canonical_vaults() {
    let (input, response) = prepared();
    let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(!state.funded);
    assert_eq!(state.recorded_collateral, 0);
    assert_eq!(
        (state.start_timestamp, state.end_timestamp),
        (100, Some(200))
    );
    assert_eq!((state.sale_inventory, state.creator_allocation), (800, 200));
    assert_eq!(
        state.price,
        FixedPrice {
            collateral_units: 3,
            token_units: 2
        }
    );
    assert_eq!(state.factory_owner, prepare().from.unwrap().account_id);
    assert_eq!(
        input.receiver.account_id,
        compute_pool_pda(id(40), id(8), id(41), id(42), state.factory_owner)
    );
    assert!(response.async_messages.is_empty());
}

#[test]
fn preparation_rejects_nonzero_inventory_or_collateral_in_pool_vaults() {
    for slot in [4, 5] {
        for amount in [1, 800, u128::MAX] {
            let mut input = prepare();
            let mut response = handle(&input).unwrap();
            let values = [
                namespace_state(),
                definition(),
                definition(),
                holding(input.from.unwrap().account_id, id(41), 800),
                holding(
                    input.receiver.account_id,
                    id(41),
                    if slot == 4 { amount } else { 0 },
                ),
                holding(
                    input.receiver.account_id,
                    id(42),
                    if slot == 5 { amount } else { 0 },
                ),
            ];
            for (index, bytes) in values.into_iter().enumerate() {
                let source = response.sync_messages.last().unwrap().to;
                input = reply(&input, &response, source, bytes);
                let result = handle(&input);
                if index == slot {
                    assert!(matches!(result, Err(lbp_core::Error::InvalidCustody)));
                    break;
                }
                response = result.unwrap();
            }
        }
    }
}

#[test]
fn activation_funds_exact_inventory_and_keeps_donations_outside_recorded_collateral() {
    for donation in [0, 1, 800, u128::MAX] {
        let (mut input, response) = activation(donation);
        let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert!(state.funded);
        assert_eq!(state.recorded_collateral, 0);
        assert_eq!(state.sale_inventory, 800);
        input.pre_state = response.post_state.unwrap();
        input.from = prepare().from;
        input.message = borsh::to_vec(&Message::ActivateFixedPrice).unwrap();
        assert!(matches!(
            handle(&input),
            Err(lbp_core::Error::AlreadyFunded)
        ));
    }
}

#[test]
fn activation_requires_zero_recorded_collateral_and_exact_credited_inventory() {
    let (mut input, response) = prepared();
    let mut state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    state.recorded_collateral = 1;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.from = prepare().from;
    input.message = borsh::to_vec(&Message::ActivateFixedPrice).unwrap();
    assert!(matches!(
        handle(&input),
        Err(lbp_core::Error::RecordedCollateral)
    ));
    for (source, vault) in [(1, 800), (800, 0), (0, 799), (0, 801), (0, u128::MAX)] {
        let (mut input, response) = prepared();
        input.pre_state = response.post_state.unwrap();
        input.from = prepare().from;
        input.message = borsh::to_vec(&Message::ActivateFixedPrice).unwrap();
        let mut response = handle(&input).unwrap();
        for (index, bytes) in [
            namespace_state(),
            definition(),
            definition(),
            holding(input.from.unwrap().account_id, id(41), source),
            holding(input.receiver.account_id, id(41), vault),
        ]
        .into_iter()
        .enumerate()
        {
            let from = response.sync_messages.last().unwrap().to;
            input = reply(&input, &response, from, bytes);
            let result = handle(&input);
            if index == if source != 0 { 3 } else { 4 } {
                assert!(matches!(result, Err(lbp_core::Error::InvalidCustody)));
                break;
            }
            response = result.unwrap();
        }
    }
}

#[test]
fn setup_rejects_substituted_token_actor_sources_and_invalid_canonical_holdings() {
    for slot in 1..6 {
        let mut input = prepare();
        let mut response = handle(&input).unwrap();
        let values = [
            namespace_state(),
            definition(),
            definition(),
            holding(input.from.unwrap().account_id, id(41), 800),
            holding(input.receiver.account_id, id(41), 0),
            holding(input.receiver.account_id, id(42), 0),
        ];
        for (index, bytes) in values.into_iter().enumerate() {
            let source = response.sync_messages.last().unwrap().to;
            input = reply(&input, &response, source, bytes);
            if index == slot {
                for from in [
                    Actor::new(id(99), source.program_account_id),
                    Actor::new(source.account_id, id(99)),
                ] {
                    let mut wrong = input.clone();
                    wrong.from = Some(from);
                    assert!(matches!(handle(&wrong), Err(lbp_core::Error::Unauthorized)));
                }
                let invalid = if slot < 3 {
                    borsh::to_vec(&lez_compat_core::token::TokenState::Definition {
                        name: "NFT".into(),
                        kind: lez_compat_core::token::TokenKind::NftMaster,
                        supply: 1,
                        metadata: None,
                    })
                    .unwrap()
                } else {
                    holding(id(99), id(99), 0)
                };
                let wrong = reply(&input, &response, source, invalid);
                assert!(handle(&wrong).is_err());
            }
            response = handle(&input).unwrap();
        }
    }
}

#[test]
fn setup_requires_inclusion_before_the_saved_end() {
    for (_, response) in [prepared(), activation(17)] {
        assert_eq!(response.validity.timestamps.start(), None);
        assert_eq!(response.validity.timestamps.end(), Some(200_000));
    }
}

#[test]
fn fixed_price_policy_binding_is_checked_before_setup_queries_and_on_state_reads() {
    for case in 0..3 {
        let mut input = prepare();
        let Message::PrepareFixedPrice { mut inputs } = borsh::from_slice(&input.message).unwrap()
        else {
            unreachable!()
        };
        inputs.buy_policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
            version: if case == 0 { 2 } else { 1 },
            namespace: if case == 1 {
                [99; 32]
            } else {
                *inputs.namespace.value()
            },
            sale_policy_id: if case == 2 {
                [99; 32]
            } else {
                *input.receiver.account_id.value()
            },
            root: [9; 32],
        });
        input.message = borsh::to_vec(&Message::PrepareFixedPrice {
            inputs: inputs.clone(),
        })
        .unwrap();
        assert!(
            matches!(handle(&input), Err(lbp_core::Error::InvalidTerms)),
            "case {case}"
        );
        let (_, mut state) = funded();
        state.buy_policy = inputs.buy_policy;
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.message = borsh::to_vec(&Message::ReadState).unwrap();
        assert!(
            matches!(handle(&input), Err(lbp_core::Error::InvalidTerms)),
            "stored case {case}"
        );
    }
}

#[test]
fn fixed_policy_matrix_rejects_unsupported_terms_before_setup_queries() {
    use lbp_core::EndingPolicy;
    for ending in [
        EndingPolicy::Manual,
        EndingPolicy::Scheduled,
        EndingPolicy::Depletion,
        EndingPolicy::Either,
    ] {
        for end in [None, Some(200)] {
            let mut input = prepare();
            let Message::PrepareFixedPrice { mut inputs } =
                borsh::from_slice(&input.message).unwrap()
            else {
                unreachable!()
            };
            inputs.ending_policy = ending;
            inputs.end_timestamp = end;
            input.message = borsh::to_vec(&Message::PrepareFixedPrice { inputs }).unwrap();
            let result = handle(&input);
            if (ending == EndingPolicy::Depletion && end.is_none())
                || (matches!(ending, EndingPolicy::Scheduled | EndingPolicy::Either)
                    && end.is_some())
            {
                let state = state_of(&result.unwrap());
                assert_eq!(state.ending_policy, ending);
                assert_eq!(state.end_timestamp, end);
            } else {
                assert_eq!(result.err(), Some(lbp_core::Error::InvalidTerms));
            }
        }
    }
}

#[test]
fn fixed_preparation_checks_both_timestamp_conversions_before_queries() {
    let last = u64::MAX / 1000;
    for (start, end, valid) in [
        (0, None, true),
        (last, None, true),
        (last - 1, Some(last), true),
        (last + 1, None, false),
        (0, Some(last + 1), false),
        (100, Some(100), false),
        (101, Some(100), false),
    ] {
        let mut input = prepare();
        let Message::PrepareFixedPrice { mut inputs } = borsh::from_slice(&input.message).unwrap()
        else {
            unreachable!()
        };
        inputs.start_timestamp = start;
        inputs.end_timestamp = end;
        inputs.ending_policy = if end.is_some() {
            lbp_core::EndingPolicy::Either
        } else {
            lbp_core::EndingPolicy::Depletion
        };
        input.message = borsh::to_vec(&Message::PrepareFixedPrice { inputs }).unwrap();
        let result = handle(&input);
        if valid {
            assert!(result.is_ok());
        } else {
            assert_eq!(result.err(), Some(lbp_core::Error::InvalidTerms));
        }
    }
}

#[test]
fn malformed_fixed_policies_cannot_bypass_pool_entry_or_setup_replies() {
    for case in 0..5 {
        let (mut input, state) = funded();
        let mut malformed = state.clone();
        match case {
            0 => malformed.ending_policy = lbp_core::EndingPolicy::Manual,
            1 => malformed.ending_policy = lbp_core::EndingPolicy::Depletion,
            2 => malformed.ending_policy = lbp_core::EndingPolicy::Manual,
            3 => malformed.start_timestamp = u64::MAX,
            _ => malformed.end_timestamp = Some(100),
        }
        let expected = if case == 0 || case == 2 {
            lbp_core::Error::InvalidState
        } else {
            lbp_core::Error::InvalidTerms
        };
        input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
        input.pre_state = borsh::to_vec(&malformed).unwrap().into();
        for message in [
            Message::ReadState,
            Message::ActivateFixedPrice,
            exact_in(5),
            Message::BuyExactOutput {
                membership: None,
                amount_out: 3,
                max_amount_in: 6,
                now: 100,
            },
            Message::SetPaused { paused: false },
            Message::SettleFixedPrice {
                creator: id(60),
                now: 200,
            },
            Message::CollectFees,
        ] {
            input.message = borsh::to_vec(&message).unwrap();
            assert_eq!(handle(&input).err(), Some(expected));
        }
        let setup = prepare();
        let response = handle(&setup).unwrap();
        let mut pending = state_of(&response);
        pending.ending_policy = lbp_core::EndingPolicy::Manual;

        let mut callback = reply(
            &setup,
            &response,
            response.sync_messages[0].to,
            namespace_state(),
        );
        callback.pre_state = borsh::to_vec(&pending).unwrap().into();
        assert_eq!(handle(&callback).err(), Some(lbp_core::Error::InvalidState));
    }
}

#[test]
fn fixed_pool_preparation_validates_saved_graduation_percentages() {
    for has_adapter in [false, true] {
        for policy in [
            lbp_core::GraduationPolicy::default(),
            lbp_core::GraduationPolicy {
                launch_bps: 10_000,
                collateral_bps: 0,
            },
            lbp_core::GraduationPolicy {
                launch_bps: 0,
                collateral_bps: 10_000,
            },
            lbp_core::GraduationPolicy {
                launch_bps: 10_001,
                collateral_bps: 0,
            },
            lbp_core::GraduationPolicy {
                launch_bps: 0,
                collateral_bps: 10_001,
            },
        ] {
            let mut input = prepare();
            let Message::PrepareFixedPrice { mut inputs } =
                borsh::from_slice(&input.message).unwrap()
            else {
                unreachable!()
            };
            inputs.graduation_adapter = has_adapter.then(|| Actor::new(id(70), id(71)));
            inputs.graduation_policy = policy;
            input.message = borsh::to_vec(&Message::PrepareFixedPrice { inputs }).unwrap();
            let result = handle(&input);
            if policy.validate(has_adapter).is_err() {
                assert_eq!(result.err(), Some(lbp_core::Error::InvalidTerms));
            } else {
                assert_eq!(state_of(&result.unwrap()).graduation_policy, policy);
            }
        }
    }
}
