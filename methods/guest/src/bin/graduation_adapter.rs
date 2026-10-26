//! Test graduation adapter actor entry point.
use lee_core::program::{ReceiveInput, Response};

lee_core::actor_entrypoint!(raw handle);

fn handle(input: &ReceiveInput) -> Response {
    factory_core::handle_adapter(input).expect("graduation adapter failed")
}
