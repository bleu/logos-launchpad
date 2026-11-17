//! Narrow helpers for the upstream clock. Sale time policy remains in each caller.
use crate::Error;
use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::Actor,
    program::{ReceiveInput, Response},
};

const TAG: [u8; 8] = *b"LZCLK001";
#[derive(BorshSerialize, BorshDeserialize)]
struct Pending {
    tag: [u8; 8],
    receiver: Actor,
    at_least: u64,
    at_most: u64,
    response: Response,
}

pub fn request() -> clock_core::Message {
    clock_core::Message::ReadTimestamp { granularity: 1 }
}

pub fn read(input: &ReceiveInput) -> Result<u64, Error> {
    if input.from != Some(clock_core::clock_actor()) {
        return Err(Error::Unauthorized);
    }
    match borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)? {
        clock_core::Reply::Timestamp(timestamp) => Ok(timestamp),
        clock_core::Reply::BlockId(_) => Err(Error::InvalidMessage),
    }
}

/// The response must set its final state, so the pending state cannot survive completion.
/// Defer a response until the canonical clock confirms an inclusive interval.
pub fn guard(
    receiver: Actor,
    response: Response,
    at_least: u64,
    at_most: u64,
) -> Result<Response, Error> {
    if response.post_state.is_none() {
        return Err(Error::InvalidState);
    }
    if at_least > at_most {
        return Err(Error::OutsideTimeBounds);
    }
    let pending = Pending {
        tag: TAG,
        receiver,
        response,
        at_least,
        at_most,
    };
    Ok(
        Response::set_state(borsh::to_vec(&pending).map_err(|_| Error::InvalidState)?)
            .send_sync(clock_core::clock_actor(), &request()),
    )
}

/// Handle only this adapter's pending state, before decoding the caller's domain state.
pub fn resume(input: &ReceiveInput) -> Option<Result<Response, Error>> {
    if !input.pre_state.starts_with(&TAG) {
        return None;
    }
    Some((|| {
        let pending: Pending =
            borsh::from_slice(&input.pre_state).map_err(|_| Error::InvalidState)?;
        if input.receiver != pending.receiver {
            return Err(Error::WrongAccount);
        }
        let timestamp = read(input)?;
        if !(pending.at_least..=pending.at_most).contains(&timestamp) {
            return Err(Error::OutsideTimeBounds);
        }
        Ok(pending.response)
    })())
}
