use super::*;
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct GraduationRequest {
    pub factory_program_id: AccountId,
    pub namespace: AccountId,
    pub launch_salt: [u8; 32],
    pub token_definition_id: AccountId,
    pub collateral_definition_id: AccountId,
    pub dex_seed_reserve: u128,
    pub collateral_amount: u128,
    pub configuration: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AdapterMessage {
    Initialize {
        factory_program_id: AccountId,
        namespace: AccountId,
        launch_salt: [u8; 32],
        ready: bool,
    },
    ReadState,
    Graduate {
        request: GraduationRequest,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AdapterState {
    pub factory: Actor,
    pub ready: bool,
    pub receipt: Option<GraduationRequest>,
    pending: Option<(GraduationRequest, u8)>,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct GraduationResult {
    pub factory: AccountId,
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
                        request.dex_seed_reserve
                    } else {
                        request.collateral_amount
                    } => {}
            _ => return Err(Error::InvalidCustody),
        }
        if step == 3 {
            state.receipt = Some(request);
            return Ok(
                Response::set_state(borsh::to_vec(&state).unwrap()).send_sync(
                    state.factory,
                    &GraduationResult {
                        factory: state.factory.account_id,
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
            ready,
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
                let previous = AdapterState::decode(&input.pre_state)?;
                if previous.factory != factory
                    || previous.receipt.is_some()
                    || previous.pending.is_some()
                {
                    return Err(Error::AlreadyInitialized);
                }
            }
            Ok(Response::set_state(
                borsh::to_vec(&AdapterState {
                    factory,
                    ready,
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
            if input.receiver.account_id
                != compute_graduation_pda(
                    input.receiver.program_account_id,
                    state.factory.account_id,
                )
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
            if !state.ready {
                return Err(Error::AdapterNotReady);
            }
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
