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
            ready,
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
        dex_seed_reserve: 100,
        collateral_amount: 50,
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
    assert_eq!(response.sync_messages.last().unwrap().to, factory);
}
#[test]
fn adapter_initialization_binds_canonical_custody_to_the_factory_actor() {
    let (input, response) = initialized(true);
    let state = AdapterState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.factory,
        Actor::new(compute_factory_pda(id(40), id(7), [3; 32]), id(7))
    );
    assert!(state.ready);
    assert!(state.receipt.is_none());
    let mut wrong = input.clone();
    wrong.receiver.account_id = id(99);
    assert!(matches!(handle_adapter(&wrong), Err(Error::WrongAccount)));
}
