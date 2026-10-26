use lee_core::program::{ReceiveInput, Response};

lee_core::actor_entrypoint!(raw handle);

fn handle(input: &ReceiveInput) -> Response {
    private_flow_core::handle_private_authority(input)
        .expect("private authority handoff is not supported")
}
