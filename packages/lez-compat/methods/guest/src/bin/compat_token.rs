use lee_core::program::{ReceiveInput, Response};

lee_core::actor_entrypoint!(raw handle);

fn handle(input: &ReceiveInput) -> Response {
    lez_compat_core::token::handle(input).expect("token operation rejected")
}
