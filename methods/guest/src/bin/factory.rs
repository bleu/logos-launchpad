//! Factory actor entry point.
use lee_core::program::{ReceiveInput, Response};

lee_core::actor_entrypoint!(raw handle);

fn handle(input: &ReceiveInput) -> Response {
    factory_core::handle(input).expect("factory operation failed")
}
