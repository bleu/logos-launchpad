//! Corrupt only the final LBP completion reply in disposable test fixtures.
use lbp_core::SettlementReply;
use lee_core::program::{ReceiveInput, Response, SyncMessage};

lee_core::actor_entrypoint!(raw handle);

fn handle(input: &ReceiveInput) -> Response {
    let mut response = lbp_core::handle(input).expect("LBP completion fault wrapper failed");
    for call in &mut response.sync_messages {
        if let Ok(SettlementReply::Completed { progress, .. }) =
            borsh::from_slice::<SettlementReply>(&call.message)
        {
            call.message = SyncMessage::new(
                call.to,
                &SettlementReply::Completed {
                    receipt_hash: [0; 32],
                    progress,
                },
            )
            .message;
        }
    }
    response
}
