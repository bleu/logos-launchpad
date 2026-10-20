use lee::{
    AccountId, Actor, PrivateKey, PublicKey, PublicTransaction, V03State,
    program::Program,
    public_transaction::{Message, WitnessSet},
};
use lee_core::account::Nonce;
use lez_compat_core::token::{self, Message as TokenMessage, TokenKind, TokenState};
use std::{borrow::Cow, collections::BTreeMap};

fn submit(
    state: &mut V03State,
    program: AccountId,
    key: &PrivateKey,
    actors: Vec<Actor>,
    evidence: Vec<lee::PublicAccountEvidence>,
    body: TokenMessage,
) -> Result<(), lee::error::LeeError> {
    let owner = AccountId::from(&PublicKey::new_from_private_key(key));
    let nonce = state.get_account_by_id(owner).nonce;
    let mut message = Message::try_new(
        Actor::new(owner, program),
        actors,
        BTreeMap::from([(owner, nonce)]),
        body,
    )
    .unwrap();
    message.admission_evidence = evidence;
    let witnesses = WitnessSet::for_message(&message, &[key]);
    state
        .transition_from_public_transaction(&PublicTransaction::new(message, witnesses), 1, 0)
        .map(|_| ())
}

#[test]
fn transfer_executes_without_a_node_and_a_rejected_transfer_preserves_balances() {
    let guest = Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_TOKEN_ELF)).unwrap();
    let program = AccountId::from_builtin_program(guest.id());
    let creator_key = PrivateKey::try_new([1; 32]).unwrap();
    let alice_key = PrivateKey::try_new([2; 32]).unwrap();
    let definition = AccountId::from(&PublicKey::new_from_private_key(&creator_key));
    let alice = AccountId::from(&PublicKey::new_from_private_key(&alice_key));
    let bob = AccountId::new([3; 32]);
    let holding = |owner| Actor::new(token::holding_address(program, owner, definition), program);
    let mut state = V03State::new().with_programs([guest]);
    let evidence = |owner| lee::PublicAccountEvidence::Pda {
        program,
        seed: token::holding_seed(owner, definition),
    };
    submit(
        &mut state,
        program,
        &creator_key,
        vec![Actor::new(definition, program), holding(alice)],
        vec![evidence(alice)],
        TokenMessage::Create {
            name: "Fixture".into(),
            kind: TokenKind::Fungible,
            supply: 100,
            recipient: alice,
            metadata: None,
        },
    )
    .unwrap();
    let actors = vec![Actor::new(alice, program), holding(alice), holding(bob)];
    submit(
        &mut state,
        program,
        &alice_key,
        actors.clone(),
        vec![evidence(bob)],
        TokenMessage::Transfer {
            definition,
            recipient: bob,
            amount: 30,
        },
    )
    .unwrap();
    let balance = |state: &V03State, owner| match TokenState::decode(
        state
            .get_account_by_id(holding(owner).account_id)
            .data
            .actor_state(program),
    )
    .unwrap()
    {
        TokenState::Holding { balance, .. } => balance,
        _ => panic!("expected holding"),
    };
    assert_eq!((balance(&state, alice), balance(&state, bob)), (70, 30));
    let nonce = state.get_account_by_id(alice).nonce;
    let rejected = submit(
        &mut state,
        program,
        &alice_key,
        actors,
        vec![],
        TokenMessage::Transfer {
            definition,
            recipient: bob,
            amount: 71,
        },
    );
    assert!(
        matches!(rejected, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("InsufficientBalance"))
    );
    assert_eq!((balance(&state, alice), balance(&state, bob)), (70, 30));
    assert_eq!(state.get_account_by_id(alice).nonce, nonce);
    assert_ne!(nonce, Nonce(0));

    let incomplete_context = vec![Actor::new(alice, program), holding(alice)];
    let rejected = submit(
        &mut state,
        program,
        &alice_key,
        incomplete_context,
        vec![],
        TokenMessage::Transfer {
            definition,
            recipient: bob,
            amount: 10,
        },
    );
    assert!(
        matches!(rejected, Err(lee::error::LeeError::InvalidProgramBehavior(
        lee::error::InvalidProgramBehaviorError::Execution(lee_core::execution_state::ExecutionError::UndeclaredActor { actor })
    )) if actor == holding(bob))
    );
    assert_eq!((balance(&state, alice), balance(&state, bob)), (70, 30));
    assert_eq!(state.get_account_by_id(alice).nonce, nonce);
}
