//! Upstream clock adapter for weighted pricing. The schedule uses Unix seconds.
use crate::Error;
use lee_core::{account::Actor, program::ReceiveInput};
pub fn actor() -> Actor {
    clock_core::clock_actor()
}
pub fn request() -> clock_core::Message {
    lez_compat_core::clock::request()
}
pub fn read(input: &ReceiveInput) -> Result<u64, Error> {
    lez_compat_core::clock::read(input)
        .map(|timestamp| timestamp / 1000)
        .map_err(|error| match error {
            lez_compat_core::Error::Unauthorized => Error::Unauthorized,
            _ => Error::InvalidMessage,
        })
}
