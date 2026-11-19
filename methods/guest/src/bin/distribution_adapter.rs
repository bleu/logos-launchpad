//! Public percentage distribution adapter.
use lee_core::program::{ReceiveInput, Response};

lee_core::actor_entrypoint!(raw handle);

fn handle(input: &ReceiveInput) -> Response {
    distribution_adapter::handle(input).expect("distribution adapter failed")
}
