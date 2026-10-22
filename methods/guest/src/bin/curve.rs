//! Curve guest entry point; domain handlers live in `curve-core`.

use lee_core::program::{ReceiveInput, Response};

lee_core::actor_entrypoint!(raw handle);

fn handle(input: &ReceiveInput) -> Response {
    curve_core::handle(input).expect("curve operation failed")
}
