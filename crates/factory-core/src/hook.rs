use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum GraduationIntentKind {
    CurveReserves,
    FixedPriceProceeds,
    WeightedBuyerProceeds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum GraduationIntent {
    CurveReserves {
        launch_tokens: u128,
        collateral: u128,
    },
    FixedPriceProceeds {
        collateral: u128,
    },
    WeightedBuyerProceeds {
        collateral: u128,
    },
}

impl GraduationIntent {
    pub fn kind(self) -> GraduationIntentKind {
        match self {
            Self::CurveReserves { .. } => GraduationIntentKind::CurveReserves,
            Self::FixedPriceProceeds { .. } => GraduationIntentKind::FixedPriceProceeds,
            Self::WeightedBuyerProceeds { .. } => GraduationIntentKind::WeightedBuyerProceeds,
        }
    }

    pub fn amounts(self) -> (u128, u128) {
        match self {
            Self::CurveReserves {
                launch_tokens,
                collateral,
            } => (launch_tokens, collateral),
            Self::FixedPriceProceeds { collateral }
            | Self::WeightedBuyerProceeds { collateral } => (0, collateral),
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
    pub configuration: Vec<u8>,
}
impl GraduationRequest {
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
        accepted_intent: Option<GraduationIntentKind>,
    },
    ReadState,
    Graduate {
        request: GraduationRequest,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AdapterState {
    pub factory: Actor,
    pub accepted_intent: Option<GraduationIntentKind>,
    pub receipt: Option<GraduationRequest>,
    pending: Option<(GraduationRequest, u8)>,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct GraduationResult {
    pub factory: AccountId,
    pub request_hash: [u8; 32],
}
impl AdapterState {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        borsh::from_slice(bytes).map_err(|_| Error::InvalidState)
    }
}
pub fn handle_adapter(input: &ReceiveInput) -> Result<Response, Error> {
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
                borsh::to_vec(&AdapterState {
                    factory,
                    accepted_intent,
                    receipt: None,
                    pending: None,
                })
                .unwrap(),
            ))
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
    if state.accepted_intent != Some(request.intent.kind()) {
        return Err(Error::AdapterNotReady);
    }
    Ok(())
}
