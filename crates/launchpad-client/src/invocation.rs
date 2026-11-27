use borsh::BorshSerialize;
use lee::AccountId;
use lee_core::account::Actor;
use lez_compat_core::token;
use wallet::AccountIdentity;

use crate::{ClientError, Result, TOKEN_PROGRAM_ACCOUNT_ID};

/// Typed message and declared actor states. Actor order does not encode instruction arguments.
#[derive(Debug, Clone)]
pub struct PublicInvocation<I> {
    pub destination: Actor,
    pub actors: Vec<Actor>,
    pub signer_accounts: Vec<AccountId>,
    pub instruction: I,
    pub fee_payer: Option<AccountId>,
    pub pda_accounts: Vec<(AccountId, lee_core::program::PdaSeed)>,
}
impl<I> PublicInvocation<I> {
    /// Wallet mentions retain admission evidence without granting PDA authorization.
    pub fn account_mentions(&self) -> Vec<wallet::AccountMention> {
        self.actors
            .iter()
            .map(|actor| {
                let signs = self.signer_accounts.contains(&actor.account_id);
                let identity = if signs {
                    AccountIdentity::Public(actor.account_id)
                } else if let Some((program, seed)) =
                    self.pda_accounts.iter().find(|(program, seed)| {
                        AccountId::for_public_pda(program, seed) == actor.account_id
                    })
                {
                    AccountIdentity::PublicPda {
                        program: *program,
                        seed: *seed,
                    }
                } else {
                    AccountIdentity::PublicNoSign(actor.account_id)
                };
                let mention = identity.select_program_actor_state(actor.program_account_id);
                if signs {
                    mention
                } else {
                    mention.without_authorization()
                }
            })
            .collect()
    }
    pub(super) fn with_pda(mut self, program: AccountId, seed: lee_core::program::PdaSeed) -> Self {
        if !self.pda_accounts.contains(&(program, seed)) {
            self.pda_accounts.push((program, seed));
        }
        self
    }
    pub(super) fn with_holding(self, owner: AccountId, definition: AccountId) -> Self {
        self.with_pda(
            TOKEN_PROGRAM_ACCOUNT_ID,
            token::holding_seed(owner, definition),
        )
    }
}
impl<I: BorshSerialize> PublicInvocation<I> {
    pub fn message_bytes(&self) -> Result<Vec<u8>> {
        borsh::to_vec(&self.instruction).map_err(ClientError::EncodeMessage)
    }
}

pub(super) fn invocation<I>(
    destination: Actor,
    mut actors: Vec<Actor>,
    signers: Vec<AccountId>,
    instruction: I,
) -> PublicInvocation<I> {
    actors.insert(0, destination);
    let mut unique = Vec::new();
    for actor in actors {
        if !unique.contains(&actor) {
            unique.push(actor);
        }
    }
    PublicInvocation {
        destination,
        actors: unique,
        signer_accounts: signers,
        instruction,
        fee_payer: None,
        pda_accounts: vec![],
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FactoryPoolAddresses {
    pub(super) factory: AccountId,
    pub(super) token_definition: AccountId,
    pub(super) creator_escrow: AccountId,
    pub(super) pool: AccountId,
}

pub(super) fn associated_token_account(owner: AccountId, definition: AccountId) -> AccountId {
    token::holding_address(curve_core::TOKEN_PROGRAM_ACCOUNT_ID, owner, definition)
}
pub(super) fn token_actor(account: AccountId) -> Actor {
    Actor::new(account, curve_core::TOKEN_PROGRAM_ACCOUNT_ID)
}
pub(super) fn clock_actor() -> Actor {
    clock_core::clock_actor()
}
