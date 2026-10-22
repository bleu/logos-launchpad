//! Private token execution is unavailable in the selected compatibility interface.

use lee_core::program::{ReceiveInput, Response};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    PrivatePurchaseUnsupported,
    PrivateAuthorityUnsupported,
}

pub fn handle_private_buy(_input: &ReceiveInput) -> Result<Response, Error> {
    Err(Error::PrivatePurchaseUnsupported)
}

pub fn handle_private_authority(_input: &ReceiveInput) -> Result<Response, Error> {
    Err(Error::PrivateAuthorityUnsupported)
}
