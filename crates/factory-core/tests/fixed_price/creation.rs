use super::*;

#[test]
fn creation_rejects_a_factory_account_outside_the_namespace_and_salt() {
    let mut input = creation();
    input.receiver.account_id = id(99);
    assert!(matches!(handle(&input), Err(Error::WrongAccount)));
}

#[test]
fn creation_rejects_invalid_or_reused_sale_terms() {
    for case in 0..10 {
        let mut value = inputs();
        let mut input = creation();
        let error = match case {
            0 => {
                input.pre_state = vec![1].into();
                Error::AlreadyInitialized
            }
            1 => {
                value.sale_inventory = 0;
                Error::InvalidTerms
            }
            2 => {
                value.creator_allocation = u128::MAX;
                Error::Overflow
            }
            3 => {
                value.price.collateral_units = 0;
                Error::InvalidTerms
            }
            4 => {
                value.price.token_units = 0;
                Error::InvalidTerms
            }
            5 => {
                value.name.clear();
                Error::InvalidTerms
            }
            6 => {
                value.uri.clear();
                Error::InvalidTerms
            }
            7 => {
                value.end_timestamp = Some(0);
                Error::InvalidTerms
            }
            8 => {
                value.creator = AccountId::default();
                Error::InvalidTerms
            }
            _ => {
                value.collateral_definition_id =
                    factory_core::compute_definition_pda(id(40), id(7), [3; 32]);
                Error::InvalidTerms
            }
        };
        input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: value }).unwrap();
        assert!(matches!(handle(&input),Err(e) if e==error), "case {case}");
    }
}

#[test]
fn scheduled_creation_allows_inclusion_at_the_saved_start_millisecond() {
    let mut input = creation();
    let mut value = inputs();
    value.start = SaleStart::At { timestamp: 150 };
    input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: value }).unwrap();
    let (_, response, _) = mint_from(&input);
    assert_eq!(response.validity.timestamps.start(), None);
    assert_eq!(response.validity.timestamps.end(), Some(150_001));
    assert!(response.validity.timestamps.is_valid_for(150_000));
    assert!(!response.validity.timestamps.is_valid_for(150_001));
}

#[test]
fn creation_intersects_saved_start_with_optional_setup_end() {
    for (start, end_timestamp, lower, upper) in [
        (
            SaleStart::Now { timestamp: 0 },
            Some(200),
            Some(0),
            Some(200_000),
        ),
        (
            SaleStart::Now { timestamp: 150 },
            Some(200),
            Some(150_000),
            Some(200_000),
        ),
        (SaleStart::Now { timestamp: 150 }, None, Some(150_000), None),
        (SaleStart::At { timestamp: 0 }, Some(200), None, Some(1)),
        (
            SaleStart::At { timestamp: 150 },
            Some(151),
            None,
            Some(150_001),
        ),
        (SaleStart::At { timestamp: 150 }, None, None, Some(150_001)),
    ] {
        let mut input = creation();
        let mut terms = inputs();
        terms.start = start;
        terms.end_timestamp = end_timestamp;
        terms.ending_policy = if end_timestamp.is_some() {
            factory_core::EndingPolicy::Either
        } else {
            factory_core::EndingPolicy::Depletion
        };
        input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
        let (_, response, _) = mint_from(&input);
        assert_eq!(response.validity.timestamps.start(), lower);
        assert_eq!(response.validity.timestamps.end(), upper);
    }
}

#[test]
fn creation_rejects_timestamp_conversion_overflow_before_minting() {
    for (start, end_timestamp) in [
        (
            SaleStart::Now {
                timestamp: u64::MAX,
            },
            None,
        ),
        (
            SaleStart::At {
                timestamp: u64::MAX,
            },
            None,
        ),
        (SaleStart::Now { timestamp: 0 }, Some(u64::MAX)),
    ] {
        let mut input = creation();
        let mut terms = inputs();
        terms.start = start;
        terms.end_timestamp = end_timestamp;
        terms.ending_policy = if end_timestamp.is_some() {
            factory_core::EndingPolicy::Either
        } else {
            factory_core::EndingPolicy::Depletion
        };
        input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
        assert_eq!(handle(&input).err(), Some(Error::InvalidTerms));
    }
}

#[test]
fn optional_end_must_be_strictly_after_the_saved_start() {
    for start in [
        SaleStart::Now { timestamp: 100 },
        SaleStart::At { timestamp: 100 },
    ] {
        for (end_timestamp, valid) in [
            (None, true),
            (Some(99), false),
            (Some(100), false),
            (Some(101), true),
        ] {
            let mut input = creation();
            let mut terms = inputs();
            terms.start = start;
            terms.end_timestamp = end_timestamp;
            terms.ending_policy = if end_timestamp.is_some() {
                factory_core::EndingPolicy::Either
            } else {
                factory_core::EndingPolicy::Depletion
            };
            input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
            let result = handle(&input);
            if valid {
                assert!(result.is_ok());
            } else {
                assert!(matches!(result, Err(Error::InvalidTerms)));
            }
        }
    }
}

#[test]
fn creation_mints_exact_supply_with_metadata_and_a_separate_creator_role() {
    let (input, response, _) = minted();
    let state =
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.total_supply, 1000);
    assert_eq!(state.start_timestamp, 0);
    assert_eq!(
        state.creation_stage,
        factory_core::FixedPriceCreationStage::Minted
    );
    assert_ne!(state.creator_role_id, input.receiver.account_id);
    let mint = response
        .sync_messages
        .iter()
        .find(|m| m.to.account_id == state.token_definition_id)
        .unwrap();
    assert_eq!(
        borsh::from_slice::<lez_compat_core::token::Message>(&mint.message).unwrap(),
        lez_compat_core::token::Message::Create {
            name: "Launch".into(),
            kind: lez_compat_core::token::TokenKind::Fungible,
            supply: 1000,
            recipient: factory_core::compute_mint_pda(id(40), id(7), [3; 32]),
            metadata: Some(lez_compat_core::token::Metadata {
                standard: lez_compat_core::token::MetadataStandard::Simple,
                uri: "https://example.test/launch.json".into(),
                creators: String::new()
            })
        }
    );
    let role = response
        .sync_messages
        .iter()
        .find(|m| m.to.account_id == state.creator_role_id)
        .unwrap();
    assert_eq!(
        borsh::from_slice::<lez_compat_core::authority::Message>(&role.message).unwrap(),
        lez_compat_core::authority::Message::Bootstrap { authority: id(42) }
    );
    assert_eq!(response.validity.timestamps.start(), Some(0));
    assert_eq!(response.validity.timestamps.end(), Some(200_000));
}

#[test]
fn fixed_price_creation_saves_the_initial_buy_policy() {
    let mut input = creation();
    let mut terms = inputs();
    terms.buy_policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *terms.namespace.value(),
        sale_policy_id: *lbp_core::compute_pool_pda(
            terms.namespace,
            terms.lbp_program_id,
            factory_core::compute_definition_pda(terms.namespace, id(7), terms.launch_salt),
            terms.collateral_definition_id,
            input.receiver.account_id,
        )
        .value(),
        root: [9; 32],
    });
    input.message = borsh::to_vec(&Message::CreateFixedPriceLbp {
        inputs: terms.clone(),
    })
    .unwrap();
    let response = handle(&input).unwrap();
    assert_eq!(
        factory_core::FixedPriceLaunchState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        terms.buy_policy
    );
}

#[test]
fn fixed_price_creation_rejects_unbound_or_unsupported_buy_policy() {
    for case in 0..3 {
        let mut input = creation();
        let mut terms = inputs();
        terms.buy_policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
            version: if case == 0 { 2 } else { 1 },
            namespace: if case == 1 {
                [99; 32]
            } else {
                *terms.namespace.value()
            },
            sale_policy_id: if case == 2 {
                [99; 32]
            } else {
                *lbp_core::compute_pool_pda(
                    terms.namespace,
                    terms.lbp_program_id,
                    factory_core::compute_definition_pda(terms.namespace, id(7), terms.launch_salt),
                    terms.collateral_definition_id,
                    input.receiver.account_id,
                )
                .value()
            },
            root: [9; 32],
        });
        input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
        assert_eq!(
            handle(&input).err(),
            Some(Error::InvalidTerms),
            "case {case}"
        );
    }
}

#[test]
fn fixed_price_creation_checks_every_ending_and_deadline_combination() {
    use factory_core::EndingPolicy;
    for ending in [
        EndingPolicy::Manual,
        EndingPolicy::Scheduled,
        EndingPolicy::Depletion,
        EndingPolicy::Either,
    ] {
        for end in [None, Some(200)] {
            let mut terms = inputs();
            terms.ending_policy = ending;
            terms.end_timestamp = end;
            let mut input = creation();
            input.message = borsh::to_vec(&Message::CreateFixedPriceLbp { inputs: terms }).unwrap();
            let result = handle(&input);
            if (ending == EndingPolicy::Depletion && end.is_none())
                || (matches!(ending, EndingPolicy::Scheduled | EndingPolicy::Either)
                    && end.is_some())
            {
                let state = factory_core::LbpLaunchState::decode(
                    result.unwrap().post_state.as_ref().unwrap(),
                )
                .unwrap();
                assert_eq!(state.ending_policy, ending);
            } else {
                assert_eq!(result.err(), Some(Error::InvalidTerms));
            }
        }
    }
}

#[test]
fn fixed_created_event_names_the_launch_and_saved_terms_once() {
    let input = creation();
    let response = handle(&input).unwrap();
    let state =
        factory_core::LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        launch_events(&response),
        vec![SaleEventV1::new(
            *state.namespace.value(),
            PricingEngineV1::FixedPrice,
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
                collateral_seed: 0,
                start_timestamp: Some(0),
                end_timestamp: Some(200),
                policies: EventPoliciesV1 {
                    ending: state.ending_policy,
                    pause: state.pause_policy,
                    sell_fee_bps: state.sell_fee_bps
                }
            }
        )]
    );
    let (_, minted, _) = minted();
    assert!(minted.events.is_empty());
    let mut replay = input;
    replay.pre_state = minted.post_state.unwrap();
    assert_eq!(handle(&replay).err(), Some(Error::AlreadyInitialized));
}
