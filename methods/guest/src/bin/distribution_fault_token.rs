//! Test fixture: fail a nonzero recipient credit after earlier transfers.
use lee_core::{
    account::AccountId,
    program::{ReceiveInput, Response},
};
use lez_compat_core::token;
lee_core::actor_entrypoint!(raw handle);
fn handle(input: &ReceiveInput) -> Response {
    if let Ok(token::Message::Credit { owner, amount, .. }) = borsh::from_slice(&input.message)
        && owner == AccountId::new([181; 32])
        && amount != 0
    {
        panic!("distribution credit fault");
    }
    token::handle(input).expect("token operation failed")
}
