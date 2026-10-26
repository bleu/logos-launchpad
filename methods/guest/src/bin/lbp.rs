//! Fixed-price setup actor entry point.
use lee_core::program::{ReceiveInput, Response};

lee_core::actor_entrypoint!(raw handle);

fn handle(input: &ReceiveInput) -> Response {
    lbp_core::handle(input).expect("fixed-price setup failed")
}
