//! Test fixture: corrupt the final callback after real distribution completes.
use lee_core::program::{ReceiveInput, Response};
lee_core::actor_entrypoint!(raw handle);
fn handle(input: &ReceiveInput) -> Response {
    let mut response = distribution_adapter::handle(input).expect("distribution adapter failed");
    if let Some(bytes) = &response.post_state
        && let Ok(state) = distribution_adapter::State::decode(bytes)
        && state.receipt.is_some()
        && state.is_idle()
    {
        for message in &mut response.sync_messages {
            if message.to == state.factory
                && let Ok(mut result) =
                    borsh::from_slice::<factory_core::GraduationResult>(&message.message)
            {
                result.request_hash[0] ^= 1;
                message.message = borsh::to_vec(&result).unwrap();
            }
        }
    }
    response
}
