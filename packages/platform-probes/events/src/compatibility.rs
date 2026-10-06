use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::account::AccountId;

#[derive(BorshSerialize, BorshDeserialize)]
pub enum Instruction {
    Authorize { definition: AccountId },
}

#[derive(Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct State {
    pub authority_definition: AccountId,
    pub authorizations: u64,
}
