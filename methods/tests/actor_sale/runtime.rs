use super::*;

pub(super) fn test_graduation_policy(adapter: bool) -> factory_core::GraduationPolicy {
    if adapter {
        factory_core::GraduationPolicy {
            launch_bps: 2_500,
            collateral_bps: 6_000,
        }
    } else {
        factory_core::GraduationPolicy::default()
    }
}

pub(super) fn holding(owner: AccountId, definition: AccountId) -> Actor {
    Actor::new(
        token::holding_address(TOKEN_PROGRAM_ACCOUNT_ID, owner, definition),
        TOKEN_PROGRAM_ACCOUNT_ID,
    )
}

pub(super) fn account(actor: Actor, bytes: Vec<u8>) -> (AccountId, Account) {
    (
        actor.account_id,
        Account::default().with_actor_state(actor.program_account_id, bytes.into()),
    )
}

pub(super) fn balance(state: &V03State, owner: AccountId, definition: AccountId) -> u128 {
    match TokenState::decode(
        state
            .get_account_by_id(holding(owner, definition).account_id)
            .data
            .actor_state(TOKEN_PROGRAM_ACCOUNT_ID),
    )
    .unwrap()
    {
        TokenState::Holding { balance, .. } => balance,
        _ => panic!("expected holding"),
    }
}

pub(super) fn clock_state(state: V03State, timestamp: u64) -> V03State {
    state.with_public_accounts([account(
        clock_core::clock_actor(),
        clock_core::ClockAccountData {
            block_id: 1,
            timestamp,
        }
        .to_bytes(),
    )])
}

pub(super) fn fixed_transaction(
    state: &V03State,
    pool: AccountId,
    key: &PrivateKey,
    instruction: lbp_core::Message,
) -> PublicTransaction {
    let launch = fixed_pool(state, pool).token_definition_id;
    let buyer = AccountId::from(&PublicKey::new_from_private_key(key));
    let collect = matches!(instruction, lbp_core::Message::CollectFees);
    let receiver = Actor::new(if collect { pool } else { buyer }, LBP);
    let actors = vec![
        Actor::new(buyer, LBP),
        Actor::new(pool, LBP),
        Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
        Actor::new(buyer, TOKEN_PROGRAM_ACCOUNT_ID),
        Actor::new(pool, TOKEN_PROGRAM_ACCOUNT_ID),
        holding(pool, launch),
        holding(pool, COLLATERAL),
        holding(buyer, launch),
        holding(buyer, COLLATERAL),
        holding(TREASURY, COLLATERAL),
        holding(NEXT_TREASURY, COLLATERAL),
        Actor::new(
            clock_core::CLOCK_PROGRAM_ACCOUNT_ID,
            clock_core::clock_account_id(),
        ),
    ];
    let nonces = if collect {
        BTreeMap::new()
    } else {
        BTreeMap::from([(buyer, state.get_account_by_id(buyer).nonce)])
    };
    let instruction = if collect {
        instruction
    } else {
        lbp_core::Message::Execute {
            pool,
            instruction: Box::new(instruction),
        }
    };
    let mut message = Message::try_new(receiver, actors, nonces, instruction).unwrap();
    message
        .admission_evidence
        .push(lee::PublicAccountEvidence::Pda {
            program: TOKEN_PROGRAM_ACCOUNT_ID,
            seed: token::holding_seed(buyer, launch),
        });
    let keys = [key];
    let witnesses = WitnessSet::for_message(&message, if collect { &[] } else { &keys });
    PublicTransaction::new(message, witnesses)
}

pub(super) fn fixed_execute(
    state: &mut V03State,
    pool: AccountId,
    key: &PrivateKey,
    instruction: lbp_core::Message,
) -> Result<(), lee::error::LeeError> {
    fixed_execute_at(state, pool, key, instruction, 100_000)
}

pub(super) fn fixed_execute_at(
    state: &mut V03State,
    pool: AccountId,
    key: &PrivateKey,
    instruction: lbp_core::Message,
    timestamp: u64,
) -> Result<(), lee::error::LeeError> {
    fixed_execute_events_at(state, pool, key, instruction, timestamp).map(|_| ())
}

pub(super) fn fixed_execute_events_at(
    state: &mut V03State,
    pool: AccountId,
    key: &PrivateKey,
    instruction: lbp_core::Message,
    timestamp: u64,
) -> Result<Vec<lee_core::program::TransactionEvent>, lee::error::LeeError> {
    let transaction = fixed_transaction(state, pool, key, instruction);
    state.transition_from_public_transaction(&transaction, 1, timestamp)
}

pub(super) fn fixed_rejected(
    state: &mut V03State,
    pool: AccountId,
    key: &PrivateKey,
    instruction: lbp_core::Message,
    reason: &str,
) {
    let buyer = AccountId::from(&PublicKey::new_from_private_key(key));
    let before = state.clone();
    let nonce = state.get_account_by_id(buyer).nonce;
    let rejected = fixed_execute(state, pool, key, instruction);
    assert!(
        matches!(&rejected, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains(reason)),
        "expected {reason}, got {rejected:?}",
    );
    assert!(
        *state == before,
        "rejected fixed-price operation changed runtime state"
    );
    assert_eq!(state.get_account_by_id(buyer).nonce, nonce);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn factory_execute_at(
    state: &mut V03State,
    factory: AccountId,
    actors: &[Actor],
    key: &PrivateKey,
    salt: [u8; 32],
    launch: AccountId,
    pool: AccountId,
    instruction: factory_core::Message,
    timestamp: u64,
) -> Result<(), lee::error::LeeError> {
    factory_execute_events_at(
        state,
        factory,
        actors,
        key,
        salt,
        launch,
        pool,
        instruction,
        timestamp,
    )
    .map(|_| ())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn factory_execute_events_at(
    state: &mut V03State,
    factory: AccountId,
    actors: &[Actor],
    key: &PrivateKey,
    salt: [u8; 32],
    launch: AccountId,
    pool: AccountId,
    instruction: factory_core::Message,
    timestamp: u64,
) -> Result<Vec<lee_core::program::TransactionEvent>, lee::error::LeeError> {
    let transaction =
        factory_transaction(state, factory, actors, key, salt, launch, pool, instruction);
    state.transition_from_public_transaction(&transaction, 1, timestamp)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn factory_transaction(
    state: &V03State,
    factory: AccountId,
    actors: &[Actor],
    key: &PrivateKey,
    salt: [u8; 32],
    launch: AccountId,
    pool: AccountId,
    instruction: factory_core::Message,
) -> PublicTransaction {
    const FACTORY_PROGRAM: AccountId = AccountId::new([17; 32]);
    let creator = AccountId::from(&PublicKey::new_from_private_key(key));
    let mut message = Message::try_new(
        Actor::new(factory, FACTORY_PROGRAM),
        actors.to_vec(),
        BTreeMap::from([(creator, state.get_account_by_id(creator).nonce)]),
        instruction,
    )
    .unwrap();
    message.admission_evidence = vec![
        lee::PublicAccountEvidence::Pda {
            program: FACTORY_PROGRAM,
            seed: factory_core::compute_factory_seed(NAMESPACE, salt),
        },
        lee::PublicAccountEvidence::Pda {
            program: if actors.contains(&Actor::new(pool, CURVE)) {
                CURVE
            } else {
                LBP
            },
            seed: if actors.contains(&Actor::new(pool, CURVE)) {
                curve_core::compute_pool_pda_seed(NAMESPACE, launch, COLLATERAL, factory)
            } else {
                lbp_core::compute_pool_pda_seed(NAMESPACE, launch, COLLATERAL, factory)
            },
        },
    ];
    let mint = factory_core::compute_mint_pda(NAMESPACE, FACTORY_PROGRAM, salt);
    let escrow = factory_core::compute_escrow_pda(NAMESPACE, FACTORY_PROGRAM, salt);
    for owner in [
        factory,
        mint,
        escrow,
        pool,
        creator,
        TREASURY,
        NEXT_TREASURY,
    ] {
        for definition in [launch, COLLATERAL] {
            message
                .admission_evidence
                .push(lee::PublicAccountEvidence::Pda {
                    program: TOKEN_PROGRAM_ACCOUNT_ID,
                    seed: token::holding_seed(owner, definition),
                });
        }
    }
    let witnesses = WitnessSet::for_message(&message, &[key]);
    PublicTransaction::new(message, witnesses)
}

/// Runs the real clock guest in the node-free state fixture. Sequencer guards and
/// end-of-block ordering are outside this fixture.
pub(super) fn tick_clock(state: &mut V03State, timestamp: u64) {
    let clock = clock_core::clock_actor();
    let old = clock_core::ClockAccountData::from_bytes(
        state
            .get_account_by_id(clock.account_id)
            .data
            .actor_state(clock.program_account_id),
    );
    let message = Message::try_new(
        clock,
        vec![clock],
        BTreeMap::new(),
        clock_core::Message::Tick {
            timestamp,
            block_id: old.block_id + 1,
        },
    )
    .unwrap();
    let tx = PublicTransaction::new(message.clone(), WitnessSet::for_message(&message, &[]));
    state
        .transition_from_public_transaction(&tx, old.block_id + 1, timestamp)
        .unwrap();
    let updated = clock_core::ClockAccountData::from_bytes(
        state
            .get_account_by_id(clock.account_id)
            .data
            .actor_state(clock.program_account_id),
    );
    assert_eq!(
        (updated.block_id, updated.timestamp),
        (old.block_id + 1, timestamp)
    );
}

pub(super) fn token_holding(
    owner: AccountId,
    definition: AccountId,
    balance: u128,
) -> (AccountId, Account) {
    account(
        holding(owner, definition),
        borsh::to_vec(&TokenState::Holding {
            owner,
            definition,
            kind: TokenKind::Fungible,
            balance,
        })
        .unwrap(),
    )
}

pub(super) fn token_supply(state: &V03State, definition: AccountId) -> u128 {
    match TokenState::decode(
        state
            .get_account_by_id(definition)
            .data
            .actor_state(TOKEN_PROGRAM_ACCOUNT_ID),
    )
    .unwrap()
    {
        TokenState::Definition { supply, .. } => supply,
        _ => panic!("expected definition"),
    }
}

pub(super) fn seeded_role(role: AccountId, holder: AccountId) -> (AccountId, Account) {
    use lee_core::{account::ActorState, program::ReceiveInput};
    use lez_compat_core::authority::{self, Message as AuthorityMessage};
    let actor = Actor::new(role, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID);
    let bootstrap = authority::handle(&ReceiveInput {
        receiver: actor,
        from: None,
        is_authorized: true,
        pre_state: ActorState::empty(),
        message: borsh::to_vec(&AuthorityMessage::Bootstrap { authority: holder }).unwrap(),
    })
    .unwrap();
    let installed = authority::handle(&ReceiveInput {
        receiver: actor,
        from: Some(Actor::new(
            holder,
            factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
        )),
        is_authorized: false,
        pre_state: bootstrap.post_state.unwrap(),
        message: borsh::to_vec(&AuthorityMessage::Install { epoch: 0 }).unwrap(),
    })
    .unwrap();
    account(actor, installed.post_state.unwrap().to_vec())
}

pub(super) fn allowlist_for(
    pool: AccountId,
    trader: AccountId,
) -> (curve_core::BuyPolicy, curve_core::MembershipPath) {
    use risc0_zkvm::sha::{Impl, Sha256};
    let path = curve_core::MembershipPath {
        index: 0,
        salt: [43; 32],
        siblings: [[44; 32]; 17],
    };
    let context = [&[1u8][..], NAMESPACE.value(), pool.value()].concat();
    let hash = |parts: &[&[u8]]| -> [u8; 32] {
        Impl::hash_bytes(&parts.concat())
            .as_bytes()
            .try_into()
            .unwrap()
    };
    let mut root = hash(&[
        b"LP/allowlist/leaf/v1\0",
        &context,
        trader.value(),
        &path.salt,
    ]);
    for sibling in path.siblings {
        root = hash(&[b"LP/allowlist/node/v1\0", &context, &root, &sibling]);
    }
    (
        curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
            version: 1,
            namespace: NAMESPACE.to_bytes(),
            sale_policy_id: pool.to_bytes(),
            root,
        }),
        path,
    )
}
