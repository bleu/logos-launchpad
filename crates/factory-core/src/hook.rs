use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum GraduationIntentKind {
    Curve,
    FixedPrice,
    Weighted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum GraduationIntent {
    Curve {
        launch_tokens: u128,
        collateral: u128,
    },
    FixedPrice {
        launch_tokens: u128,
        collateral: u128,
    },
    Weighted {
        launch_tokens: u128,
        collateral: u128,
    },
}

impl GraduationIntent {
    pub fn kind(self) -> GraduationIntentKind {
        match self {
            Self::Curve { .. } => GraduationIntentKind::Curve,
            Self::FixedPrice { .. } => GraduationIntentKind::FixedPrice,
            Self::Weighted { .. } => GraduationIntentKind::Weighted,
        }
    }

    pub fn amounts(self) -> (u128, u128) {
        match self {
            Self::Curve {
                launch_tokens,
                collateral,
            }
            | Self::FixedPrice {
                launch_tokens,
                collateral,
            }
            | Self::Weighted {
                launch_tokens,
                collateral,
            } => (launch_tokens, collateral),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct GraduationRequest {
    pub factory_program_id: AccountId,
    pub namespace: AccountId,
    pub launch_salt: [u8; 32],
    pub token_definition_id: AccountId,
    pub collateral_definition_id: AccountId,
    pub intent: GraduationIntent,
    pub reserve_tokens: u128,
    pub unsold_tokens: u128,
    pub configuration: Vec<u8>,
}
impl GraduationRequest {
    pub fn validate_amounts(&self) -> Result<(), Error> {
        if self.reserve_tokens.checked_add(self.unsold_tokens) != Some(self.intent.amounts().0) {
            return Err(Error::InvalidTerms);
        }
        Ok(())
    }
    pub fn hash(&self) -> [u8; 32] {
        use risc0_zkvm::sha::{Impl, Sha256 as _};
        Impl::hash_bytes(&borsh::to_vec(self).expect("graduation request serializes"))
            .as_bytes()
            .try_into()
            .unwrap()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AdapterMessage {
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
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AdapterState {
    pub factory: Actor,
    pub accepted_intent: GraduationIntentKind,
    pub receipt: Option<GraduationRequest>,
    pending: Option<(GraduationRequest, u8)>,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct GraduationResult {
    pub factory: AccountId,
    pub request_hash: [u8; 32],
}
impl AdapterState {
    /// Stable idle state view for adapters with their own private storage.
    pub fn public_view(
        factory: Actor,
        accepted_intent: GraduationIntentKind,
        receipt: Option<GraduationRequest>,
    ) -> Self {
        Self {
            factory,
            accepted_intent,
            receipt,
            pending: None,
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        borsh::from_slice(bytes).map_err(|_| Error::InvalidState)
    }
}
// The tag occupies the accepted-intent byte in AdapterState, which only permits 0..=2.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct AdapterInitialization {
    factory: Actor,
    tag: u8,
    namespace: AccountId,
    launch_salt: [u8; 32],
    accepted_intent: GraduationIntentKind,
}

impl AdapterInitialization {
    fn finish(self, input: &ReceiveInput) -> Result<Response, Error> {
        if input.from != Some(self.factory) {
            return Err(Error::Unauthorized);
        }
        if input.receiver.account_id
            != compute_graduation_pda(input.receiver.program_account_id, self.factory.account_id)
            || self.factory.account_id
                != compute_factory_pda(
                    self.namespace,
                    self.factory.program_account_id,
                    self.launch_salt,
                )
        {
            return Err(Error::WrongAccount);
        }
        let reply: StateReply =
            borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
        let (namespace, launch_salt, definition, hook, kind) =
            if let Ok(state) = LbpLaunchState::decode(&reply.state) {
                (
                    state.namespace,
                    state.launch_salt,
                    state.token_definition_id,
                    state.graduation_hook,
                    match state.mode {
                        LbpSetupMode::FixedPrice => GraduationIntentKind::FixedPrice,
                        LbpSetupMode::Weighted { .. } => GraduationIntentKind::Weighted,
                    },
                )
            } else {
                let state = FactoryState::decode(&reply.state)?;
                state.validate(self.factory)?;
                (
                    state.namespace,
                    state.launch_salt,
                    state.token_definition_id,
                    state.graduation_hook,
                    GraduationIntentKind::Curve,
                )
            };
        if namespace != self.namespace
            || launch_salt != self.launch_salt
            || definition
                != compute_definition_pda(namespace, self.factory.program_account_id, launch_salt)
            || hook.as_ref().map(|hook| hook.program_id) != Some(input.receiver.program_account_id)
        {
            return Err(Error::WrongAccount);
        }
        if self.accepted_intent != kind {
            return Err(Error::AdapterIntentMismatch);
        }
        Ok(Response::set_state(
            borsh::to_vec(&AdapterState {
                factory: self.factory,
                accepted_intent: self.accepted_intent,
                receipt: None,
                pending: None,
            })
            .unwrap(),
        ))
    }
}

pub fn handle_adapter(input: &ReceiveInput) -> Result<Response, Error> {
    if let Ok(initialization) = borsh::from_slice::<AdapterInitialization>(&input.pre_state)
        && initialization.tag == u8::MAX
    {
        return initialization.finish(input);
    }
    if let Ok(mut state) = AdapterState::decode(&input.pre_state)
        && let Some((request, step)) = state.pending.take()
    {
        validate_request(&state, &request, input.receiver)?;
        if step > 3 {
            return Err(Error::InvalidState);
        }
        let owner = input.receiver.account_id;
        let definition = if step == 0 || step == 2 {
            request.token_definition_id
        } else {
            request.collateral_definition_id
        };
        let expected = Actor::new(
            if step < 2 {
                definition
            } else {
                token::holding_address(TOKEN_PROGRAM_ACCOUNT_ID, owner, definition)
            },
            TOKEN_PROGRAM_ACCOUNT_ID,
        );
        let value = token::read_reply(input, expected)?;
        match value {
            TokenState::Definition {
                kind: TokenKind::Fungible,
                ..
            } if step < 2 => {}
            TokenState::Holding {
                owner: stored,
                definition: stored_definition,
                kind: TokenKind::Fungible,
                balance,
            } if step >= 2
                && stored == owner
                && stored_definition == definition
                && balance
                    >= if step == 2 {
                        request.intent.amounts().0
                    } else {
                        request.intent.amounts().1
                    } => {}
            _ => return Err(Error::InvalidCustody),
        }
        if step == 3 {
            let request_hash = request.hash();
            state.receipt = Some(request);
            return Ok(
                Response::set_state(borsh::to_vec(&state).unwrap()).send_sync(
                    state.factory,
                    &GraduationResult {
                        factory: state.factory.account_id,
                        request_hash,
                    },
                ),
            );
        }
        let next = step + 1;
        let definition = if next == 2 {
            request.token_definition_id
        } else {
            request.collateral_definition_id
        };
        let address = if next < 2 {
            definition
        } else {
            token::holding_address(TOKEN_PROGRAM_ACCOUNT_ID, owner, definition)
        };
        state.pending = Some((request, next));
        return Ok(
            Response::set_state(borsh::to_vec(&state).unwrap()).send_sync(
                Actor::new(address, TOKEN_PROGRAM_ACCOUNT_ID),
                &token::Message::ReadState,
            ),
        );
    }
    let message: AdapterMessage =
        borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    match message {
        AdapterMessage::Initialize {
            factory_program_id,
            namespace,
            launch_salt,
            accepted_intent,
        } => {
            let factory = Actor::new(
                compute_factory_pda(namespace, factory_program_id, launch_salt),
                factory_program_id,
            );
            if input.receiver.account_id
                != compute_graduation_pda(input.receiver.program_account_id, factory.account_id)
            {
                return Err(Error::WrongAccount);
            }
            if !input.pre_state.is_empty() {
                return Err(Error::AlreadyInitialized);
            }
            Ok(Response::set_state(
                borsh::to_vec(&AdapterInitialization {
                    factory,
                    tag: u8::MAX,
                    namespace,
                    launch_salt,
                    accepted_intent,
                })
                .unwrap(),
            )
            .send_sync(factory, &Message::ReadState))
        }
        AdapterMessage::Graduate { request } => {
            let mut state = AdapterState::decode(&input.pre_state)?;
            if input.from != Some(state.factory) {
                return Err(Error::Unauthorized);
            }
            validate_request(&state, &request, input.receiver)?;
            if state.receipt.is_some() {
                return Err(Error::AlreadySettled);
            }
            state.pending = Some((request.clone(), 0));
            Ok(
                Response::set_state(borsh::to_vec(&state).unwrap()).send_sync(
                    Actor::new(request.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                    &token::Message::ReadState,
                ),
            )
        }
        AdapterMessage::ReadState => {
            AdapterState::decode(&input.pre_state)?;
            Ok(Response::keep_state().send_sync(
                input.from.ok_or(Error::Unauthorized)?,
                &StateReply::from(input),
            ))
        }
    }
}

fn validate_request(
    state: &AdapterState,
    request: &GraduationRequest,
    receiver: Actor,
) -> Result<(), Error> {
    request.validate_amounts()?;
    if receiver.account_id
        != compute_graduation_pda(receiver.program_account_id, state.factory.account_id)
        || state.factory
            != Actor::new(
                compute_factory_pda(
                    request.namespace,
                    request.factory_program_id,
                    request.launch_salt,
                ),
                request.factory_program_id,
            )
        || request.token_definition_id
            != compute_definition_pda(
                request.namespace,
                request.factory_program_id,
                request.launch_salt,
            )
        || request.token_definition_id == request.collateral_definition_id
    {
        return Err(Error::WrongAccount);
    }
    if state.accepted_intent != request.intent.kind() {
        return Err(Error::AdapterIntentMismatch);
    }
    Ok(())
}
