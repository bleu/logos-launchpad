//! Temporary fixture pricing clock used by all builds. Replace before real sales.
//! Tests set its saved milliseconds; the clock does not advance itself.
use crate::Error;
use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response},
};

pub const ACCOUNT: AccountId = AccountId::new([201; 32]);
pub const PROGRAM: AccountId = AccountId::new([202; 32]);
pub fn actor() -> Actor {
    Actor::new(ACCOUNT, PROGRAM)
}
#[derive(BorshSerialize, BorshDeserialize)]
pub enum Request {
    ReadTimestamp,
}
#[derive(BorshSerialize, BorshDeserialize)]
pub struct Reply {
    pub timestamp_ms: u64,
}
pub fn read(input: &ReceiveInput) -> Result<u64, Error> {
    if input.from != Some(actor()) {
        return Err(Error::Unauthorized);
    }
    let reply: Reply = borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    Ok(reply.timestamp_ms / 1000)
}
/// TEMPORARY fake actor: fixture state is milliseconds, not trusted chain time.
/// No message can set its time. Only the test harness seeds/advances its state.
pub fn mock_handle(input: &ReceiveInput) -> Result<Response, Error> {
    if input.receiver != actor() {
        return Err(Error::WrongAccount);
    }
    let _: Request = borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
    let timestamp_ms: u64 = borsh::from_slice(&input.pre_state).map_err(|_| Error::InvalidState)?;
    Ok(Response::keep_state().send_sync(
        input.from.ok_or(Error::Unauthorized)?,
        &Reply { timestamp_ms },
    ))
}
