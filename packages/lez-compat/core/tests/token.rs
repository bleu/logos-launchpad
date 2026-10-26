use lee_core::{
    account::{AccountId, Actor, ActorState},
    program::ReceiveInput,
};
use lez_compat_core::token::{self, Message, TokenKind, TokenState};

const PROGRAM: AccountId = AccountId::new([9; 32]);
const DEFINITION: AccountId = AccountId::new([1; 32]);
const ALICE: AccountId = AccountId::new([2; 32]);

fn receive(
    account: AccountId,
    state: ActorState,
    authorized: bool,
    message: Message,
) -> ReceiveInput {
    ReceiveInput {
        receiver: Actor::new(account, PROGRAM),
        from: None,
        is_authorized: authorized,
        pre_state: state,
        message: borsh::to_vec(&message).unwrap(),
    }
}

#[test]
fn creator_defines_an_exact_supply_and_credits_the_initial_holding() {
    let input = receive(
        DEFINITION,
        ActorState::empty(),
        true,
        Message::Create {
            name: "Launch".into(),
            kind: TokenKind::Fungible,
            supply: 1000,
            recipient: ALICE,
            metadata: None,
        },
    );
    let response = token::handle(&input).unwrap();
    let definition = TokenState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(matches!(
        definition,
        TokenState::Definition { supply: 1000, .. }
    ));
    let credit = &response.sync_messages[0];
    let holding = token::handle(&ReceiveInput {
        receiver: credit.to,
        from: Some(input.receiver),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: credit.message.clone(),
    })
    .unwrap();
    assert_eq!(
        TokenState::decode(holding.post_state.as_ref().unwrap()).unwrap(),
        TokenState::Holding {
            definition: DEFINITION,
            owner: ALICE,
            kind: TokenKind::Fungible,
            balance: 1000
        }
    );
}

#[test]
fn an_unsigned_account_cannot_create_a_definition() {
    let input = receive(
        DEFINITION,
        ActorState::empty(),
        false,
        Message::Create {
            name: "Launch".into(),
            kind: TokenKind::Fungible,
            supply: 1000,
            recipient: ALICE,
            metadata: None,
        },
    );
    assert!(matches!(
        token::handle(&input),
        Err(lez_compat_core::Error::Unauthorized)
    ));
}

#[test]
fn owner_transfers_tokens_between_associated_holdings() {
    let request = receive(
        ALICE,
        ActorState::empty(),
        true,
        Message::Transfer {
            definition: DEFINITION,
            recipient: AccountId::new([3; 32]),
            amount: 30,
        },
    );
    let debit = token::handle(&request).unwrap().sync_messages.remove(0);
    let pre_state = borsh::to_vec(&TokenState::Holding {
        definition: DEFINITION,
        owner: ALICE,
        kind: TokenKind::Fungible,
        balance: 100,
    })
    .unwrap()
    .into();
    let response = token::handle(&ReceiveInput {
        receiver: debit.to,
        from: Some(request.receiver),
        is_authorized: false,
        pre_state,
        message: debit.message,
    })
    .unwrap();
    assert!(matches!(
        TokenState::decode(response.post_state.as_ref().unwrap()).unwrap(),
        TokenState::Holding { balance: 70, .. }
    ));
    let credit = &response.sync_messages[0];
    let received = token::handle(&ReceiveInput {
        receiver: credit.to,
        from: Some(debit.to),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: credit.message.clone(),
    })
    .unwrap();
    assert!(matches!(
        TokenState::decode(received.post_state.as_ref().unwrap()).unwrap(),
        TokenState::Holding { balance: 30, .. }
    ));
}

#[test]
fn definition_authority_mints_checked_supply_into_an_associated_holding() {
    let state = borsh::to_vec(&TokenState::Definition {
        name: "Launch".into(),
        kind: TokenKind::Fungible,
        supply: 100,
        metadata: None,
    })
    .unwrap()
    .into();
    let input = receive(
        DEFINITION,
        state,
        true,
        Message::Mint {
            recipient: ALICE,
            amount: 20,
        },
    );
    let response = token::handle(&input).unwrap();
    assert!(matches!(
        TokenState::decode(response.post_state.as_ref().unwrap()).unwrap(),
        TokenState::Definition { supply: 120, .. }
    ));
    let credit = &response.sync_messages[0];
    let received = token::handle(&ReceiveInput {
        receiver: credit.to,
        from: Some(input.receiver),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: credit.message.clone(),
    })
    .unwrap();
    assert!(matches!(
        TokenState::decode(received.post_state.as_ref().unwrap()).unwrap(),
        TokenState::Holding { balance: 20, .. }
    ));
}

#[test]
fn owner_burns_tokens_and_reduces_the_definition_supply() {
    let request = receive(
        ALICE,
        ActorState::empty(),
        true,
        Message::Burn {
            definition: DEFINITION,
            amount: 30,
        },
    );
    let debit = token::handle(&request).unwrap().sync_messages.remove(0);
    let response = token::handle(&ReceiveInput {
        receiver: debit.to,
        from: Some(request.receiver),
        is_authorized: false,
        pre_state: borsh::to_vec(&TokenState::Holding {
            definition: DEFINITION,
            owner: ALICE,
            kind: TokenKind::Fungible,
            balance: 100,
        })
        .unwrap()
        .into(),
        message: debit.message,
    })
    .unwrap();
    assert!(matches!(
        TokenState::decode(response.post_state.as_ref().unwrap()).unwrap(),
        TokenState::Holding { balance: 70, .. }
    ));
    let burn = &response.sync_messages[0];
    let definition = token::handle(&ReceiveInput {
        receiver: burn.to,
        from: Some(debit.to),
        is_authorized: false,
        pre_state: borsh::to_vec(&TokenState::Definition {
            name: "Launch".into(),
            kind: TokenKind::Fungible,
            supply: 1000,
            metadata: None,
        })
        .unwrap()
        .into(),
        message: burn.message.clone(),
    })
    .unwrap();
    assert!(matches!(
        TokenState::decode(definition.post_state.as_ref().unwrap()).unwrap(),
        TokenState::Definition { supply: 970, .. }
    ));
}

#[test]
fn anyone_can_prepare_an_empty_holding_for_an_existing_definition() {
    let input = receive(
        DEFINITION,
        borsh::to_vec(&TokenState::Definition {
            name: "Launch".into(),
            kind: TokenKind::Fungible,
            supply: 100,
            metadata: None,
        })
        .unwrap()
        .into(),
        false,
        Message::Initialize { owner: ALICE },
    );
    let credit = token::handle(&input).unwrap().sync_messages.remove(0);
    let response = token::handle(&ReceiveInput {
        receiver: credit.to,
        from: Some(input.receiver),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: credit.message,
    })
    .unwrap();
    assert_eq!(
        TokenState::decode(response.post_state.as_ref().unwrap()).unwrap(),
        TokenState::Holding {
            definition: DEFINITION,
            owner: ALICE,
            kind: TokenKind::Fungible,
            balance: 0
        }
    );
}

#[test]
fn an_authorized_master_holder_can_prove_its_role() {
    let callback = Actor::new(AccountId::new([5; 32]), AccountId::new([6; 32]));
    let input = receive(
        ALICE,
        ActorState::empty(),
        true,
        Message::AssertNft {
            definition: DEFINITION,
            callback,
        },
    );
    let check = token::handle(&input).unwrap().sync_messages.remove(0);
    let response = token::handle(&ReceiveInput {
        receiver: check.to,
        from: Some(input.receiver),
        is_authorized: false,
        pre_state: borsh::to_vec(&TokenState::Holding {
            definition: DEFINITION,
            owner: ALICE,
            kind: TokenKind::NftMaster,
            balance: 1,
        })
        .unwrap()
        .into(),
        message: check.message,
    })
    .unwrap();
    let reply = &response.sync_messages[0];
    assert_eq!(reply.to, callback);
    assert_eq!(
        token::verify_nft_holder(
            &ReceiveInput {
                receiver: callback,
                from: Some(check.to),
                is_authorized: false,
                pre_state: ActorState::empty(),
                message: reply.message.clone()
            },
            PROGRAM,
            DEFINITION,
            ALICE
        ),
        Ok(())
    );
}

#[test]
fn definition_creation_keeps_the_metadata() {
    let metadata = token::Metadata {
        standard: token::MetadataStandard::Simple,
        uri: "ipfs://launch-metadata".into(),
        creators: "Creator".into(),
    };
    let input = receive(
        DEFINITION,
        ActorState::empty(),
        true,
        Message::Create {
            name: "Launch".into(),
            kind: TokenKind::Fungible,
            supply: 1000,
            recipient: ALICE,
            metadata: Some(metadata.clone()),
        },
    );
    let response = token::handle(&input).unwrap();
    assert!(
        matches!(TokenState::decode(response.post_state.as_ref().unwrap()).unwrap(),
        TokenState::Definition { metadata: Some(stored), .. } if stored == metadata)
    );
}

#[test]
fn a_credit_requires_the_token_program_and_the_matching_definition() {
    use lez_compat_core::Error;
    let mut input = receive(
        token::holding_address(PROGRAM, ALICE, DEFINITION),
        ActorState::empty(),
        false,
        Message::Credit {
            definition: DEFINITION,
            owner: ALICE,
            kind: TokenKind::Fungible,
            amount: 1,
        },
    );
    assert!(matches!(token::handle(&input), Err(Error::Unauthorized)));
    input.from = Some(Actor::new(DEFINITION, AccountId::new([7; 32])));
    assert!(matches!(token::handle(&input), Err(Error::Unauthorized)));
    input.from = Some(Actor::new(DEFINITION, PROGRAM));
    input.pre_state = borsh::to_vec(&TokenState::Holding {
        definition: AccountId::new([4; 32]),
        owner: ALICE,
        kind: TokenKind::Fungible,
        balance: 10,
    })
    .unwrap()
    .into();
    assert!(matches!(token::handle(&input), Err(Error::WrongDefinition)));
}

#[test]
fn minting_cannot_overflow_the_supply_or_create_another_nft_master() {
    use lez_compat_core::Error;
    let mut input = receive(
        DEFINITION,
        borsh::to_vec(&TokenState::Definition {
            name: "Launch".into(),
            kind: TokenKind::Fungible,
            supply: u128::MAX,
            metadata: None,
        })
        .unwrap()
        .into(),
        true,
        Message::Mint {
            recipient: ALICE,
            amount: 1,
        },
    );
    assert!(matches!(token::handle(&input), Err(Error::Overflow)));
    input.pre_state = borsh::to_vec(&TokenState::Definition {
        name: "Role".into(),
        kind: TokenKind::NftMaster,
        supply: 1,
        metadata: None,
    })
    .unwrap()
    .into();
    assert!(matches!(token::handle(&input), Err(Error::InvalidNft)));
}

#[test]
fn transferring_the_master_removes_the_former_holders_role() {
    let bob = AccountId::new([3; 32]);
    let source = token::holding_address(PROGRAM, ALICE, DEFINITION);
    let transfer = receive(
        ALICE,
        ActorState::empty(),
        true,
        Message::Transfer {
            definition: DEFINITION,
            recipient: bob,
            amount: 1,
        },
    );
    let debit = token::handle(&transfer).unwrap().sync_messages.remove(0);
    let sent = token::handle(&ReceiveInput {
        receiver: debit.to,
        from: Some(transfer.receiver),
        is_authorized: false,
        pre_state: borsh::to_vec(&TokenState::Holding {
            definition: DEFINITION,
            owner: ALICE,
            kind: TokenKind::NftMaster,
            balance: 1,
        })
        .unwrap()
        .into(),
        message: debit.message,
    })
    .unwrap();
    let credit = &sent.sync_messages[0];
    let received = token::handle(&ReceiveInput {
        receiver: credit.to,
        from: Some(Actor::new(source, PROGRAM)),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: credit.message.clone(),
    })
    .unwrap();
    for (owner, state, expected) in [
        (ALICE, sent.post_state.unwrap(), false),
        (bob, received.post_state.unwrap(), true),
    ] {
        let request = receive(
            owner,
            ActorState::empty(),
            true,
            Message::AssertNft {
                definition: DEFINITION,
                callback: Actor::new(owner, PROGRAM),
            },
        );
        let check = token::handle(&request).unwrap().sync_messages.remove(0);
        assert_eq!(
            token::handle(&ReceiveInput {
                receiver: check.to,
                from: Some(request.receiver),
                is_authorized: false,
                pre_state: state,
                message: check.message
            })
            .is_ok(),
            expected
        );
    }
}

#[test]
fn another_program_can_read_and_authenticate_token_state() {
    let reader = Actor::new(AccountId::new([4; 32]), AccountId::new([5; 32]));
    let mut request = receive(
        DEFINITION,
        borsh::to_vec(&TokenState::Definition {
            name: "Launch".into(),
            kind: TokenKind::Fungible,
            supply: 1000,
            metadata: None,
        })
        .unwrap()
        .into(),
        false,
        Message::ReadState,
    );
    request.from = Some(reader);
    let response = token::handle(&request).unwrap();
    let reply = &response.sync_messages[0];
    assert_eq!(reply.to, reader);
    let input = ReceiveInput {
        receiver: reader,
        from: Some(request.receiver),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: reply.message.clone(),
    };
    assert!(matches!(
        token::read_reply(&input, request.receiver).unwrap(),
        TokenState::Definition { supply: 1000, .. }
    ));
    assert!(token::read_reply(&input, Actor::new(DEFINITION, reader.program_account_id)).is_err());
}
