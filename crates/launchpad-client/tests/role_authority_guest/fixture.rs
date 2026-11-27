use lee::{
    AccountId, Actor, PrivateKey, PublicKey, PublicTransaction, V03State,
    program::Program,
    public_transaction::{Message, WitnessSet},
};
use lee_core::account::Account;
use lez_compat_core::token::{self, TokenKind, TokenState};
use std::{borrow::Cow, collections::BTreeMap};

pub(super) const AUTHORITY: AccountId = curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID;
pub(super) const TOKEN: AccountId = curve_core::TOKEN_PROGRAM_ACCOUNT_ID;
pub(super) const CURVE: AccountId = AccountId::new([7; 32]);
pub(super) const FACTORY: AccountId = AccountId::new([8; 32]);
pub(super) const SALT: [u8; 32] = [51; 32];
pub(super) const LAUNCH: AccountId = AccountId::new([10; 32]);
pub(super) const COLLATERAL: AccountId = AccountId::new([11; 32]);
pub(super) const TREASURY: AccountId = AccountId::new([15; 32]);

pub(super) fn holding(owner: AccountId, definition: AccountId) -> Actor {
    Actor::new(token::holding_address(TOKEN, owner, definition), TOKEN)
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
            .actor_state(TOKEN),
    )
    .unwrap()
    {
        TokenState::Holding { balance, .. } => balance,
        _ => panic!("expected token holding"),
    }
}

pub(super) fn base_state(namespace: &PrivateKey, holders: &[AccountId]) -> (V03State, Vec<Actor>) {
    let namespace_id = id(namespace);
    let mut actors = vec![
        Actor::new(namespace_id, CURVE),
        Actor::new(namespace_id, AUTHORITY),
        Actor::new(curve_core::compute_config_pda(namespace_id, CURVE), CURVE),
        Actor::new(
            curve_core::compute_admin_role(namespace_id, CURVE),
            AUTHORITY,
        ),
        Actor::new(
            clock_core::CLOCK_PROGRAM_ACCOUNT_ID,
            clock_core::clock_account_id(),
        ),
    ];
    let mut accounts = vec![account(
        actors[4],
        clock_core::ClockAccountData {
            block_id: 1,
            timestamp: 1_000,
        }
        .to_bytes(),
    )];
    for definition in [LAUNCH, COLLATERAL] {
        actors.push(Actor::new(definition, TOKEN));
        accounts.push(account(
            Actor::new(definition, TOKEN),
            borsh::to_vec(&TokenState::Definition {
                name: "fixture".into(),
                kind: TokenKind::Fungible,
                supply: 100_000,
                metadata: None,
            })
            .unwrap(),
        ));
        for owner in holders {
            actors.extend([
                Actor::new(*owner, CURVE),
                Actor::new(*owner, TOKEN),
                Actor::new(*owner, AUTHORITY),
                holding(*owner, definition),
            ]);
            accounts.push(account(
                holding(*owner, definition),
                borsh::to_vec(&TokenState::Holding {
                    definition,
                    owner: *owner,
                    kind: TokenKind::Fungible,
                    balance: 1_000,
                })
                .unwrap(),
            ));
        }
    }
    actors.sort();
    actors.dedup();
    let mut state = V03State::new()
        .with_programs_at(
            [
                (CURVE, guest("LAUNCHPAD_CURVE_ELF")),
                (TOKEN, guest("LAUNCHPAD_TOKEN_ELF")),
                (AUTHORITY, guest("LAUNCHPAD_AUTHORITY_ELF")),
                (clock_core::clock_account_id(), guest("LAUNCHPAD_CLOCK_ELF")),
            ]
            .map(|(address, program)| (address, program, true)),
        )
        .with_public_accounts(accounts);
    execute(
        &mut state,
        Actor::new(namespace_id, CURVE),
        actors.clone(),
        curve_core::Message::InitializeNamespace {
            namespace: namespace_id,
            protocol_fee_bps: 0,
            treasury: TREASURY,
        },
        &[namespace],
    )
    .unwrap();
    (state, actors)
}

pub(super) fn curve_call(
    state: &mut V03State,
    actors: &[Actor],
    pool: AccountId,
    signer: &PrivateKey,
    instruction: curve_core::Message,
) -> Result<(), lee::error::LeeError> {
    curve_call_at(state, actors, pool, signer, instruction, 1_000)
}

pub(super) fn curve_call_at(
    state: &mut V03State,
    actors: &[Actor],
    pool: AccountId,
    signer: &PrivateKey,
    instruction: curve_core::Message,
    inclusion_timestamp_ms: u64,
) -> Result<(), lee::error::LeeError> {
    execute_at(
        state,
        Actor::new(id(signer), CURVE),
        actors.to_vec(),
        curve_core::Message::Execute {
            pool,
            instruction: Box::new(instruction),
        },
        &[signer],
        inclusion_timestamp_ms,
    )
}

pub(super) fn rejected_without_changes(
    state: &mut V03State,
    operation: impl FnOnce(&mut V03State) -> Result<(), lee::error::LeeError>,
    reason: &str,
) {
    let before = state.clone();
    let result = operation(state);
    assert!(
        matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains(reason)),
        "expected {reason}, got {result:?}"
    );
    assert!(
        *state == before,
        "rejected operation changed runtime state or nonce"
    );
}

pub(super) fn key(byte: u8) -> PrivateKey {
    PrivateKey::try_new([byte; 32]).unwrap()
}

pub(super) fn id(key: &PrivateKey) -> AccountId {
    AccountId::from(&PublicKey::new_from_private_key(key))
}

pub(super) fn guest(variable: &str) -> Program {
    let path = std::env::var(variable)
        .unwrap_or_else(|_| panic!("set {variable} to a freshly built guest ELF"));
    Program::new(Cow::Owned(
        std::fs::read(&path).unwrap_or_else(|error| panic!("cannot read {path}: {error}")),
    ))
    .unwrap()
}

pub(super) fn execute(
    state: &mut V03State,
    receiver: Actor,
    actors: Vec<Actor>,
    instruction: impl borsh::BorshSerialize,
    keys: &[&PrivateKey],
) -> Result<(), lee::error::LeeError> {
    execute_at(state, receiver, actors, instruction, keys, 1_000)
}

pub(super) fn execute_at(
    state: &mut V03State,
    receiver: Actor,
    actors: Vec<Actor>,
    instruction: impl borsh::BorshSerialize,
    keys: &[&PrivateKey],
    inclusion_timestamp_ms: u64,
) -> Result<(), lee::error::LeeError> {
    execute_with_evidence(
        state,
        receiver,
        actors,
        instruction,
        keys,
        vec![],
        inclusion_timestamp_ms,
    )
}

pub(super) fn execute_with_evidence(
    state: &mut V03State,
    receiver: Actor,
    actors: Vec<Actor>,
    instruction: impl borsh::BorshSerialize,
    keys: &[&PrivateKey],
    evidence: Vec<lee::PublicAccountEvidence>,
    inclusion_timestamp_ms: u64,
) -> Result<(), lee::error::LeeError> {
    execute_with_evidence_events(
        state,
        receiver,
        actors,
        instruction,
        keys,
        evidence,
        inclusion_timestamp_ms,
    )
    .map(|_| ())
}

fn execute_with_evidence_events(
    state: &mut V03State,
    receiver: Actor,
    actors: Vec<Actor>,
    instruction: impl borsh::BorshSerialize,
    keys: &[&PrivateKey],
    evidence: Vec<lee::PublicAccountEvidence>,
    inclusion_timestamp_ms: u64,
) -> Result<Vec<lee_core::program::TransactionEvent>, lee::error::LeeError> {
    let nonces = keys
        .iter()
        .map(|key| (id(key), state.get_account_by_id(id(key)).nonce))
        .collect::<BTreeMap<_, _>>();
    let mut message = Message::try_new(receiver, actors, nonces, instruction).unwrap();
    message.admission_evidence.extend(evidence);
    let witnesses = WitnessSet::for_message(&message, keys);
    state.transition_from_public_transaction(
        &PublicTransaction::new(message, witnesses),
        1,
        inclusion_timestamp_ms,
    )
}

pub(super) struct FactoryFixture {
    pub(super) state: V03State,
    pub(super) inclusion_timestamp_ms: u64,
    pub(super) actors: Vec<Actor>,
    pub(super) namespace: AccountId,
    pub(super) factory: AccountId,
    pub(super) pool: AccountId,
    pub(super) definition: AccountId,
}

impl FactoryFixture {
    pub(super) fn call(
        &mut self,
        signer: &PrivateKey,
        command: factory_core::Message,
    ) -> Result<(), lee::error::LeeError> {
        self.call_with_events(signer, command).map(|_| ())
    }

    pub(super) fn call_with_events(
        &mut self,
        signer: &PrivateKey,
        command: factory_core::Message,
    ) -> Result<Vec<lee_core::program::TransactionEvent>, lee::error::LeeError> {
        execute_with_evidence_events(
            &mut self.state,
            Actor::new(self.factory, FACTORY),
            self.actors.clone(),
            command,
            &[signer],
            vec![lee::PublicAccountEvidence::Pda {
                program: FACTORY,
                seed: factory_core::compute_factory_seed(self.namespace, SALT),
            }],
            self.inclusion_timestamp_ms,
        )
    }
}

pub(super) fn factory_fixture() -> FactoryFixture {
    let namespace = key(47);
    let creator = key(48);
    let successor = key(49);
    let namespace_id = id(&namespace);
    let (state, mut actors) = base_state(&namespace, &[id(&creator), id(&successor)]);
    let factory = factory_core::compute_factory_pda(namespace_id, FACTORY, SALT);
    let definition = factory_core::compute_definition_pda(namespace_id, FACTORY, SALT);
    let pool = curve_core::compute_pool_pda(namespace_id, CURVE, definition, COLLATERAL, factory);
    let mint = factory_core::compute_mint_pda(namespace_id, FACTORY, SALT);
    let escrow = factory_core::compute_escrow_pda(namespace_id, FACTORY, SALT);
    actors.extend([
        Actor::new(factory, FACTORY),
        Actor::new(factory, CURVE),
        Actor::new(factory, TOKEN),
        Actor::new(definition, TOKEN),
        Actor::new(mint, TOKEN),
        Actor::new(escrow, TOKEN),
        Actor::new(pool, CURVE),
        Actor::new(pool, TOKEN),
        Actor::new(
            factory_core::compute_curve_creator_role_pda(namespace_id, FACTORY, SALT),
            AUTHORITY,
        ),
        holding(mint, definition),
        holding(escrow, definition),
        holding(factory, definition),
        holding(factory, COLLATERAL),
        holding(pool, definition),
        holding(pool, COLLATERAL),
        holding(id(&creator), definition),
        holding(id(&successor), definition),
    ]);
    FactoryFixture {
        state: state.with_programs_at(
            [(FACTORY, guest("LAUNCHPAD_FACTORY_ELF"))]
                .map(|(address, program)| (address, program, true)),
        ),
        inclusion_timestamp_ms: 1_000,
        actors,
        namespace: namespace_id,
        factory,
        pool,
        definition,
    }
}
