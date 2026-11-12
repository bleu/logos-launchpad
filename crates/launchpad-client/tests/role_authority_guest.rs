use lee::{
    AccountId, Actor, PrivateKey, PublicKey, PublicTransaction, V03State,
    program::Program,
    public_transaction::{Message, WitnessSet},
};
use lee_core::account::Account;
use lez_compat_core::{
    authority,
    token::{self, TokenKind, TokenState},
};
use std::{borrow::Cow, collections::BTreeMap};

#[path = "role_authority_guest/controls.rs"]
mod controls;
#[path = "role_authority_guest/settlement.rs"]
mod settlement;

const AUTHORITY: AccountId = curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID;
const TOKEN: AccountId = curve_core::TOKEN_PROGRAM_ACCOUNT_ID;
const CURVE: AccountId = AccountId::new([7; 32]);
const FACTORY: AccountId = AccountId::new([8; 32]);
const SALT: [u8; 32] = [51; 32];
const LAUNCH: AccountId = AccountId::new([10; 32]);
const COLLATERAL: AccountId = AccountId::new([11; 32]);
const TREASURY: AccountId = AccountId::new([15; 32]);

fn holding(owner: AccountId, definition: AccountId) -> Actor {
    Actor::new(token::holding_address(TOKEN, owner, definition), TOKEN)
}

fn account(actor: Actor, bytes: Vec<u8>) -> (AccountId, Account) {
    (
        actor.account_id,
        Account::default().with_actor_state(actor.program_account_id, bytes.into()),
    )
}

fn balance(state: &V03State, owner: AccountId, definition: AccountId) -> u128 {
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

fn base_state(namespace: &PrivateKey, holders: &[AccountId]) -> (V03State, Vec<Actor>) {
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
            clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
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
        .with_named_programs([
            (CURVE, guest("LAUNCHPAD_CURVE_ELF")),
            (TOKEN, guest("LAUNCHPAD_TOKEN_ELF")),
            (AUTHORITY, guest("LAUNCHPAD_AUTHORITY_ELF")),
            (clock_core::clock_account_id(), guest("LAUNCHPAD_CLOCK_ELF")),
        ])
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

fn curve_call(
    state: &mut V03State,
    actors: &[Actor],
    pool: AccountId,
    signer: &PrivateKey,
    instruction: curve_core::Message,
) -> Result<(), lee::error::LeeError> {
    curve_call_at(state, actors, pool, signer, instruction, 1_000)
}

fn curve_call_at(
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

fn rejected_without_changes(
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

fn key(byte: u8) -> PrivateKey {
    PrivateKey::try_new([byte; 32]).unwrap()
}

fn id(key: &PrivateKey) -> AccountId {
    AccountId::from(&PublicKey::new_from_private_key(key))
}

fn guest(variable: &str) -> Program {
    let path = std::env::var(variable)
        .unwrap_or_else(|_| panic!("set {variable} to a freshly built guest ELF"));
    Program::new(Cow::Owned(
        std::fs::read(&path).unwrap_or_else(|error| panic!("cannot read {path}: {error}")),
    ))
    .unwrap()
}

fn execute(
    state: &mut V03State,
    receiver: Actor,
    actors: Vec<Actor>,
    instruction: impl borsh::BorshSerialize,
    keys: &[&PrivateKey],
) -> Result<(), lee::error::LeeError> {
    execute_at(state, receiver, actors, instruction, keys, 1_000)
}

fn execute_at(
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

fn execute_with_evidence(
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

struct FactoryFixture {
    state: V03State,
    inclusion_timestamp_ms: u64,
    actors: Vec<Actor>,
    namespace: AccountId,
    factory: AccountId,
    pool: AccountId,
    definition: AccountId,
}

impl FactoryFixture {
    fn call(
        &mut self,
        signer: &PrivateKey,
        command: factory_core::Message,
    ) -> Result<(), lee::error::LeeError> {
        self.call_with_events(signer, command).map(|_| ())
    }

    fn call_with_events(
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

fn factory_fixture() -> FactoryFixture {
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
        state: state.with_named_programs([(FACTORY, guest("LAUNCHPAD_FACTORY_ELF"))]),
        inclusion_timestamp_ms: 1_000,
        actors,
        namespace: namespace_id,
        factory,
        pool,
        definition,
    }
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn program_controlled_admin_accepts_and_updates_namespace_fee() {
    let namespace = key(44);
    let controller = key(50);
    let owner_program = AccountId::new([18; 32]);
    let seed = lee_core::program::PdaSeed::new(*id(&controller).value());
    let successor = AccountId::for_public_pda(&owner_program, &seed);
    let namespace_id = id(&namespace);
    let (state, mut actors) = base_state(&namespace, &[id(&controller)]);
    actors.extend([
        Actor::new(id(&controller), owner_program),
        Actor::new(successor, owner_program),
        Actor::new(successor, CURVE),
        Actor::new(successor, AUTHORITY),
    ]);
    let mut state = state.with_named_programs([(owner_program, guest("LAUNCHPAD_OWNER_ELF"))]);
    execute_with_evidence(
        &mut state,
        Actor::new(namespace_id, CURVE),
        actors.clone(),
        curve_core::Message::TransferAdmin {
            namespace: namespace_id,
            new_admin: successor,
            candidate: curve_core::AdminCandidate::Pda {
                program_id: owner_program,
                seed: *seed.as_bytes(),
            },
        },
        &[&namespace],
        vec![lee::PublicAccountEvidence::Pda {
            program: owner_program,
            seed,
        }],
        1_000,
    )
    .expect("program-controlled successor accepts its role");
    rejected_without_changes(
        &mut state,
        |state| {
            execute_with_evidence(
                state,
                Actor::new(id(&controller), owner_program),
                actors.clone(),
                (
                    CURVE,
                    curve_core::Message::UpdateFee {
                        namespace: namespace_id,
                        protocol_fee_bps: 250,
                    },
                ),
                &[],
                vec![lee::PublicAccountEvidence::Key(
                    PublicKey::new_from_private_key(&controller),
                )],
                1_000,
            )
        },
        "owner controller must sign",
    );
    execute(
        &mut state,
        Actor::new(id(&controller), owner_program),
        actors.clone(),
        (
            CURVE,
            curve_core::Message::UpdateFee {
                namespace: namespace_id,
                protocol_fee_bps: 250,
            },
        ),
        &[&controller],
    )
    .expect("successor exercises namespace administration");
    let config = curve_core::Config::decode(
        state
            .get_account_by_id(curve_core::compute_config_pda(namespace_id, CURVE))
            .data
            .actor_state(CURVE),
    )
    .unwrap();
    assert_eq!(config.protocol_fee_bps, 250);
    rejected_without_changes(
        &mut state,
        |state| {
            execute(
                state,
                Actor::new(namespace_id, CURVE),
                actors,
                curve_core::Message::UpdateFee {
                    namespace: namespace_id,
                    protocol_fee_bps: 500,
                },
                &[&namespace],
            )
        },
        "Unauthorized",
    );
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn transferred_curve_creator_funds_and_settles_without_pool_custody() {
    let mut fixture = factory_fixture();
    let creator = key(48);
    let successor = key(49);
    fixture
        .call(
            &creator,
            factory_core::Message::CreateFactoryPool {
                buy_policy: curve_core::BuyPolicy::Open,
                pause_policy: curve_core::PausePolicy::Disabled,
                ending_policy: curve_core::EndingPolicy::Either,
                trade_mode: 0,
                graduation_hook_program_id: None,
                graduation_hook_configuration: vec![],
                namespace: fixture.namespace,
                launch_salt: SALT,
                name: "Sale".into(),
                uri: "ipfs://sale".into(),
                sale_reserve: 400,
                dex_seed_reserve: 100,
                creator_allocation: 50,
                virtual_token_reserve: 1_000,
                virtual_collateral_reserve: 1_000,
                end_timestamp: Some(2),
                curve_program_id: CURVE,
                collateral_definition_id: COLLATERAL,
                creator: id(&creator),
                now: 1,
            },
        )
        .expect("mint stage");
    fixture
        .call(
            &creator,
            factory_core::Message::ContinueCreation {
                creator: id(&creator),
                now: 1,
            },
        )
        .expect("allocation stage");
    fixture
        .call(
            &creator,
            factory_core::Message::ContinueCreation {
                creator: id(&creator),
                now: 1,
            },
        )
        .expect("pool preparation stage");
    let role = factory_core::compute_curve_creator_role_pda(fixture.namespace, FACTORY, SALT);
    execute(
        &mut fixture.state,
        Actor::new(id(&creator), AUTHORITY),
        fixture.actors.clone(),
        authority::Message::Transfer {
            role,
            successor: id(&successor),
        },
        &[&creator, &successor],
    )
    .unwrap();
    let before = fixture.state.clone();
    let rejected = fixture.call(
        &creator,
        factory_core::Message::ContinueCreation {
            creator: id(&creator),
            now: 1,
        },
    );
    assert!(
        matches!(&rejected, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("Unauthorized")),
        "{rejected:?}"
    );
    assert!(
        fixture.state == before,
        "former creator changed pending sale"
    );
    fixture
        .call(
            &successor,
            factory_core::Message::ContinueCreation {
                creator: id(&successor),
                now: 1,
            },
        )
        .expect("funding stage");
    let saved = curve_core::PoolAccount::decode(
        fixture
            .state
            .get_account_by_id(fixture.pool)
            .data
            .actor_state(CURVE),
    )
    .unwrap();
    assert!(saved.funded);
    assert_eq!(saved.owner, fixture.factory);
    assert_eq!(
        saved.owner_program,
        Some((
            FACTORY,
            *factory_core::compute_factory_seed(fixture.namespace, SALT).as_bytes()
        ))
    );
    assert_eq!(
        balance(&fixture.state, fixture.pool, fixture.definition),
        400
    );
    assert_eq!(
        balance(&fixture.state, fixture.factory, fixture.definition),
        100
    );
    rejected_without_changes(
        &mut fixture.state,
        |state| {
            curve_call(
                state,
                &fixture.actors,
                fixture.pool,
                &successor,
                curve_core::Message::ClosePool { now: 1 },
            )
        },
        "Unauthorized",
    );
    for owner in [id(&creator), id(&successor)] {
        execute(
            &mut fixture.state,
            Actor::new(fixture.definition, TOKEN),
            fixture.actors.clone(),
            token::Message::Initialize { owner },
            &[],
        )
        .unwrap();
    }
    curve_call(
        &mut fixture.state,
        &fixture.actors,
        fixture.pool,
        &successor,
        curve_core::Message::SwapExactOutput {
            membership: None,
            amount_out: 100,
            max_amount_in: 112,
            token_in: COLLATERAL,
            now: 1,
        },
    )
    .unwrap();
    assert_eq!(
        balance(&fixture.state, id(&successor), fixture.definition),
        100
    );
    assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 112);
    fixture.inclusion_timestamp_ms = 2_000;
    let before = fixture.state.clone();
    let rejected = fixture.call(
        &creator,
        factory_core::Message::ClaimCreatorAllocation {
            creator: id(&creator),
            now: 2,
        },
    );
    assert!(
        matches!(&rejected, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("Unauthorized")),
        "{rejected:?}"
    );
    assert!(fixture.state == before, "former creator claimed allocation");
    let valid_state = fixture.state.clone();
    fixture.state = fixture.state.clone().with_public_accounts([account(
        holding(id(&successor), fixture.definition),
        borsh::to_vec(&TokenState::Holding {
            definition: COLLATERAL,
            owner: id(&successor),
            kind: TokenKind::Fungible,
            balance: 100,
        })
        .unwrap(),
    )]);
    let before = fixture.state.clone();
    let rejected = fixture.call(
        &successor,
        factory_core::Message::ClaimCreatorAllocation {
            creator: id(&successor),
            now: 2,
        },
    );
    assert!(
        matches!(&rejected, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("WrongDefinition")),
        "{rejected:?}"
    );
    assert!(
        fixture.state == before,
        "failed payout changed allocation, escrow, role state or nonce"
    );
    fixture.state = valid_state;
    fixture
        .call(
            &successor,
            factory_core::Message::ClaimCreatorAllocation {
                creator: id(&successor),
                now: 2,
            },
        )
        .unwrap();
    assert_eq!(
        balance(&fixture.state, id(&successor), fixture.definition),
        150
    );
    for expected_stage in [
        factory_core::SettlementStage::Ready,
        factory_core::SettlementStage::Withdrawn {
            unsold: 300,
            collateral_due: 112,
        },
        factory_core::SettlementStage::Burned {
            collateral_due: 112,
        },
        factory_core::SettlementStage::TokenPaid {
            collateral_due: 112,
        },
        factory_core::SettlementStage::Complete,
    ] {
        let before = fixture.state.clone();
        let rejected = fixture.call(
            &creator,
            factory_core::Message::WithdrawFactoryProceeds {
                creator: id(&creator),
                now: 2,
            },
        );
        assert!(
            matches!(&rejected, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("Unauthorized")),
            "{rejected:?}"
        );
        assert!(fixture.state == before, "former creator changed settlement");
        fixture
            .call(
                &successor,
                factory_core::Message::WithdrawFactoryProceeds {
                    creator: id(&successor),
                    now: 2,
                },
            )
            .unwrap();
        let saved = factory_core::FactoryState::decode(
            fixture
                .state
                .get_account_by_id(fixture.factory)
                .data
                .actor_state(FACTORY),
        )
        .unwrap();
        assert_eq!(saved.settlement_stage, expected_stage);
    }
    assert_eq!(
        balance(&fixture.state, id(&successor), fixture.definition),
        250
    );
    assert_eq!(balance(&fixture.state, id(&successor), COLLATERAL), 1_000);
    assert_eq!(balance(&fixture.state, id(&creator), fixture.definition), 0);
    assert_eq!(
        balance(&fixture.state, fixture.factory, fixture.definition),
        0
    );
    assert_eq!(balance(&fixture.state, fixture.factory, COLLATERAL), 0);
    assert_eq!(balance(&fixture.state, fixture.pool, fixture.definition), 0);
    let definition = TokenState::decode(
        fixture
            .state
            .get_account_by_id(fixture.definition)
            .data
            .actor_state(TOKEN),
    )
    .unwrap();
    assert!(matches!(
        definition,
        TokenState::Definition { supply: 250, .. }
    ));
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn transferred_direct_pool_owner_funds_closes_and_receives_reserves() {
    let namespace = key(44);
    let first = key(45);
    let next = key(46);
    let namespace_id = id(&namespace);
    let (mut state, mut actors) = base_state(&namespace, &[id(&first), id(&next)]);
    let pool = curve_core::compute_pool_pda(namespace_id, CURVE, LAUNCH, COLLATERAL, id(&first));
    let role = curve_core::compute_pool_owner_role(namespace_id, pool, CURVE);
    actors.extend([
        Actor::new(pool, CURVE),
        Actor::new(pool, TOKEN),
        Actor::new(role, AUTHORITY),
        holding(pool, LAUNCH),
        holding(pool, COLLATERAL),
    ]);
    curve_call(
        &mut state,
        &actors,
        pool,
        &first,
        curve_core::Message::CreatePool {
            buy_policy: curve_core::BuyPolicy::Open,
            pause_policy: curve_core::PausePolicy::Disabled,
            ending_policy: curve_core::EndingPolicy::Manual,
            namespace: namespace_id,
            token0_definition_id: LAUNCH,
            token1_definition_id: COLLATERAL,
            allowed_input_token: None,
            defer_funding: true,
            token0_amount: 400,
            token1_amount: 100,
            virtual_reserve0: 1_000,
            virtual_reserve1: 1_000,
            close_timestamp: None,
            close_on_depletion: None,
            owner: id(&first),
            owner_program_id: None,
            owner_program_seed: None,
            curve_program_id: CURVE,
            now: 1,
        },
    )
    .unwrap();
    execute(
        &mut state,
        Actor::new(id(&first), AUTHORITY),
        actors.clone(),
        authority::Message::Transfer {
            role,
            successor: id(&next),
        },
        &[&first, &next],
    )
    .unwrap();
    rejected_without_changes(
        &mut state,
        |state| {
            curve_call(
                state,
                &actors,
                pool,
                &first,
                curve_core::Message::ActivatePool { now: 1 },
            )
        },
        "Unauthorized",
    );
    curve_call(
        &mut state,
        &actors,
        pool,
        &next,
        curve_core::Message::ActivatePool { now: 1 },
    )
    .unwrap();
    assert_eq!(balance(&state, id(&first), LAUNCH), 1_000);
    assert_eq!(balance(&state, id(&next), LAUNCH), 600);
    assert_eq!(balance(&state, id(&next), COLLATERAL), 900);
    assert_eq!(balance(&state, pool, LAUNCH), 400);
    assert_eq!(balance(&state, pool, COLLATERAL), 100);
    let saved =
        curve_core::PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(CURVE))
            .unwrap();
    assert_eq!(saved.owner, id(&first));
    assert!(saved.owner_program.is_none());
    rejected_without_changes(
        &mut state,
        |state| {
            curve_call(
                state,
                &actors,
                pool,
                &first,
                curve_core::Message::ClosePool { now: 1 },
            )
        },
        "Unauthorized",
    );
    curve_call(
        &mut state,
        &actors,
        pool,
        &next,
        curve_core::Message::ClosePool { now: 1 },
    )
    .unwrap();
    rejected_without_changes(
        &mut state,
        |state| {
            curve_call(
                state,
                &actors,
                pool,
                &first,
                curve_core::Message::WithdrawReserves { now: 1 },
            )
        },
        "Unauthorized",
    );
    curve_call(
        &mut state,
        &actors,
        pool,
        &next,
        curve_core::Message::WithdrawReserves { now: 1 },
    )
    .unwrap();
    assert_eq!(balance(&state, id(&next), LAUNCH), 1_000);
    assert_eq!(balance(&state, id(&next), COLLATERAL), 1_000);
    assert_eq!(balance(&state, pool, LAUNCH), 0);
    assert_eq!(balance(&state, pool, COLLATERAL), 0);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn another_namespace_pool_role_and_namespace_admin_cannot_fund_a_pool() {
    let namespace = key(53);
    let other_namespace = key(54);
    let owner = key(55);
    let other_owner = key(56);
    let (mut state, mut actors) = base_state(&namespace, &[id(&owner), id(&other_owner)]);
    actors.extend([
        Actor::new(id(&other_namespace), CURVE),
        Actor::new(id(&other_namespace), AUTHORITY),
        Actor::new(
            curve_core::compute_config_pda(id(&other_namespace), CURVE),
            CURVE,
        ),
        Actor::new(
            curve_core::compute_admin_role(id(&other_namespace), CURVE),
            AUTHORITY,
        ),
    ]);
    execute(
        &mut state,
        Actor::new(id(&other_namespace), CURVE),
        actors.clone(),
        curve_core::Message::InitializeNamespace {
            namespace: id(&other_namespace),
            protocol_fee_bps: 0,
            treasury: TREASURY,
        },
        &[&other_namespace],
    )
    .unwrap();
    let mut pools = vec![];
    for (namespace, owner) in [
        (id(&namespace), &owner),
        (id(&other_namespace), &other_owner),
    ] {
        let pool = curve_core::compute_pool_pda(namespace, CURVE, LAUNCH, COLLATERAL, id(owner));
        actors.extend([
            Actor::new(pool, CURVE),
            Actor::new(pool, TOKEN),
            holding(pool, LAUNCH),
            holding(pool, COLLATERAL),
            Actor::new(
                curve_core::compute_pool_owner_role(namespace, pool, CURVE),
                AUTHORITY,
            ),
        ]);
        curve_call(
            &mut state,
            &actors,
            pool,
            owner,
            curve_core::Message::CreatePool {
                buy_policy: curve_core::BuyPolicy::Open,
                pause_policy: curve_core::PausePolicy::Disabled,
                ending_policy: curve_core::EndingPolicy::Manual,
                namespace,
                token0_definition_id: LAUNCH,
                token1_definition_id: COLLATERAL,
                allowed_input_token: None,
                defer_funding: true,
                token0_amount: 400,
                token1_amount: 100,
                virtual_reserve0: 1_000,
                virtual_reserve1: 1_000,
                close_timestamp: None,
                close_on_depletion: None,
                owner: id(owner),
                owner_program_id: None,
                owner_program_seed: None,
                curve_program_id: CURVE,
                now: 1,
            },
        )
        .unwrap();
        pools.push(pool);
    }
    for wrong_holder in [&other_owner, &namespace, &other_namespace] {
        rejected_without_changes(
            &mut state,
            |state| {
                curve_call(
                    state,
                    &actors,
                    pools[0],
                    wrong_holder,
                    curve_core::Message::ActivatePool { now: 1 },
                )
            },
            "Unauthorized",
        );
    }
    curve_call(
        &mut state,
        &actors,
        pools[0],
        &owner,
        curve_core::Message::ActivatePool { now: 1 },
    )
    .unwrap();
    curve_call(
        &mut state,
        &actors,
        pools[1],
        &other_owner,
        curve_core::Message::ActivatePool { now: 1 },
    )
    .unwrap();
    assert_eq!(balance(&state, pools[0], LAUNCH), 400);
    assert_eq!(balance(&state, pools[1], LAUNCH), 400);
}

#[test]
#[ignore = "requires a fresh LAUNCHPAD_AUTHORITY_ELF and local guest executor"]
fn transfer_only_role_requires_successor_consent_and_cannot_renounce() {
    let role_key = key(41);
    let first = key(42);
    let next = key(43);
    let role = id(&role_key);
    let actors = vec![
        Actor::new(role, AUTHORITY),
        Actor::new(id(&first), AUTHORITY),
        Actor::new(id(&next), AUTHORITY),
    ];
    let mut state = V03State::new()
        .with_named_programs([(AUTHORITY, guest("LAUNCHPAD_AUTHORITY_ELF"))])
        .with_public_accounts([account(actors[2], vec![])]);
    execute(
        &mut state,
        actors[0],
        actors.clone(),
        authority::Message::BootstrapTransferOnly {
            authority: id(&first),
        },
        &[&role_key, &first],
    )
    .unwrap();

    let before = state.clone();
    let rejected = execute(
        &mut state,
        actors[1],
        actors.clone(),
        authority::Message::Transfer {
            role,
            successor: id(&next),
        },
        &[&first],
    );
    assert!(
        matches!(&rejected, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("Unauthorized")),
        "{rejected:?}"
    );
    assert!(
        state == before,
        "missing successor consent changed runtime state"
    );

    execute(
        &mut state,
        actors[1],
        actors.clone(),
        authority::Message::Transfer {
            role,
            successor: id(&next),
        },
        &[&first, &next],
    )
    .unwrap();
    let before = state.clone();
    let rejected = execute(
        &mut state,
        actors[1],
        actors.clone(),
        authority::Message::Transfer {
            role,
            successor: id(&first),
        },
        &[&first],
    );
    assert!(
        matches!(&rejected, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("Unauthorized")),
        "{rejected:?}"
    );
    assert!(state == before, "former holder changed runtime state");

    let rejected = execute(
        &mut state,
        actors[2],
        actors,
        authority::Message::Renounce { role },
        &[&next],
    );
    assert!(
        matches!(&rejected, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("Unauthorized")),
        "{rejected:?}"
    );
    assert!(state == before, "renounce changed transfer-only role state");
}
