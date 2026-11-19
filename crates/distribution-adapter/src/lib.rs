//! Immutable percentage distribution for public graduation assets.
use borsh::{BorshDeserialize, BorshSerialize};
use factory_core::{
    AdapterState, FactoryState, GraduationIntentKind, GraduationRequest, GraduationResult,
    LbpLaunchState, LbpSetupMode, TOKEN_PROGRAM_ACCOUNT_ID, compute_definition_pda,
    compute_factory_pda, compute_graduation_pda, compute_graduation_seed,
};
use lee_core::{
    account::{AccountId, Actor},
    program::{ProgramEvent, ReceiveInput, Response, StateReply, SyncMessage},
};
use lez_compat_core::token::{self, TokenKind, TokenState};
use serde::{Deserialize, Serialize};

pub const VERSION: u8 = 1;
pub const DISTRIBUTION_EVENT_SELECTOR: [u8; 8] = [78, 169, 222, 125, 157, 30, 46, 109];
pub const MAX_RULES_PER_ASSET: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub destination: AccountId,
    pub bps: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub version: u8,
    pub launch_rules: Vec<Rule>,
    pub collateral_rules: Vec<Rule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidConfiguration,
    InvalidMessage,
    InvalidState,
    Unauthorized,
    WrongAccount,
    WrongIntent,
    InvalidCustody,
    AlreadyInitialized,
    AlreadyGraduated,
    NotGraduated,
    Arithmetic,
}

impl Configuration {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        // Bound allocation before Borsh decodes caller-controlled vector lengths.
        if bytes.len() > 1 + 8 + 2 * MAX_RULES_PER_ASSET * 34 {
            return Err(Error::InvalidConfiguration);
        }
        let value: Self = borsh::from_slice(bytes).map_err(|_| Error::InvalidConfiguration)?;
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.version != VERSION {
            return Err(Error::InvalidConfiguration);
        }
        for rules in [&self.launch_rules, &self.collateral_rules] {
            if rules.is_empty() || rules.len() > MAX_RULES_PER_ASSET {
                return Err(Error::InvalidConfiguration);
            }
            let total: u32 = rules.iter().map(|r| u32::from(r.bps)).sum();
            if total != 10_000 || rules.iter().any(|r| r.bps > 10_000) {
                return Err(Error::InvalidConfiguration);
            }
        }
        Ok(())
    }

    pub fn validate_custody(&self, custody: AccountId) -> Result<(), Error> {
        self.validate()?;
        if self
            .launch_rules
            .iter()
            .chain(&self.collateral_rules)
            .any(|r| r.destination == custody)
        {
            return Err(Error::InvalidConfiguration);
        }
        Ok(())
    }

    pub fn hash(&self) -> [u8; 32] {
        use risc0_zkvm::sha::{Impl, Sha256 as _};
        Impl::hash_bytes(&borsh::to_vec(self).unwrap())
            .as_bytes()
            .try_into()
            .unwrap()
    }

    /// Plans from the original full balance; only the last rule uses the remainder.
    pub fn plan(&self, balances: [u128; 2]) -> Result<Vec<Payout>, Error> {
        self.validate()?;
        let mut result = Vec::new();
        for (asset, rules) in [&self.launch_rules, &self.collateral_rules]
            .into_iter()
            .enumerate()
        {
            let initial = balances[asset];
            let mut remaining = initial;
            for (index, rule) in rules.iter().enumerate() {
                let amount = if index + 1 == rules.len() {
                    remaining
                } else {
                    integer_math::mul_div(
                        initial,
                        u128::from(rule.bps),
                        10_000,
                        integer_math::Rounding::Floor,
                    )
                    .map_err(|_| Error::Arithmetic)?
                };
                remaining = remaining.checked_sub(amount).ok_or(Error::Arithmetic)?;
                if amount != 0 {
                    result.push(Payout {
                        asset: asset as u8,
                        destination: rule.destination,
                        amount,
                    });
                }
            }
        }
        Ok(result)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize)]
pub struct Payout {
    pub asset: u8,
    pub destination: AccountId,
    pub amount: u128,
}

// The first three tags match the factory's AdapterMessage contract.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Message {
    Initialize {
        factory_program_id: AccountId,
        namespace: AccountId,
        launch_salt: [u8; 32],
        accepted_intent: GraduationIntentKind,
    },
    ReadState,
    Graduate {
        request: GraduationRequest,
    },
    DistributeRemaining,
    ReadDistributionState,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct State {
    pub factory: Actor,
    pub namespace: AccountId,
    pub launch_salt: [u8; 32],
    pub accepted_intent: GraduationIntentKind,
    pub definitions: [AccountId; 2],
    pub configuration: Configuration,
    pub configuration_hash: [u8; 32],
    pub receipt: Option<GraduationRequest>,
    pub graduation_amounts: [u128; 2],
    pub distribution_count: u64,
    pending: Option<Pending>,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
enum Stored {
    Initializing {
        factory: Actor,
        namespace: AccountId,
        launch_salt: [u8; 32],
        kind: GraduationIntentKind,
    },
    Ready(Box<State>),
}

impl State {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        match borsh::from_slice(bytes).map_err(|_| Error::InvalidState)? {
            Stored::Ready(state) => Ok(*state),
            _ => Err(Error::InvalidState),
        }
    }
    fn response(&self) -> Response {
        Response::set_state(borsh::to_vec(&Stored::Ready(Box::new(self.clone()))).unwrap())
    }
    pub fn is_idle(&self) -> bool {
        self.pending.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct Credit {
    payout: Payout,
    before: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
enum Phase {
    Definition(u8),
    Custody(u8),
    RecipientBefore(u8),
    CustodyAfter(u8),
    RecipientAfter(u8),
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct Pending {
    request: Option<GraduationRequest>,
    phase: Phase,
    balances: [u128; 2],
    payouts: Vec<Payout>,
    credits: Vec<Credit>,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DistributionEvent {
    pub version: u8,
    pub factory: Actor,
    pub namespace: AccountId,
    pub launch_salt: [u8; 32],
    pub definitions: [AccountId; 2],
    pub sequence: u64,
    pub graduation: bool,
    pub amounts: [u128; 2],
}

impl DistributionEvent {
    pub const SELECTOR_NAME: &'static str = "distribution_adapter::DistributionV1";
}

fn token_actor(id: AccountId) -> Actor {
    Actor::new(id, TOKEN_PROGRAM_ACCOUNT_ID)
}
fn holding(owner: AccountId, definition: AccountId) -> Actor {
    token_actor(token::holding_address(
        TOKEN_PROGRAM_ACCOUNT_ID,
        owner,
        definition,
    ))
}
fn read(response: Response, actor: Actor) -> Response {
    response.send_sync(actor, &token::Message::ReadState)
}
fn definition(value: TokenState) -> Result<(), Error> {
    match value {
        TokenState::Definition {
            kind: TokenKind::Fungible,
            ..
        } => Ok(()),
        _ => Err(Error::InvalidCustody),
    }
}
fn balance(value: TokenState, owner: AccountId, definition: AccountId) -> Result<u128, Error> {
    match value {
        TokenState::Holding {
            owner: stored,
            definition: stored_definition,
            kind: TokenKind::Fungible,
            balance,
        } if stored == owner && stored_definition == definition => Ok(balance),
        _ => Err(Error::InvalidCustody),
    }
}
fn token_reply(input: &ReceiveInput, expected: Actor) -> Result<TokenState, Error> {
    if input.from != Some(expected) {
        return Err(Error::Unauthorized);
    }
    token::read_reply(input, expected).map_err(|_| Error::InvalidCustody)
}

pub fn handle(input: &ReceiveInput) -> Result<Response, Error> {
    if input.pre_state.is_empty() {
        let Message::Initialize {
            factory_program_id,
            namespace,
            launch_salt,
            accepted_intent,
        } = borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?
        else {
            return Err(Error::InvalidState);
        };
        let factory = Actor::new(
            compute_factory_pda(namespace, factory_program_id, launch_salt),
            factory_program_id,
        );
        if input.receiver.account_id
            != compute_graduation_pda(input.receiver.program_account_id, factory.account_id)
        {
            return Err(Error::WrongAccount);
        }
        return Ok(Response::set_state(
            borsh::to_vec(&Stored::Initializing {
                factory,
                namespace,
                launch_salt,
                kind: accepted_intent,
            })
            .unwrap(),
        )
        .send_sync(factory, &factory_core::Message::ReadState));
    }
    let stored: Stored = borsh::from_slice(&input.pre_state).map_err(|_| Error::InvalidState)?;
    let mut state = match stored {
        Stored::Initializing {
            factory,
            namespace,
            launch_salt,
            kind,
        } => {
            return finish_initialization(input, factory, namespace, launch_salt, kind);
        }
        Stored::Ready(state) => *state,
    };
    if input.receiver.account_id
        != compute_graduation_pda(input.receiver.program_account_id, state.factory.account_id)
    {
        return Err(Error::WrongAccount);
    }
    if let Some(pending) = state.pending.take() {
        return continue_distribution(input, state, pending);
    }
    let message: Message = borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    match message {
        Message::Initialize { .. } => Err(Error::AlreadyInitialized),
        Message::ReadState => {
            let view = AdapterState::public_view(
                state.factory,
                state.accepted_intent,
                state.receipt.clone(),
            );
            Ok(Response::keep_state().send_sync(
                input.from.ok_or(Error::Unauthorized)?,
                &StateReply {
                    state: borsh::to_vec(&view).unwrap().into(),
                },
            ))
        }
        Message::ReadDistributionState => Ok(Response::keep_state().send_sync(
            input.from.ok_or(Error::Unauthorized)?,
            &StateReply::from(input),
        )),
        Message::Graduate { request } => {
            if input.from != Some(state.factory) {
                return Err(Error::Unauthorized);
            }
            if state.receipt.is_some() {
                return Err(Error::AlreadyGraduated);
            }
            request
                .validate_amounts()
                .map_err(|_| Error::InvalidMessage)?;
            if request.factory_program_id != state.factory.program_account_id
                || request.namespace != state.namespace
                || request.launch_salt != state.launch_salt
                || request.token_definition_id != state.definitions[0]
                || request.collateral_definition_id != state.definitions[1]
            {
                return Err(Error::WrongAccount);
            }
            if request.intent.kind() != state.accepted_intent {
                return Err(Error::WrongIntent);
            }
            let config = Configuration::decode(&request.configuration)?;
            if config != state.configuration || config.hash() != state.configuration_hash {
                return Err(Error::InvalidConfiguration);
            }
            begin(state, Some(request))
        }
        Message::DistributeRemaining => {
            if state.receipt.is_none() {
                return Err(Error::NotGraduated);
            }
            begin(state, None)
        }
    }
}

fn finish_initialization(
    input: &ReceiveInput,
    factory: Actor,
    namespace: AccountId,
    launch_salt: [u8; 32],
    kind: GraduationIntentKind,
) -> Result<Response, Error> {
    if input.from != Some(factory) {
        return Err(Error::Unauthorized);
    }
    if input.receiver.account_id
        != compute_graduation_pda(input.receiver.program_account_id, factory.account_id)
    {
        return Err(Error::WrongAccount);
    }
    let reply: StateReply = borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    let (saved_namespace, salt, launch, collateral, hook, actual_kind) =
        if let Ok(value) = LbpLaunchState::decode(&reply.state) {
            (
                value.namespace,
                value.launch_salt,
                value.token_definition_id,
                value.collateral_definition_id,
                value.graduation_hook,
                match value.mode {
                    LbpSetupMode::FixedPrice => GraduationIntentKind::FixedPrice,
                    LbpSetupMode::Weighted { .. } => GraduationIntentKind::Weighted,
                },
            )
        } else {
            let value = FactoryState::decode(&reply.state).map_err(|_| Error::InvalidState)?;
            value.validate(factory).map_err(|_| Error::WrongAccount)?;
            (
                value.namespace,
                value.launch_salt,
                value.token_definition_id,
                value.collateral_definition_id,
                value.graduation_hook,
                GraduationIntentKind::Curve,
            )
        };
    let hook = hook.ok_or(Error::WrongAccount)?;
    if saved_namespace != namespace
        || salt != launch_salt
        || factory.account_id
            != compute_factory_pda(namespace, factory.program_account_id, launch_salt)
        || launch != compute_definition_pda(namespace, factory.program_account_id, launch_salt)
        || launch == collateral
        || hook.program_id != input.receiver.program_account_id
    {
        return Err(Error::WrongAccount);
    }
    if actual_kind != kind {
        return Err(Error::WrongIntent);
    }
    let configuration = Configuration::decode(&hook.configuration)?;
    configuration.validate_custody(input.receiver.account_id)?;
    Ok(State {
        factory,
        namespace,
        launch_salt,
        accepted_intent: kind,
        definitions: [launch, collateral],
        configuration_hash: configuration.hash(),
        configuration,
        receipt: None,
        graduation_amounts: [0; 2],
        distribution_count: 0,
        pending: None,
    }
    .response())
}

fn begin(mut state: State, request: Option<GraduationRequest>) -> Result<Response, Error> {
    state.pending = Some(Pending {
        request,
        phase: Phase::Definition(0),
        balances: [0; 2],
        payouts: vec![],
        credits: vec![],
    });
    Ok(read(state.response(), token_actor(state.definitions[0])))
}

fn continue_distribution(
    input: &ReceiveInput,
    mut state: State,
    mut pending: Pending,
) -> Result<Response, Error> {
    let custody = input.receiver.account_id;
    let next_actor = match pending.phase {
        Phase::Definition(side) => {
            let side = usize::from(side);
            let asset = *state.definitions.get(side).ok_or(Error::InvalidState)?;
            definition(token_reply(input, token_actor(asset))?)?;
            if side == 0 {
                pending.phase = Phase::Definition(1);
                token_actor(state.definitions[1])
            } else {
                pending.phase = Phase::Custody(0);
                holding(custody, state.definitions[0])
            }
        }
        Phase::Custody(side) => {
            let side = usize::from(side);
            let asset = *state.definitions.get(side).ok_or(Error::InvalidState)?;
            pending.balances[side] =
                balance(token_reply(input, holding(custody, asset))?, custody, asset)?;
            if let Some(request) = &pending.request {
                let (launch, collateral) = request.intent.amounts();
                if pending.balances[side] < [launch, collateral][side] {
                    return Err(Error::InvalidCustody);
                }
            }
            if side == 0 {
                pending.phase = Phase::Custody(1);
                holding(custody, state.definitions[1])
            } else {
                pending.payouts = state.configuration.plan(pending.balances)?;
                for payout in &pending.payouts {
                    if let Some(credit) = pending.credits.iter_mut().find(|c| {
                        c.payout.asset == payout.asset && c.payout.destination == payout.destination
                    }) {
                        credit.payout.amount = credit
                            .payout
                            .amount
                            .checked_add(payout.amount)
                            .ok_or(Error::Arithmetic)?;
                    } else {
                        pending.credits.push(Credit {
                            payout: payout.clone(),
                            before: 0,
                        });
                    }
                }
                if pending.credits.is_empty() {
                    return finish(state, pending);
                }
                pending.phase = Phase::RecipientBefore(0);
                let actor = holding(
                    pending.credits[0].payout.destination,
                    state.definitions[usize::from(pending.credits[0].payout.asset)],
                );
                let credits = pending.credits.clone();
                state.pending = Some(pending);
                let mut response = state.response();
                for credit in &credits {
                    response = response.send_sync(
                        token_actor(state.definitions[usize::from(credit.payout.asset)]),
                        &token::Message::Initialize {
                            owner: credit.payout.destination,
                        },
                    );
                }
                return Ok(read(response, actor));
            }
        }
        Phase::RecipientBefore(index) => {
            let credit = pending
                .credits
                .get_mut(usize::from(index))
                .ok_or(Error::InvalidState)?;
            let asset = state.definitions[usize::from(credit.payout.asset)];
            credit.before = balance(
                token_reply(input, holding(credit.payout.destination, asset))?,
                credit.payout.destination,
                asset,
            )?;
            credit
                .before
                .checked_add(credit.payout.amount)
                .ok_or(Error::Arithmetic)?;
            if let Some(next) = pending.credits.get(usize::from(index) + 1) {
                pending.phase = Phase::RecipientBefore(index + 1);
                holding(
                    next.payout.destination,
                    state.definitions[usize::from(next.payout.asset)],
                )
            } else {
                let payouts = pending.payouts.clone();
                pending.phase = Phase::CustodyAfter(0);
                state.pending = Some(pending);
                let mut response = state.response();
                for payout in payouts {
                    response = response.send(
                        SyncMessage::new(
                            token_actor(custody),
                            &token::Message::Transfer {
                                definition: state.definitions[usize::from(payout.asset)],
                                recipient: payout.destination,
                                amount: payout.amount,
                            },
                        )
                        .with_pda_seeds([compute_graduation_seed(state.factory.account_id)]),
                    );
                }
                return Ok(read(response, holding(custody, state.definitions[0])));
            }
        }
        Phase::CustodyAfter(side) => {
            let asset = *state
                .definitions
                .get(usize::from(side))
                .ok_or(Error::InvalidState)?;
            if balance(token_reply(input, holding(custody, asset))?, custody, asset)? != 0 {
                return Err(Error::InvalidCustody);
            }
            if side == 0 {
                pending.phase = Phase::CustodyAfter(1);
                holding(custody, state.definitions[1])
            } else {
                pending.phase = Phase::RecipientAfter(0);
                let first = pending.credits.first().ok_or(Error::InvalidState)?;
                holding(
                    first.payout.destination,
                    state.definitions[usize::from(first.payout.asset)],
                )
            }
        }
        Phase::RecipientAfter(index) => {
            let credit = pending
                .credits
                .get(usize::from(index))
                .ok_or(Error::InvalidState)?;
            let asset = state.definitions[usize::from(credit.payout.asset)];
            let after = balance(
                token_reply(input, holding(credit.payout.destination, asset))?,
                credit.payout.destination,
                asset,
            )?;
            if after
                != credit
                    .before
                    .checked_add(credit.payout.amount)
                    .ok_or(Error::Arithmetic)?
            {
                return Err(Error::InvalidCustody);
            }
            if let Some(next) = pending.credits.get(usize::from(index) + 1) {
                pending.phase = Phase::RecipientAfter(index + 1);
                holding(
                    next.payout.destination,
                    state.definitions[usize::from(next.payout.asset)],
                )
            } else {
                return finish(state, pending);
            }
        }
    };
    state.pending = Some(pending);
    Ok(read(state.response(), next_actor))
}

fn finish(mut state: State, pending: Pending) -> Result<Response, Error> {
    let graduation = pending.request.is_some();
    if let Some(request) = &pending.request {
        state.receipt = Some(request.clone());
        state.graduation_amounts = pending.balances;
    }
    state.distribution_count = state
        .distribution_count
        .checked_add(1)
        .ok_or(Error::Arithmetic)?;
    let event = DistributionEvent {
        version: VERSION,
        factory: state.factory,
        namespace: state.namespace,
        launch_salt: state.launch_salt,
        definitions: state.definitions,
        sequence: state.distribution_count,
        graduation,
        amounts: pending.balances,
    };
    let mut response = state.response().event(ProgramEvent {
        selector: DISTRIBUTION_EVENT_SELECTOR,
        data: borsh::to_vec(&event).unwrap(),
    });
    if let Some(request) = pending.request {
        response = response.send_sync(
            state.factory,
            &GraduationResult {
                factory: state.factory.account_id,
                request_hash: request.hash(),
            },
        );
    }
    Ok(response)
}
