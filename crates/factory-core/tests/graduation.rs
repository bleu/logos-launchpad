use factory_core::*;
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response, StateReply},
};
use lez_compat_core::token::{TokenKind, TokenState};
fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}
fn initialized(ready: bool) -> (ReceiveInput, Response) {
    let factory = compute_factory_pda(id(40), id(7), [3; 32]);
    let input = ReceiveInput {
        receiver: Actor::new(compute_graduation_pda(id(9), factory), id(9)),
        from: None,
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&AdapterMessage::Initialize {
            factory_program_id: id(7),
            namespace: id(40),
            launch_salt: [3; 32],
            accepted_intent: ready.then_some(GraduationIntentKind::CurveReserves),
        })
        .unwrap(),
    };
    let response = handle_adapter(&input).unwrap();
    (input, response)
}
fn request() -> GraduationRequest {
    GraduationRequest {
        factory_program_id: id(7),
        namespace: id(40),
        launch_salt: [3; 32],
        token_definition_id: compute_definition_pda(id(40), id(7), [3; 32]),
        collateral_definition_id: id(41),
        intent: GraduationIntent::CurveReserves {
            launch_tokens: 100,
            collateral: 50,
        },
        configuration: b"route".to_vec(),
    }
}
#[test]
fn callback_accepts_only_the_saved_factory_and_ready_canonical_custody() {
    let (mut input, initialized) = initialized(true);
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
    let (mut input, initialized) = initialized(true);
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
    let (input, response) = initialized(true);
    let state = AdapterState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.factory,
        Actor::new(compute_factory_pda(id(40), id(7), [3; 32]), id(7))
    );
    assert_eq!(
        state.accepted_intent,
        Some(GraduationIntentKind::CurveReserves)
    );
    assert!(state.receipt.is_none());
    let mut wrong = input.clone();
    wrong.receiver.account_id = id(99);
    assert!(matches!(handle_adapter(&wrong), Err(Error::WrongAccount)));
}

fn initialized_kind(kind: Option<GraduationIntentKind>) -> (ReceiveInput, Response) {
    let (mut input, _) = initialized(true);
    input.message = borsh::to_vec(&AdapterMessage::Initialize {
        factory_program_id: id(7),
        namespace: id(40),
        launch_salt: [3; 32],
        accepted_intent: kind,
    })
    .unwrap();
    let response = handle_adapter(&input).unwrap();
    (input, response)
}

fn begin_intent(intent: GraduationIntent) -> (ReceiveInput, Response, GraduationRequest) {
    let (mut input, initialized) = initialized_kind(Some(intent.kind()));
    let mut request = request();
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
        GraduationIntent::CurveReserves {
            launch_tokens: 101,
            collateral: 50,
        },
        GraduationIntent::CurveReserves {
            launch_tokens: 100,
            collateral: 51,
        },
        GraduationIntent::FixedPriceProceeds { collateral: 50 },
        GraduationIntent::WeightedBuyerProceeds { collateral: 50 },
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
    for intent in [
        GraduationIntent::FixedPriceProceeds { collateral: 50 },
        GraduationIntent::WeightedBuyerProceeds { collateral: 50 },
    ] {
        let original = GraduationRequest {
            intent,
            ..base.clone()
        };
        let mut changed = original.clone();
        changed.intent = match intent {
            GraduationIntent::FixedPriceProceeds { .. } => {
                GraduationIntent::FixedPriceProceeds { collateral: 51 }
            }
            _ => GraduationIntent::WeightedBuyerProceeds { collateral: 51 },
        };
        assert_ne!(original.hash(), changed.hash());
    }
}

#[test]
fn readiness_can_activate_once_for_each_intent_kind() {
    let kinds = [
        None,
        Some(GraduationIntentKind::CurveReserves),
        Some(GraduationIntentKind::FixedPriceProceeds),
        Some(GraduationIntentKind::WeightedBuyerProceeds),
    ];
    for kind in kinds.into_iter().flatten() {
        let (mut input, response) = initialized_kind(None);
        input.pre_state = response.post_state.unwrap();
        let mut expected = AdapterState::decode(&input.pre_state).unwrap();
        input.message = borsh::to_vec(&AdapterMessage::Initialize {
            factory_program_id: id(7),
            namespace: id(40),
            launch_salt: [3; 32],
            accepted_intent: Some(kind),
        })
        .unwrap();
        assert!(input.from.is_none());
        assert!(!input.is_authorized);
        let response = handle_adapter(&input).unwrap();
        expected.accepted_intent = Some(kind);
        assert_eq!(
            AdapterState::decode(response.post_state.as_ref().unwrap()).unwrap(),
            expected
        );
        assert!(response.sync_messages.is_empty());
        input.pre_state = response.post_state.unwrap();
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
        }
    }
}

#[test]
fn readiness_activation_requires_the_saved_factory_and_canonical_custody() {
    let (mut input, response) = initialized_kind(None);
    input.pre_state = response.post_state.unwrap();
    input.message = borsh::to_vec(&AdapterMessage::Initialize {
        factory_program_id: id(7),
        namespace: id(40),
        launch_salt: [3; 32],
        accepted_intent: Some(GraduationIntentKind::CurveReserves),
    })
    .unwrap();
    let mut wrong = input.clone();
    wrong.receiver.account_id = id(99);
    assert!(matches!(handle_adapter(&wrong), Err(Error::WrongAccount)));
    let saved_factory = AdapterState::decode(&input.pre_state).unwrap().factory;
    for factory in [
        Actor::new(id(99), saved_factory.program_account_id),
        Actor::new(saved_factory.account_id, id(99)),
    ] {
        let mut state = AdapterState::decode(&input.pre_state).unwrap();
        state.factory = factory;
        wrong = input.clone();
        wrong.pre_state = borsh::to_vec(&state).unwrap().into();
        assert!(matches!(handle_adapter(&wrong), Err(Error::WrongAccount)));
    }
}

#[test]
fn readiness_accepts_only_the_explicit_intent_kind() {
    let intents = [
        GraduationIntent::CurveReserves {
            launch_tokens: 100,
            collateral: 50,
        },
        GraduationIntent::FixedPriceProceeds { collateral: 50 },
        GraduationIntent::WeightedBuyerProceeds { collateral: 50 },
    ];
    for kind in [
        None,
        Some(GraduationIntentKind::CurveReserves),
        Some(GraduationIntentKind::FixedPriceProceeds),
        Some(GraduationIntentKind::WeightedBuyerProceeds),
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
            if kind == Some(intent.kind()) {
                let response = result.unwrap();
                assert_eq!(response.sync_messages.len(), 1);
                assert!(response.sync_messages[0].pda_seeds.is_empty());
            } else {
                assert!(matches!(result, Err(Error::AdapterNotReady)));
            }
        }
    }
}

#[test]
fn every_intent_checks_both_canonical_holdings_and_returns_its_exact_request_hash() {
    for intent in [
        GraduationIntent::CurveReserves {
            launch_tokens: 100,
            collateral: 50,
        },
        GraduationIntent::CurveReserves {
            launch_tokens: 0,
            collateral: 0,
        },
        GraduationIntent::FixedPriceProceeds { collateral: 50 },
        GraduationIntent::FixedPriceProceeds { collateral: 0 },
        GraduationIntent::WeightedBuyerProceeds { collateral: 50 },
        GraduationIntent::WeightedBuyerProceeds { collateral: 0 },
    ] {
        let (mut input, mut response, request) = begin_intent(intent);
        let factory = input.from.unwrap();
        assert_eq!(
            intent.amounts().0,
            match intent {
                GraduationIntent::CurveReserves { launch_tokens, .. } => launch_tokens,
                _ => 0,
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
        assert_eq!(state.accepted_intent, Some(intent.kind()));
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
        for accepted_intent in [None, Some(intent.kind())] {
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
        GraduationIntent::CurveReserves {
            launch_tokens: 100,
            collateral: 50,
        },
        GraduationIntent::FixedPriceProceeds { collateral: 50 },
        GraduationIntent::WeightedBuyerProceeds { collateral: 50 },
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
    let (mut input, response, request) =
        begin_intent(GraduationIntent::FixedPriceProceeds { collateral: 0 });
    input.pre_state = response.post_state.unwrap();
    for message in [
        AdapterMessage::Graduate { request },
        AdapterMessage::Initialize {
            factory_program_id: id(7),
            namespace: id(40),
            launch_salt: [3; 32],
            accepted_intent: None,
        },
        AdapterMessage::Initialize {
            factory_program_id: id(7),
            namespace: id(40),
            launch_salt: [3; 32],
            accepted_intent: Some(GraduationIntentKind::FixedPriceProceeds),
        },
    ] {
        input.message = borsh::to_vec(&message).unwrap();
        assert!(matches!(handle_adapter(&input), Err(Error::Unauthorized)));
    }
}
