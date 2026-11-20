use factory_core::*;
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response, StateReply},
};
use lez_compat_core::token::{TokenKind, TokenState};
fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}
const KINDS: [GraduationIntentKind; 3] = [
    GraduationIntentKind::Curve,
    GraduationIntentKind::FixedPrice,
    GraduationIntentKind::Weighted,
];

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
        configuration: vec![],
    });
    let message = match kind {
        GraduationIntentKind::Curve => Message::CreateFactoryPool {
            buy_policy: curve_core::BuyPolicy::Open,
            pause_policy: PausePolicy::Disabled,
            ending_policy: EndingPolicy::Scheduled,
            trade_mode: 0,
            sell_fee_bps: 0,
            graduation_hook_program_id: Some(id(9)),
            graduation_hook_configuration: vec![],
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

fn initialized() -> (ReceiveInput, Response) {
    initialized_kind(GraduationIntentKind::Curve)
}
fn request() -> GraduationRequest {
    GraduationRequest {
        factory_program_id: id(7),
        namespace: id(40),
        launch_salt: [3; 32],
        token_definition_id: compute_definition_pda(id(40), id(7), [3; 32]),
        collateral_definition_id: id(41),
        intent: GraduationIntent::Curve {
            launch_tokens: 100,
            collateral: 50,
        },
        reserve_tokens: 0,
        unsold_tokens: 100,
        configuration: b"route".to_vec(),
    }
}
#[test]
fn callback_accepts_only_the_saved_factory_and_canonical_custody() {
    let (mut input, initialized) = initialized();
    let factory = AdapterState::decode(initialized.post_state.as_ref().unwrap())
        .unwrap()
        .factory;
    input.pre_state = initialized.post_state.unwrap();
    input.message = borsh::to_vec(&AdapterMessage::Graduate { request: request() }).unwrap();
    for source in [
        None,
        Some(Actor::new(id(99), factory.program_account_id)),
        Some(Actor::new(factory.account_id, id(99))),
    ] {
        input.from = source;
        assert!(matches!(handle_adapter(&input), Err(Error::Unauthorized)))
    }
    input.from = Some(factory);
    let response = handle_adapter(&input).unwrap();
    assert_eq!(
        response.sync_messages.last().unwrap().to,
        Actor::new(request().token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID)
    );
}
#[test]
fn adapter_authenticates_token_reads_and_acknowledges_only_credited_canonical_custody() {
    let (mut input, initialized) = initialized();
    let factory = AdapterState::decode(initialized.post_state.as_ref().unwrap())
        .unwrap()
        .factory;
    input.pre_state = initialized.post_state.unwrap();
    input.from = Some(factory);
    input.message = borsh::to_vec(&AdapterMessage::Graduate { request: request() }).unwrap();
    let mut response = handle_adapter(&input).unwrap();
    let definition = borsh::to_vec(&TokenState::Definition {
        name: "Token".into(),
        kind: TokenKind::Fungible,
        supply: 1000,
        metadata: None,
    })
    .unwrap();
    for bytes in [
        definition.clone(),
        definition,
        borsh::to_vec(&TokenState::Holding {
            owner: input.receiver.account_id,
            definition: request().token_definition_id,
            kind: TokenKind::Fungible,
            balance: 117,
        })
        .unwrap(),
        borsh::to_vec(&TokenState::Holding {
            owner: input.receiver.account_id,
            definition: id(41),
            kind: TokenKind::Fungible,
            balance: 67,
        })
        .unwrap(),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input.pre_state = response.post_state.unwrap();
        input.from = Some(Actor::new(source.account_id, id(99)));
        input.message = borsh::to_vec(&StateReply {
            state: bytes.into(),
        })
        .unwrap();
        assert!(matches!(handle_adapter(&input), Err(Error::Unauthorized)));
        input.from = Some(source);
        response = handle_adapter(&input).unwrap();
    }
    let state = AdapterState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.receipt, Some(request()));
    let reply = response.sync_messages.last().unwrap();
    assert_eq!(reply.to, factory);
    assert_eq!(
        borsh::from_slice::<GraduationResult>(&reply.message).unwrap(),
        GraduationResult {
            factory: factory.account_id,
            request_hash: request().hash()
        }
    );
    assert!(reply.pda_seeds.is_empty());
}
#[test]
fn adapter_initialization_binds_canonical_custody_to_the_factory_actor() {
    let (input, response) = initialized();
    let state = AdapterState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.factory,
        Actor::new(compute_factory_pda(id(40), id(7), [3; 32]), id(7))
    );
    assert_eq!(state.accepted_intent, GraduationIntentKind::Curve);
    assert!(state.receipt.is_none());
    let mut wrong = input.clone();
    wrong.receiver.account_id = id(99);
    assert!(matches!(handle_adapter(&wrong), Err(Error::WrongAccount)));
}

fn initialized_kind(kind: GraduationIntentKind) -> (ReceiveInput, Response) {
    let input = initialization(kind);
    let pending = handle_adapter(&input).unwrap();
    assert!(AdapterState::decode(pending.post_state.as_ref().unwrap()).is_err());
    assert_eq!(pending.sync_messages.len(), 1);
    assert_eq!(
        borsh::from_slice::<Message>(&pending.sync_messages[0].message).unwrap(),
        Message::ReadState
    );
    let response = handle_adapter(&factory_reply(&input, &pending, factory_state(kind))).unwrap();
    assert!(response.sync_messages.is_empty());
    (input, response)
}

fn begin_intent(intent: GraduationIntent) -> (ReceiveInput, Response, GraduationRequest) {
    let (mut input, initialized) = initialized_kind(intent.kind());
    let mut request = request();
    request.unsold_tokens = intent.amounts().0;
    request.intent = intent;
    input.pre_state = initialized.post_state.unwrap();
    input.from = Some(AdapterState::decode(&input.pre_state).unwrap().factory);
    input.message = borsh::to_vec(&AdapterMessage::Graduate {
        request: request.clone(),
    })
    .unwrap();
    let response = handle_adapter(&input).unwrap();
    (input, response, request)
}

fn token_reply(
    input: &mut ReceiveInput,
    response: Response,
    value: TokenState,
) -> Result<Response, Error> {
    input.from = Some(response.sync_messages.last().unwrap().to);
    input.pre_state = response.post_state.unwrap();
    input.message = borsh::to_vec(&StateReply {
        state: borsh::to_vec(&value).unwrap().into(),
    })
    .unwrap();
    handle_adapter(input)
}

fn definition() -> TokenState {
    TokenState::Definition {
        name: "Token".into(),
        kind: TokenKind::Fungible,
        supply: 1000,
        metadata: None,
    }
}

#[test]
fn request_hash_binds_each_field_and_each_typed_asset_intent() {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let base = request();
    let bytes = borsh::to_vec(&base).unwrap();
    assert_eq!(base.hash().as_slice(), Impl::hash_bytes(&bytes).as_bytes());
    assert_eq!(
        base.hash(),
        borsh::from_slice::<GraduationRequest>(&bytes)
            .unwrap()
            .hash()
    );
    let mut changed = Vec::new();
    for field in 0..6 {
        let mut value = base.clone();
        match field {
            0 => value.factory_program_id = id(98),
            1 => value.namespace = id(98),
            2 => value.launch_salt = [98; 32],
            3 => value.token_definition_id = id(98),
            4 => value.collateral_definition_id = id(98),
            _ => value.configuration.push(98),
        }
        changed.push(value);
    }
    for intent in [
        GraduationIntent::Curve {
            launch_tokens: 101,
            collateral: 50,
        },
        GraduationIntent::Curve {
            launch_tokens: 100,
            collateral: 51,
        },
        GraduationIntent::FixedPrice {
            launch_tokens: 100,
            collateral: 50,
        },
        GraduationIntent::Weighted {
            launch_tokens: 100,
            collateral: 50,
        },
    ] {
        changed.push(GraduationRequest {
            intent,
            ..base.clone()
        });
    }
    for value in &changed {
        assert_ne!(base.hash(), value.hash());
    }
    for (index, value) in changed.iter().enumerate() {
        for other in changed.iter().skip(index + 1) {
            assert_ne!(value.hash(), other.hash());
        }
    }
    for kind in [
        GraduationIntentKind::Curve,
        GraduationIntentKind::FixedPrice,
        GraduationIntentKind::Weighted,
    ] {
        let intent = |launch_tokens, collateral| match kind {
            GraduationIntentKind::Curve => GraduationIntent::Curve {
                launch_tokens,
                collateral,
            },
            GraduationIntentKind::FixedPrice => GraduationIntent::FixedPrice {
                launch_tokens,
                collateral,
            },
            GraduationIntentKind::Weighted => GraduationIntent::Weighted {
                launch_tokens,
                collateral,
            },
        };
        let original = GraduationRequest {
            intent: intent(100, 50),
            ..base.clone()
        };
        for changed in [intent(101, 50), intent(100, 51)] {
            assert_ne!(
                original.hash(),
                GraduationRequest {
                    intent: changed,
                    ..base.clone()
                }
                .hash()
            );
        }
    }
}

#[test]
fn initialization_sets_each_intent_once_and_rejects_every_repeat() {
    let kinds = [
        GraduationIntentKind::Curve,
        GraduationIntentKind::FixedPrice,
        GraduationIntentKind::Weighted,
    ];
    for kind in kinds {
        let (mut input, response) = initialized_kind(kind);
        input.pre_state = response.post_state.unwrap();
        let expected = AdapterState::decode(&input.pre_state).unwrap();
        assert_eq!(expected.accepted_intent, kind);
        assert!(expected.receipt.is_none());
        for accepted_intent in kinds {
            input.message = borsh::to_vec(&AdapterMessage::Initialize {
                factory_program_id: id(7),
                namespace: id(40),
                launch_salt: [3; 32],
                accepted_intent,
            })
            .unwrap();
            assert!(matches!(
                handle_adapter(&input),
                Err(Error::AlreadyInitialized)
            ));
            assert_eq!(AdapterState::decode(&input.pre_state).unwrap(), expected);
        }
    }
}

#[test]
fn adapter_accepts_only_the_initialized_intent_kind() {
    let intents = [
        GraduationIntent::Curve {
            launch_tokens: 100,
            collateral: 50,
        },
        GraduationIntent::FixedPrice {
            launch_tokens: 100,
            collateral: 50,
        },
        GraduationIntent::Weighted {
            launch_tokens: 100,
            collateral: 50,
        },
    ];
    for kind in [
        GraduationIntentKind::Curve,
        GraduationIntentKind::FixedPrice,
        GraduationIntentKind::Weighted,
    ] {
        let (mut input, response) = initialized_kind(kind);
        input.pre_state = response.post_state.unwrap();
        assert!(matches!(
            handle_adapter(&input),
            Err(Error::AlreadyInitialized)
        ));
        input.from = Some(AdapterState::decode(&input.pre_state).unwrap().factory);
        for intent in intents {
            input.message = borsh::to_vec(&AdapterMessage::Graduate {
                request: GraduationRequest {
                    intent,
                    ..request()
                },
            })
            .unwrap();
            let result = handle_adapter(&input);
            if kind == intent.kind() {
                let response = result.unwrap();
                assert_eq!(response.sync_messages.len(), 1);
                assert!(response.sync_messages[0].pda_seeds.is_empty());
            } else {
                assert!(matches!(result, Err(Error::AdapterIntentMismatch)));
            }
        }
    }
}

#[test]
fn every_intent_checks_both_canonical_holdings_and_returns_its_exact_request_hash() {
    for intent in [
        GraduationIntent::Curve {
            launch_tokens: 100,
            collateral: 50,
        },
        GraduationIntent::Curve {
            launch_tokens: 0,
            collateral: 0,
        },
        GraduationIntent::FixedPrice {
            launch_tokens: 100,
            collateral: 50,
        },
        GraduationIntent::FixedPrice {
            launch_tokens: 0,
            collateral: 0,
        },
        GraduationIntent::Weighted {
            launch_tokens: 100,
            collateral: 50,
        },
        GraduationIntent::Weighted {
            launch_tokens: 0,
            collateral: 0,
        },
    ] {
        let (mut input, mut response, request) = begin_intent(intent);
        let factory = input.from.unwrap();
        assert_eq!(
            intent.amounts().0,
            match intent {
                GraduationIntent::Curve { launch_tokens, .. }
                | GraduationIntent::FixedPrice { launch_tokens, .. }
                | GraduationIntent::Weighted { launch_tokens, .. } => launch_tokens,
            }
        );
        response = token_reply(&mut input, response, definition()).unwrap();
        response = token_reply(&mut input, response, definition()).unwrap();
        for (definition, balance) in [
            (request.token_definition_id, intent.amounts().0),
            (request.collateral_definition_id, intent.amounts().1),
        ] {
            assert_eq!(
                response.sync_messages[0].to,
                Actor::new(
                    lez_compat_core::token::holding_address(
                        TOKEN_PROGRAM_ACCOUNT_ID,
                        input.receiver.account_id,
                        definition
                    ),
                    TOKEN_PROGRAM_ACCOUNT_ID
                )
            );
            let owner = input.receiver.account_id;
            response = token_reply(
                &mut input,
                response,
                TokenState::Holding {
                    owner,
                    definition,
                    kind: TokenKind::Fungible,
                    balance,
                },
            )
            .unwrap();
        }
        let state = AdapterState::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(state.receipt, Some(request.clone()));
        assert_eq!(state.accepted_intent, intent.kind());
        let reply = &response.sync_messages[0];
        assert_eq!(reply.to, factory);
        assert!(reply.pda_seeds.is_empty());
        assert_eq!(
            borsh::from_slice::<GraduationResult>(&reply.message).unwrap(),
            GraduationResult {
                factory: factory.account_id,
                request_hash: request.hash(),
            }
        );
        input.pre_state = response.post_state.unwrap();
        input.from = None;
        for accepted_intent in [
            GraduationIntentKind::Curve,
            GraduationIntentKind::FixedPrice,
            GraduationIntentKind::Weighted,
        ] {
            input.message = borsh::to_vec(&AdapterMessage::Initialize {
                factory_program_id: id(7),
                namespace: id(40),
                launch_salt: [3; 32],
                accepted_intent,
            })
            .unwrap();
            assert!(matches!(
                handle_adapter(&input),
                Err(Error::AlreadyInitialized)
            ));
            assert_eq!(
                AdapterState::decode(&input.pre_state).unwrap().receipt,
                Some(request.clone())
            );
        }
        input.from = Some(factory);
        input.message = borsh::to_vec(&AdapterMessage::Graduate { request }).unwrap();
        assert!(matches!(handle_adapter(&input), Err(Error::AlreadySettled)));
    }
}

#[test]
fn malformed_or_underfunded_holdings_fail_for_each_intent_without_a_receipt() {
    for intent in [
        GraduationIntent::Curve {
            launch_tokens: 100,
            collateral: 50,
        },
        GraduationIntent::FixedPrice {
            launch_tokens: 100,
            collateral: 50,
        },
        GraduationIntent::Weighted {
            launch_tokens: 100,
            collateral: 50,
        },
    ] {
        for side in 0..2 {
            let (mut input, mut response, request) = begin_intent(intent);
            response = token_reply(&mut input, response, definition()).unwrap();
            response = token_reply(&mut input, response, definition()).unwrap();
            if side == 1 {
                let owner = input.receiver.account_id;
                response = token_reply(
                    &mut input,
                    response,
                    TokenState::Holding {
                        owner,
                        definition: request.token_definition_id,
                        kind: TokenKind::Fungible,
                        balance: intent.amounts().0,
                    },
                )
                .unwrap();
            }
            let definition = if side == 0 {
                request.token_definition_id
            } else {
                request.collateral_definition_id
            };
            let amount = if side == 0 {
                intent.amounts().0
            } else {
                intent.amounts().1
            };
            let saved = response.post_state.clone().unwrap();
            let source = response.sync_messages[0].to;
            for fault in 0..4 {
                if fault == 3 && amount == 0 {
                    continue;
                }
                let value = TokenState::Holding {
                    owner: if fault == 0 {
                        id(99)
                    } else {
                        input.receiver.account_id
                    },
                    definition: if fault == 1 { id(99) } else { definition },
                    kind: TokenKind::Fungible,
                    balance: if fault == 3 { amount - 1 } else { amount },
                };
                input.pre_state = saved.clone();
                input.from = Some(if fault == 2 {
                    Actor::new(id(99), TOKEN_PROGRAM_ACCOUNT_ID)
                } else {
                    source
                });
                input.message = borsh::to_vec(&StateReply {
                    state: borsh::to_vec(&value).unwrap().into(),
                })
                .unwrap();
                let Err(error) = handle_adapter(&input) else {
                    panic!("invalid holding accepted");
                };
                if fault == 2 {
                    assert_eq!(error, Error::Unauthorized);
                } else {
                    assert_eq!(error, Error::InvalidCustody);
                }
                assert!(AdapterState::decode(&saved).unwrap().receipt.is_none());
            }
        }
    }
}

#[test]
fn pending_callback_cannot_be_replaced_by_another_adapter_message() {
    let (mut input, response, request) = begin_intent(GraduationIntent::FixedPrice {
        launch_tokens: 100,
        collateral: 0,
    });
    input.pre_state = response.post_state.unwrap();
    for message in [
        AdapterMessage::Graduate { request },
        AdapterMessage::Initialize {
            factory_program_id: id(7),
            namespace: id(40),
            launch_salt: [3; 32],
            accepted_intent: GraduationIntentKind::Curve,
        },
        AdapterMessage::Initialize {
            factory_program_id: id(7),
            namespace: id(40),
            launch_salt: [3; 32],
            accepted_intent: GraduationIntentKind::FixedPrice,
        },
    ] {
        input.message = borsh::to_vec(&message).unwrap();
        assert!(matches!(handle_adapter(&input), Err(Error::Unauthorized)));
    }
}

#[test]
fn malicious_kind_choice_fails_before_commit_and_correct_initialization_can_retry() {
    for actual in KINDS {
        for selected in KINDS {
            let input = initialization(selected);
            let pending = handle_adapter(&input).unwrap();
            let result = handle_adapter(&factory_reply(&input, &pending, factory_state(actual)));
            if selected == actual {
                assert_eq!(
                    AdapterState::decode(result.unwrap().post_state.as_ref().unwrap())
                        .unwrap()
                        .accepted_intent,
                    actual
                );
            } else {
                assert!(matches!(result, Err(Error::AdapterIntentMismatch)));
                assert!(input.pre_state.is_empty());
                let (_, response) = initialized_kind(actual);
                assert_eq!(
                    AdapterState::decode(response.post_state.as_ref().unwrap())
                        .unwrap()
                        .accepted_intent,
                    actual
                );
            }
        }
    }
}

#[test]
fn initialization_authenticates_the_exact_factory_actor_and_rejects_reentry() {
    for kind in KINDS {
        let input = initialization(kind);
        let pending = handle_adapter(&input).unwrap();
        let expected = pending.sync_messages[0].to;
        let correct = factory_reply(&input, &pending, factory_state(kind));
        for from in [
            None,
            Some(Actor::new(id(99), expected.program_account_id)),
            Some(Actor::new(expected.account_id, id(99))),
        ] {
            let mut forged = correct.clone();
            forged.from = from;
            assert!(matches!(handle_adapter(&forged), Err(Error::Unauthorized)));
        }
        for message in [
            AdapterMessage::ReadState,
            AdapterMessage::Initialize {
                factory_program_id: id(7),
                namespace: id(40),
                launch_salt: [3; 32],
                accepted_intent: kind,
            },
            AdapterMessage::Graduate { request: request() },
        ] {
            let mut reentry = correct.clone();
            reentry.message = borsh::to_vec(&message).unwrap();
            assert!(handle_adapter(&reentry).is_err());
        }
        let mut wrong_custody = correct.clone();
        wrong_custody.receiver.account_id = id(99);
        assert!(matches!(
            handle_adapter(&wrong_custody),
            Err(Error::WrongAccount)
        ));
        let completed = handle_adapter(&correct).unwrap();
        assert_eq!(
            AdapterState::decode(completed.post_state.as_ref().unwrap())
                .unwrap()
                .accepted_intent,
            kind
        );
    }
}

#[test]
fn initialization_rejects_uncreated_factory_and_wrong_saved_hook_or_sale_identity() {
    for kind in KINDS {
        let input = initialization(kind);
        let pending = handle_adapter(&input).unwrap();
        assert!(matches!(
            handle_adapter(&factory_reply(&input, &pending, vec![])),
            Err(Error::InvalidState)
        ));
        for fault in 0..5 {
            let saved = factory_state(kind);
            let wrong = if kind == GraduationIntentKind::Curve {
                let mut state = FactoryState::decode(&saved).unwrap();
                match fault {
                    0 => state.graduation_hook = None,
                    1 => state.graduation_hook.as_mut().unwrap().program_id = id(99),
                    2 => state.namespace = id(99),
                    3 => state.launch_salt = [99; 32],
                    _ => state.token_definition_id = id(99),
                }
                borsh::to_vec(&state).unwrap()
            } else {
                let mut state = LbpLaunchState::decode(&saved).unwrap();
                match fault {
                    0 => state.graduation_hook = None,
                    1 => state.graduation_hook.as_mut().unwrap().program_id = id(99),
                    2 => state.namespace = id(99),
                    3 => state.launch_salt = [99; 32],
                    _ => state.token_definition_id = id(99),
                }
                borsh::to_vec(&state).unwrap()
            };
            assert!(matches!(
                handle_adapter(&factory_reply(&input, &pending, wrong)),
                Err(Error::WrongAccount)
            ));
        }
        let completed =
            handle_adapter(&factory_reply(&input, &pending, factory_state(kind))).unwrap();
        assert_eq!(
            AdapterState::decode(completed.post_state.as_ref().unwrap())
                .unwrap()
                .accepted_intent,
            kind
        );
    }
}

#[test]
fn reserve_breakdown_is_checked_and_both_components_are_bound_to_the_hash() {
    let mut value = request();
    value.reserve_tokens = 40;
    value.unsold_tokens = 60;
    assert!(value.validate_amounts().is_ok());
    let original = value.hash();
    value.reserve_tokens = 41;
    value.unsold_tokens = 59;
    assert!(value.validate_amounts().is_ok());
    assert_ne!(value.hash(), original);
    value.unsold_tokens = 60;
    assert_eq!(value.validate_amounts(), Err(Error::InvalidTerms));
    value.reserve_tokens = u128::MAX;
    assert_eq!(value.validate_amounts(), Err(Error::InvalidTerms));
}
