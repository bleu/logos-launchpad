use crate::Error;
use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, Actor, ActorState},
    program::{PdaSeed, ReceiveInput, Response, StateReply},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TokenKind {
    Fungible,
    NftMaster,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MetadataStandard {
    Simple,
    Expanded,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Metadata {
    pub standard: MetadataStandard,
    pub uri: String,
    pub creators: String,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TokenState {
    Definition {
        name: String,
        kind: TokenKind,
        supply: u128,
        metadata: Option<Metadata>,
    },
    Holding {
        definition: AccountId,
        owner: AccountId,
        kind: TokenKind,
        balance: u128,
    },
}

impl TokenState {
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        borsh::from_slice(bytes).map_err(|_| Error::InvalidState)
    }
    fn encode(&self) -> ActorState {
        borsh::to_vec(self).unwrap().into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Message {
    Create {
        name: String,
        kind: TokenKind,
        supply: u128,
        recipient: AccountId,
        metadata: Option<Metadata>,
    },
    Credit {
        definition: AccountId,
        owner: AccountId,
        kind: TokenKind,
        amount: u128,
    },
    Transfer {
        definition: AccountId,
        recipient: AccountId,
        amount: u128,
    },
    Debit {
        definition: AccountId,
        owner: AccountId,
        recipient: AccountId,
        amount: u128,
    },
    Mint {
        recipient: AccountId,
        amount: u128,
    },
    Burn {
        definition: AccountId,
        amount: u128,
    },
    BurnHolding {
        definition: AccountId,
        owner: AccountId,
        amount: u128,
    },
    BurnSupply {
        owner: AccountId,
        amount: u128,
    },
    Initialize {
        owner: AccountId,
    },
    AssertNft {
        definition: AccountId,
        callback: Actor,
    },
    CheckNft {
        definition: AccountId,
        owner: AccountId,
        callback: Actor,
    },
    ReadState,
}

pub fn read_reply(input: &ReceiveInput, expected: Actor) -> Result<TokenState, Error> {
    if input.from != Some(expected) {
        return Err(Error::Unauthorized);
    }
    let reply: StateReply = borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    TokenState::decode(&reply.state)
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NftHolder {
    pub definition: AccountId,
    pub owner: AccountId,
}

pub fn verify_nft_holder(
    input: &ReceiveInput,
    program: AccountId,
    definition: AccountId,
    owner: AccountId,
) -> Result<(), Error> {
    if input.from
        != Some(Actor::new(
            holding_address(program, owner, definition),
            program,
        ))
    {
        return Err(Error::Unauthorized);
    }
    let reply: NftHolder = borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    if reply != (NftHolder { definition, owner }) {
        return Err(Error::WrongDefinition);
    }
    Ok(())
}

pub fn holding_seed(owner: AccountId, definition: AccountId) -> PdaSeed {
    use lee_core::program::PdaSeed;
    // The domain keeps custody addresses separate from other program PDAs.
    use lee_core::account::AccountId;
    let domain = AccountId::from_builtin_program_name(b"launchpad/token-holding/v1");
    let first = AccountId::for_public_pda(&domain, &PdaSeed::new(*owner.value()));
    let seed = AccountId::for_public_pda(&first, &PdaSeed::new(*definition.value()));
    PdaSeed::new(*seed.value())
}

pub fn holding_address(program: AccountId, owner: AccountId, definition: AccountId) -> AccountId {
    AccountId::for_public_pda(&program, &holding_seed(owner, definition))
}

pub fn handle(input: &ReceiveInput) -> Result<Response, Error> {
    let message: Message = borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    let program = input.receiver.program_account_id;
    match message {
        Message::ReadState => {
            let reader = input.from.ok_or(Error::Unauthorized)?;
            TokenState::decode(&input.pre_state)?;
            Ok(Response::keep_state().send_sync(reader, &StateReply::from(input)))
        }
        Message::AssertNft {
            definition,
            callback,
        } => {
            if !input.is_authorized {
                return Err(Error::Unauthorized);
            }
            let owner = input.receiver.account_id;
            Ok(Response::keep_state().send_sync(
                Actor::new(holding_address(program, owner, definition), program),
                &Message::CheckNft {
                    definition,
                    owner,
                    callback,
                },
            ))
        }
        Message::CheckNft {
            definition,
            owner,
            callback,
        } => {
            if input.from != Some(Actor::new(owner, program)) {
                return Err(Error::Unauthorized);
            }
            if input.receiver.account_id != holding_address(program, owner, definition) {
                return Err(Error::WrongAccount);
            }
            if TokenState::decode(&input.pre_state)?
                != (TokenState::Holding {
                    definition,
                    owner,
                    kind: TokenKind::NftMaster,
                    balance: 1,
                })
            {
                return Err(Error::InvalidNft);
            }
            Ok(Response::keep_state().send_sync(callback, &NftHolder { definition, owner }))
        }
        Message::Initialize { owner } => {
            let TokenState::Definition { kind, .. } = TokenState::decode(&input.pre_state)? else {
                return Err(Error::WrongDefinition);
            };
            let definition = input.receiver.account_id;
            Ok(Response::keep_state().send_sync(
                Actor::new(holding_address(program, owner, definition), program),
                &Message::Credit {
                    definition,
                    owner,
                    kind,
                    amount: 0,
                },
            ))
        }
        Message::Burn { definition, amount } => {
            if !input.is_authorized {
                return Err(Error::Unauthorized);
            }
            let owner = input.receiver.account_id;
            Ok(Response::keep_state().send_sync(
                Actor::new(holding_address(program, owner, definition), program),
                &Message::BurnHolding {
                    definition,
                    owner,
                    amount,
                },
            ))
        }
        Message::BurnHolding {
            definition,
            owner,
            amount,
        } => {
            if input.from != Some(Actor::new(owner, program)) {
                return Err(Error::Unauthorized);
            }
            if input.receiver.account_id != holding_address(program, owner, definition) {
                return Err(Error::WrongAccount);
            }
            let TokenState::Holding {
                definition: stored,
                owner: holder,
                kind,
                balance,
            } = TokenState::decode(&input.pre_state)?
            else {
                return Err(Error::WrongDefinition);
            };
            if stored != definition || holder != owner {
                return Err(Error::WrongDefinition);
            }
            let balance = balance
                .checked_sub(amount)
                .ok_or(Error::InsufficientBalance)?;
            Ok(Response::set_state(
                TokenState::Holding {
                    definition,
                    owner,
                    kind,
                    balance,
                }
                .encode(),
            )
            .send_sync(
                Actor::new(definition, program),
                &Message::BurnSupply { owner, amount },
            ))
        }
        Message::BurnSupply { owner, amount } => {
            let definition = input.receiver.account_id;
            if input.from
                != Some(Actor::new(
                    holding_address(program, owner, definition),
                    program,
                ))
            {
                return Err(Error::Unauthorized);
            }
            let TokenState::Definition {
                name,
                kind,
                supply,
                metadata,
            } = TokenState::decode(&input.pre_state)?
            else {
                return Err(Error::WrongDefinition);
            };
            let supply = supply
                .checked_sub(amount)
                .ok_or(Error::InsufficientBalance)?;
            Ok(Response::set_state(
                TokenState::Definition {
                    name,
                    kind,
                    supply,
                    metadata,
                }
                .encode(),
            ))
        }
        Message::Mint { recipient, amount } => {
            if !input.is_authorized {
                return Err(Error::Unauthorized);
            }
            let TokenState::Definition {
                name,
                kind,
                supply,
                metadata,
            } = TokenState::decode(&input.pre_state)?
            else {
                return Err(Error::WrongDefinition);
            };
            if kind != TokenKind::Fungible {
                return Err(Error::InvalidNft);
            }
            let supply = supply.checked_add(amount).ok_or(Error::Overflow)?;
            let definition = input.receiver.account_id;
            Ok(Response::set_state(
                TokenState::Definition {
                    name,
                    kind,
                    supply,
                    metadata,
                }
                .encode(),
            )
            .send_sync(
                Actor::new(holding_address(program, recipient, definition), program),
                &Message::Credit {
                    definition,
                    owner: recipient,
                    kind,
                    amount,
                },
            ))
        }
        Message::Create {
            name,
            kind,
            supply,
            recipient,
            metadata,
        } => {
            if !input.is_authorized {
                return Err(Error::Unauthorized);
            }
            if !input.pre_state.is_empty() {
                return Err(Error::AlreadyInitialized);
            }
            if kind == TokenKind::NftMaster && supply != 1 {
                return Err(Error::InvalidNft);
            }
            let definition = input.receiver.account_id;
            Ok(Response::set_state(
                TokenState::Definition {
                    name,
                    kind,
                    supply,
                    metadata,
                }
                .encode(),
            )
            .send_sync(
                Actor::new(holding_address(program, recipient, definition), program),
                &Message::Credit {
                    definition,
                    owner: recipient,
                    kind,
                    amount: supply,
                },
            ))
        }
        Message::Credit {
            definition,
            owner,
            kind,
            amount,
        } => {
            if !input.from_own_program() {
                return Err(Error::Unauthorized);
            }
            if input.receiver.account_id != holding_address(program, owner, definition) {
                return Err(Error::WrongAccount);
            }
            let previous = if input.pre_state.is_empty() {
                0
            } else {
                match TokenState::decode(&input.pre_state)? {
                    TokenState::Holding {
                        definition: stored,
                        owner: holder,
                        kind: stored_kind,
                        balance,
                    } if stored == definition && holder == owner && stored_kind == kind => balance,
                    _ => return Err(Error::WrongDefinition),
                }
            };
            let balance = previous.checked_add(amount).ok_or(Error::Overflow)?;
            if kind == TokenKind::NftMaster && balance > 1 {
                return Err(Error::InvalidNft);
            }
            Ok(Response::set_state(
                TokenState::Holding {
                    definition,
                    owner,
                    kind,
                    balance,
                }
                .encode(),
            ))
        }
        Message::Transfer {
            definition,
            recipient,
            amount,
        } => {
            if !input.is_authorized {
                return Err(Error::Unauthorized);
            }
            let owner = input.receiver.account_id;
            Ok(Response::keep_state().send_sync(
                Actor::new(holding_address(program, owner, definition), program),
                &Message::Debit {
                    definition,
                    owner,
                    recipient,
                    amount,
                },
            ))
        }
        Message::Debit {
            definition,
            owner,
            recipient,
            amount,
        } => {
            if input.from != Some(Actor::new(owner, program)) {
                return Err(Error::Unauthorized);
            }
            if input.receiver.account_id != holding_address(program, owner, definition) {
                return Err(Error::WrongAccount);
            }
            let TokenState::Holding {
                definition: stored,
                owner: holder,
                kind,
                balance,
            } = TokenState::decode(&input.pre_state)?
            else {
                return Err(Error::WrongDefinition);
            };
            if stored != definition || holder != owner {
                return Err(Error::WrongDefinition);
            }
            let balance = balance
                .checked_sub(amount)
                .ok_or(Error::InsufficientBalance)?;
            Ok(Response::set_state(
                TokenState::Holding {
                    definition,
                    owner,
                    kind,
                    balance,
                }
                .encode(),
            )
            .send_sync(
                Actor::new(holding_address(program, recipient, definition), program),
                &Message::Credit {
                    definition,
                    owner: recipient,
                    kind,
                    amount,
                },
            ))
        }
    }
}
