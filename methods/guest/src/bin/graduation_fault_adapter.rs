//! Graduation adapter faults for transaction rollback tests.
use factory_core::{AdapterState, GraduationResult};
use lee_core::{
    account::AccountId,
    program::{ReceiveInput, Response, SyncMessage},
};

lee_core::actor_entrypoint!(raw handle);

fn handle(input: &ReceiveInput) -> Response {
    let mut response = factory_core::handle_adapter(input).expect("graduation adapter failed");
    if let Ok(state) = AdapterState::decode(&input.pre_state)
        && input.from == Some(state.factory)
        && input.message.ends_with(b"reject-callback")
    {
        panic!("callback rejected after transfers");
    }
    if let Some(state) = response.post_state.as_ref()
        && let Ok(state) = AdapterState::decode(state)
        && let Some(receipt) = state.receipt
        && matches!(
            receipt.configuration.as_slice(),
            b"corrupt-final-ack" | b"corrupt-request-hash"
        )
    {
        let acknowledgment = response
            .sync_messages
            .last_mut()
            .expect("graduation acknowledgment");
        acknowledgment.message = SyncMessage::new(
            acknowledgment.to,
            &GraduationResult {
                factory: if receipt.configuration == b"corrupt-final-ack" {
                    AccountId::new([0; 32])
                } else {
                    state.factory.account_id
                },
                request_hash: if receipt.configuration == b"corrupt-request-hash" {
                    [0; 32]
                } else {
                    receipt.hash()
                },
            },
        )
        .message;
    }
    if let Some(state) = response.post_state.as_ref()
        && let Ok(state) = AdapterState::decode(state)
        && let Some(receipt) = state.receipt
        && let Some(payload) = receipt.configuration.strip_prefix(b"nested-call:")
    {
        let target = lee_core::account::Actor::new(
            AccountId::new(payload[..32].try_into().expect("nested account")),
            AccountId::new(payload[32..64].try_into().expect("nested program")),
        );
        let mut nested = SyncMessage::new(target, &());
        nested.message = payload[64..].to_vec();
        response.sync_messages.insert(0, nested);
    }
    response
}
