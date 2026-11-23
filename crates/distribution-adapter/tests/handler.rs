use distribution_adapter::{Configuration, Rule, State, VERSION};
use factory_core::*;
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response, StateReply},
};
use lez_compat_core::token::{TokenKind, TokenState};
fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}
fn configuration() -> Configuration {
    Configuration {
        version: VERSION,
        launch_rules: vec![
            Rule {
                destination: id(80),
                bps: 5000,
            },
            Rule {
                destination: id(81),
                bps: 3000,
            },
            Rule {
                destination: id(80),
                bps: 2000,
            },
        ],
        collateral_rules: vec![Rule {
            destination: id(81),
            bps: 10000,
        }],
    }
}
fn definition() -> TokenState {
    TokenState::Definition {
        name: "Test".into(),
        kind: TokenKind::Fungible,
        supply: 1000,
        metadata: None,
    }
}
fn initialization(kind: GraduationIntentKind) -> ReceiveInput {
    let factory = compute_factory_pda(id(40), id(7), [3; 32]);
    ReceiveInput {
        receiver: Actor::new(compute_graduation_pda(id(9), factory), id(9)),
        from: None,
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&AdapterMessage::Initialize {
            factory_program_id: id(7),
            namespace: id(40),
            launch_salt: [3; 32],
            accepted_intent: kind,
        })
        .unwrap(),
    }
}
fn factory_state(kind: GraduationIntentKind) -> Vec<u8> {
    let hook = Some(GraduationHook {
        program_id: id(9),
        configuration: borsh::to_vec(&configuration()).unwrap(),
    });
    let message = match kind {
        GraduationIntentKind::Curve => Message::CreateFactoryPool {
            buy_policy: curve_core::BuyPolicy::Open,
            pause_policy: PausePolicy::Disabled,
            ending_policy: EndingPolicy::Scheduled,
            trade_mode: 0,
            sell_fee_bps: 0,
            per_block_token_limit: None,
            graduation_hook_program_id: Some(id(9)),
            graduation_hook_configuration: borsh::to_vec(&configuration()).unwrap(),
            namespace: id(40),
            launch_salt: [3; 32],
            name: "Launch".into(),
            uri: "uri".into(),
            graduation_reserve: 0,
            sale_reserve: 800,
            graduation_policy: GraduationPolicy::default(),
            creator_allocation: 100,
            virtual_token_reserve: 1_000,
            virtual_collateral_reserve: 100,
            end_timestamp: Some(400),
            curve_program_id: id(6),
            collateral_definition_id: id(41),
            creator: id(42),
            now: 100,
        },
        GraduationIntentKind::FixedPrice => Message::CreateFixedPriceLbp {
            inputs: FixedPriceInputs {
                per_block_token_limit: None,
                buy_policy: curve_core::BuyPolicy::Open,
                pause_policy: PausePolicy::Disabled,
                ending_policy: EndingPolicy::Scheduled,
                trade_mode: TradeMode::BuyOnly,
                sell_fee_bps: 0,
                graduation_hook: hook,
                graduation_policy: GraduationPolicy::default(),
                namespace: id(40),
                launch_salt: [3; 32],
                name: "Launch".into(),
                uri: "uri".into(),
                graduation_reserve: 0,
                sale_inventory: 800,
                creator_allocation: 100,
                price: FixedPrice {
                    collateral_units: 3,
                    token_units: 2,
                },
                start: SaleStart::Now { timestamp: 100 },
                end_timestamp: Some(200),
                allocation_release: CreatorAllocationRelease::AfterEnd,
                namespace_program_id: id(6),
                lbp_program_id: id(8),
                collateral_definition_id: id(41),
                creator: id(42),
                now: 100,
            },
        },
        GraduationIntentKind::Weighted => Message::CreateWeightedLbp {
            inputs: WeightedInputs {
                per_block_token_limit: None,
                buy_policy: curve_core::BuyPolicy::Open,
                pause_policy: PausePolicy::Disabled,
                ending_policy: EndingPolicy::Scheduled,
                trade_mode: TradeMode::BuyOnly,
                sell_fee_bps: 0,
                graduation_hook: hook,
                graduation_policy: GraduationPolicy::default(),
                namespace: id(40),
                launch_salt: [3; 32],
                name: "Launch".into(),
                uri: "uri".into(),
                graduation_reserve: 0,
                sale_inventory: 800,
                creator_allocation: 100,
                collateral_seed: 100,
                token_weight_start: lbp_core::WEIGHT_TOTAL / 2,
                token_weight_end: lbp_core::WEIGHT_TOTAL / 2,
                start_timestamp: 150,
                end_timestamp: 300,
                allocation_release: CreatorAllocationRelease::AfterEnd,
                namespace_program_id: id(6),
                lbp_program_id: id(8),
                collateral_definition_id: id(41),
                creator: id(42),
            },
        },
    };
    let mut input = ReceiveInput {
        receiver: Actor::new(compute_factory_pda(id(40), id(7), [3; 32]), id(7)),
        from: None,
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&message).unwrap(),
    };
    let mut response = handle(&input).unwrap();
    let config = curve_core::handle(&ReceiveInput {
        receiver: Actor::new(curve_core::compute_config_pda(id(40), id(6)), id(6)),
        from: Some(Actor::new(id(40), id(6))),
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&curve_core::Message::InitializeNamespace {
            namespace: id(40),
            protocol_fee_bps: 100,
            treasury: id(44),
        })
        .unwrap(),
    })
    .unwrap()
    .post_state
    .unwrap()
    .to_vec();
    for value in [config, borsh::to_vec(&definition()).unwrap()] {
        input = factory_reply(&input, &response, value);
        response = handle(&input).unwrap();
    }
    while response
        .post_state
        .as_ref()
        .is_some_and(|state| state.starts_with(b"LZCLK001"))
    {
        assert_eq!(kind, GraduationIntentKind::Weighted);
        assert_eq!(response.sync_messages.len(), 1);
        assert_eq!(response.sync_messages[0].to, clock_core::clock_actor());
        assert_eq!(
            borsh::from_slice::<clock_core::Message>(&response.sync_messages[0].message).unwrap(),
            clock_core::Message::ReadTimestamp { granularity: 1 }
        );
        response = handle(&ReceiveInput {
            receiver: input.receiver,
            from: Some(clock_core::clock_actor()),
            is_authorized: false,
            pre_state: response.post_state.unwrap(),
            message: borsh::to_vec(&clock_core::Reply::Timestamp(100_000)).unwrap(),
        })
        .unwrap();
    }
    let state = response.post_state.unwrap();
    let read = handle(&ReceiveInput {
        receiver: input.receiver,
        from: Some(initialization(kind).receiver),
        is_authorized: false,
        pre_state: state.clone(),
        message: borsh::to_vec(&Message::ReadState).unwrap(),
    })
    .unwrap();
    assert_eq!(
        borsh::from_slice::<StateReply>(&read.sync_messages[0].message)
            .unwrap()
            .state,
        state
    );
    state.to_vec()
}

fn factory_reply(input: &ReceiveInput, response: &Response, state: Vec<u8>) -> ReceiveInput {
    ReceiveInput {
        receiver: input.receiver,
        from: Some(response.sync_messages[0].to),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&StateReply {
            state: state.into(),
        })
        .unwrap(),
    }
}

fn initialized(kind: GraduationIntentKind) -> ReceiveInput {
    let mut input = initialization(kind);
    let response = distribution_adapter::handle(&input).unwrap();
    let reply = factory_reply(&input, &response, factory_state(kind));
    let response = distribution_adapter::handle(&reply).unwrap();
    input.pre_state = response.post_state.unwrap();
    input
}

#[test]
fn initialization_binds_each_sale_kind_configuration_assets_and_public_projection() {
    for kind in [
        GraduationIntentKind::Curve,
        GraduationIntentKind::FixedPrice,
        GraduationIntentKind::Weighted,
    ] {
        let mut input = initialized(kind);
        let state = State::decode(&input.pre_state).unwrap();
        assert_eq!(state.configuration, configuration());
        assert_eq!(state.configuration.hash(), state.configuration_hash);
        assert_eq!(state.accepted_intent, kind);
        assert_eq!(
            state.definitions,
            [compute_definition_pda(id(40), id(7), [3; 32]), id(41)]
        );
        assert!(state.is_idle());
        input.from = Some(state.factory);
        input.message = borsh::to_vec(&AdapterMessage::ReadState).unwrap();
        let response = distribution_adapter::handle(&input).unwrap();
        let reply: StateReply = borsh::from_slice(&response.sync_messages[0].message).unwrap();
        let public = AdapterState::decode(&reply.state).unwrap();
        assert_eq!(public.factory, state.factory);
        assert_eq!(public.accepted_intent, kind);
        assert!(public.receipt.is_none());
    }
}

#[test]
fn initialization_rejects_forged_reply_wrong_kind_and_invalid_config() {
    let input = initialization(GraduationIntentKind::Curve);
    let pending = distribution_adapter::handle(&input).unwrap();
    let correct = factory_reply(&input, &pending, factory_state(GraduationIntentKind::Curve));
    let mut forged = correct.clone();
    forged.from = Some(Actor::new(id(99), id(7)));
    assert_eq!(
        distribution_adapter::handle(&forged).err(),
        Some(distribution_adapter::Error::Unauthorized)
    );
    let wrong_kind = factory_reply(
        &input,
        &pending,
        factory_state(GraduationIntentKind::Weighted),
    );
    assert_eq!(
        distribution_adapter::handle(&wrong_kind).err(),
        Some(distribution_adapter::Error::WrongIntent)
    );
    let mut saved = FactoryState::decode(
        &borsh::from_slice::<StateReply>(&correct.message)
            .unwrap()
            .state,
    )
    .unwrap();
    saved.graduation_hook.as_mut().unwrap().configuration = vec![99];
    let invalid = factory_reply(&input, &pending, borsh::to_vec(&saved).unwrap());
    assert_eq!(
        distribution_adapter::handle(&invalid).err(),
        Some(distribution_adapter::Error::InvalidConfiguration)
    );
    assert!(distribution_adapter::handle(&correct).is_ok());
}

#[test]
fn pre_graduation_distribution_is_rejected_and_request_terms_cannot_change() {
    let mut input = initialized(GraduationIntentKind::Curve);
    input.message = borsh::to_vec(&distribution_adapter::Message::DistributeRemaining).unwrap();
    assert_eq!(
        distribution_adapter::handle(&input).err(),
        Some(distribution_adapter::Error::NotGraduated)
    );
    let state = State::decode(&input.pre_state).unwrap();
    let request = GraduationRequest {
        factory_program_id: state.factory.program_account_id,
        namespace: state.namespace,
        launch_salt: state.launch_salt,
        token_definition_id: state.definitions[0],
        collateral_definition_id: state.definitions[1],
        intent: GraduationIntent::Curve {
            launch_tokens: 0,
            collateral: 0,
        },
        reserve_tokens: 0,
        unsold_tokens: 0,
        configuration: borsh::to_vec(&configuration()).unwrap(),
    };
    for fault in 0..7 {
        let mut value = request.clone();
        input.from = Some(state.factory);
        match fault {
            0 => input.from = None,
            1 => value.namespace = id(98),
            2 => value.launch_salt = [98; 32],
            3 => value.token_definition_id = id(98),
            4 => value.collateral_definition_id = id(98),
            5 => value.configuration.push(0),
            _ => {
                value.intent = GraduationIntent::Weighted {
                    launch_tokens: 0,
                    collateral: 0,
                }
            }
        }
        input.message = borsh::to_vec(&AdapterMessage::Graduate { request: value }).unwrap();
        assert!(distribution_adapter::handle(&input).is_err());
    }
    input.from = Some(state.factory);
    input.message = borsh::to_vec(&AdapterMessage::Graduate { request }).unwrap();
    let pending = distribution_adapter::handle(&input).unwrap();
    input.pre_state = pending.post_state.unwrap();
    input.message = borsh::to_vec(&distribution_adapter::Message::DistributeRemaining).unwrap();
    assert!(distribution_adapter::handle(&input).is_err());
}

#[test]
fn serialized_initialized_state_has_an_independent_wire_fixture() {
    let input = initialized(GraduationIntentKind::Curve);
    let state = State::decode(&input.pre_state).unwrap();
    let mut bytes = vec![1]; // Stored::Ready
    bytes.extend(state.factory.account_id.value());
    bytes.extend(state.factory.program_account_id.value());
    bytes.extend([40; 32]);
    bytes.extend([3; 32]);
    bytes.push(0); // Curve
    bytes.extend(state.definitions[0].value());
    bytes.extend([41; 32]);
    bytes.extend(borsh::to_vec(&configuration()).unwrap()); // Independently checked by planner wire fixture.
    bytes.extend(state.configuration_hash);
    bytes.push(0); // No graduation receipt.
    bytes.extend([0; 32]); // Both actual graduation amounts.
    bytes.extend([0; 8]); // Distribution count.
    bytes.push(0); // Idle.
    assert_eq!(input.pre_state.as_ref(), bytes);
}

#[test]
fn token_replies_require_the_expected_actor_asset_owner_kind_and_coverage() {
    let mut input = initialized(GraduationIntentKind::Curve);
    let state = State::decode(&input.pre_state).unwrap();
    input.from = Some(state.factory);
    input.message = borsh::to_vec(&AdapterMessage::Graduate {
        request: GraduationRequest {
            factory_program_id: state.factory.program_account_id,
            namespace: state.namespace,
            launch_salt: state.launch_salt,
            token_definition_id: state.definitions[0],
            collateral_definition_id: state.definitions[1],
            intent: GraduationIntent::Curve {
                launch_tokens: 100,
                collateral: 0,
            },
            reserve_tokens: 0,
            unsold_tokens: 100,
            configuration: borsh::to_vec(&state.configuration).unwrap(),
        },
    })
    .unwrap();
    let mut response = distribution_adapter::handle(&input).unwrap();
    for _ in 0..2 {
        let correct = factory_reply(&input, &response, borsh::to_vec(&definition()).unwrap());
        let mut forged = correct.clone();
        forged.from = None;
        assert_eq!(
            distribution_adapter::handle(&forged).err(),
            Some(distribution_adapter::Error::Unauthorized)
        );
        let mut nft = definition();
        if let TokenState::Definition { kind, .. } = &mut nft {
            *kind = TokenKind::NftMaster;
        }
        assert!(
            distribution_adapter::handle(&factory_reply(
                &input,
                &response,
                borsh::to_vec(&nft).unwrap()
            ))
            .is_err()
        );
        response = distribution_adapter::handle(&correct).unwrap();
        input = correct;
    }
    for fault in 0..4 {
        let holding = TokenState::Holding {
            owner: if fault == 0 {
                id(99)
            } else {
                input.receiver.account_id
            },
            definition: if fault == 1 {
                id(99)
            } else {
                state.definitions[0]
            },
            kind: if fault == 2 {
                TokenKind::NftMaster
            } else {
                TokenKind::Fungible
            },
            balance: if fault == 3 { 99 } else { 100 },
        };
        let reply = factory_reply(&input, &response, borsh::to_vec(&holding).unwrap());
        assert_eq!(
            distribution_adapter::handle(&reply).err(),
            Some(distribution_adapter::Error::InvalidCustody)
        );
    }
    assert!(
        State::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .receipt
            .is_none()
    );
}
