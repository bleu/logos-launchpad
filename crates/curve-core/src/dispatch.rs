//! Actor reads bind each reply to the requested account and program.
use crate::{Config, Error, Message, PoolAccount, TOKEN_PROGRAM_ACCOUNT_ID, compute_config_pda};
use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response, StateReply, SyncMessage},
};
use lez_compat_core::token::{self, TokenKind, TokenState};

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub(crate) enum Query {
    OwnerRole,
    Namespace,
    Definition0,
    Definition1,
    Vault0,
    Vault1,
    AllocationBlock,
}
#[derive(BorshSerialize, BorshDeserialize)]
pub(crate) struct Pending {
    pub tag: [u8; 8],
    pub state: PoolAccount,
    pub message: Message,
    pub authority: AccountId,
    pub queries: Vec<Query>,
    pub config: Option<Config>,
    pub balances: [u128; 2],
    pub allocation_block: Option<u64>,
}
impl Pending {
    pub fn new(
        state: PoolAccount,
        message: Message,
        authority: AccountId,
        queries: Vec<Query>,
    ) -> Self {
        Self {
            tag: *b"LZCVPEND",
            state,
            message,
            authority,
            queries,
            config: None,
            balances: [0; 2],
            allocation_block: None,
        }
    }
    fn expected(&self, query: &Query, program: AccountId) -> Actor {
        let account = match query {
            Query::OwnerRole => {
                return Actor::new(
                    crate::compute_pool_owner_role(
                        self.state.namespace,
                        crate::compute_pool_pda(
                            self.state.namespace,
                            program,
                            self.state.token0_definition_id,
                            self.state.token1_definition_id,
                            self.state.owner,
                        ),
                        program,
                    ),
                    crate::AUTHORITY_PROGRAM_ACCOUNT_ID,
                );
            }
            Query::Namespace => {
                return Actor::new(compute_config_pda(self.state.namespace, program), program);
            }
            Query::AllocationBlock => return clock_core::clock_actor(),
            Query::Definition0 => self.state.token0_definition_id,
            Query::Definition1 => self.state.token1_definition_id,
            Query::Vault0 | Query::Vault1 => {
                let owner = crate::compute_pool_pda(
                    self.state.namespace,
                    program,
                    self.state.token0_definition_id,
                    self.state.token1_definition_id,
                    self.state.owner,
                );
                token::holding_address(
                    TOKEN_PROGRAM_ACCOUNT_ID,
                    owner,
                    if matches!(query, Query::Vault0) {
                        self.state.token0_definition_id
                    } else {
                        self.state.token1_definition_id
                    },
                )
            }
        };
        Actor::new(account, TOKEN_PROGRAM_ACCOUNT_ID)
    }
    pub fn advance(self, receiver: Actor) -> Result<Response, Error> {
        self.state.validate_address(receiver)?;
        let Some(query) = self.queries.first() else {
            return match self.message {
                Message::SwapExactInput { .. } | Message::SwapExactOutput { .. } => {
                    crate::pool_swap::finish(self, receiver)
                }
                Message::WithdrawReserves { .. }
                | Message::ClosePool { .. }
                | Message::SetPaused { .. }
                | Message::CollectFees => crate::pool_lifecycle::finish(self, receiver),
                _ => crate::pool_create::finish(self, receiver),
            };
        };
        let call = match query {
            Query::OwnerRole => SyncMessage::new(
                Actor::new(self.authority, crate::AUTHORITY_PROGRAM_ACCOUNT_ID),
                &lez_compat_core::authority::Message::Check {
                    role: self.expected(query, receiver.program_account_id).account_id,
                    callback: receiver,
                },
            ),
            Query::Namespace => SyncMessage::new(
                self.expected(query, receiver.program_account_id),
                &Message::ReadNamespace {
                    namespace: self.state.namespace,
                },
            ),
            Query::AllocationBlock => SyncMessage::new(
                clock_core::clock_actor(),
                &clock_core::Message::ReadBlockId { granularity: 1 },
            ),
            _ => SyncMessage::new(
                self.expected(query, receiver.program_account_id),
                &token::Message::ReadState,
            ),
        };
        Ok(Response::set_state(borsh::to_vec(&self).unwrap()).send(call))
    }
}
pub fn resume(input: &ReceiveInput) -> Option<Result<Response, Error>> {
    if !input.pre_state.starts_with(b"LZCVPEND") {
        return None;
    }
    Some((|| {
        let mut pending: Pending =
            borsh::from_slice(&input.pre_state).map_err(|_| Error::InvalidState)?;
        pending.state.validate_address(input.receiver)?;
        let query = pending.queries.first().ok_or(Error::InvalidState)?.clone();
        let expected = pending.expected(&query, input.receiver.program_account_id);
        if input.from != Some(expected) {
            return Err(Error::Unauthorized);
        }
        match query {
            Query::OwnerRole => {
                lez_compat_core::authority::verify(input, expected, pending.authority)
                    .map_err(|_| Error::Unauthorized)?
            }
            Query::Namespace => {
                let reply: StateReply =
                    borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
                let config = Config::decode(&reply.state)?;
                if config.namespace != pending.state.namespace {
                    return Err(Error::WrongAccount);
                }
                pending.config = Some(config);
            }
            Query::AllocationBlock => {
                let clock_core::Reply::BlockId(block) =
                    borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?
                else {
                    return Err(Error::InvalidMessage);
                };
                pending.allocation_block = Some(block);
            }
            Query::Definition0 | Query::Definition1 => {
                if !matches!(
                    token::read_reply(input, expected).map_err(|_| Error::InvalidToken)?,
                    TokenState::Definition {
                        kind: TokenKind::Fungible,
                        ..
                    }
                ) {
                    return Err(Error::InvalidToken);
                }
            }
            Query::Vault0 | Query::Vault1 => {
                let side = usize::from(matches!(query, Query::Vault1));
                let definition = if side == 0 {
                    pending.state.token0_definition_id
                } else {
                    pending.state.token1_definition_id
                };
                let TokenState::Holding {
                    definition: stored,
                    owner,
                    kind: TokenKind::Fungible,
                    balance,
                } = token::read_reply(input, expected).map_err(|_| Error::InvalidToken)?
                else {
                    return Err(Error::InvalidToken);
                };
                if stored != definition || owner != input.receiver.account_id {
                    return Err(Error::InvalidToken);
                }
                pending.balances[side] = balance;
            }
        }
        pending.queries.remove(0);
        pending.advance(input.receiver)
    })())
}
