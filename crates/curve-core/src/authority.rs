//! Pool-owner NFTs remain separate from namespace roles.
use crate::{Error, PoolAccount, dispatch::Query};
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, ReceiveInput},
};

pub(crate) fn owner(
    input: &ReceiveInput,
    state: &PoolAccount,
) -> Result<(AccountId, Vec<Query>), Error> {
    let sender = input.from.ok_or(Error::Unauthorized)?;
    match state.owner_program {
        Some((program, seed)) => {
            if sender != Actor::new(state.owner, program)
                || state.owner != AccountId::for_public_pda(&program, &PdaSeed::new(seed))
            {
                return Err(Error::Unauthorized);
            }
            Ok((state.owner, vec![]))
        }
        None => {
            if sender.program_account_id != input.receiver.program_account_id {
                return Err(Error::Unauthorized);
            }
            Ok((sender.account_id, vec![Query::OwnerNft]))
        }
    }
}
