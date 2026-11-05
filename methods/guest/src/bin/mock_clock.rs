//! TEMPORARY local timestamp fixture; not a LEZ system clock.
use lee_core::program::{ReceiveInput, Response};
lee_core::actor_entrypoint!(raw handle);
fn handle(input: &ReceiveInput) -> Response {
    lbp_core::pricing_clock::mock_handle(input).expect("mock clock failed")
}
