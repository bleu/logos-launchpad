use eligibility_proof_verifier::{PROOF_BYTES, verify_for};
use lee_core::program::{ReceiveInput, Response};

lee_core::actor_entrypoint!(raw handle);

fn handle(input: &ReceiveInput) -> Response {
    // This experiment checks signed self-execution, not production pool callers.
    assert!(input.from.is_none(), "self-execution only");
    assert!(input.is_authorized, "receiver authorization required");
    assert_eq!(
        input.message.len(),
        PROOF_BYTES + 4,
        "bounded proof message"
    );
    let proof: Vec<u8> = borsh::from_slice(&input.message).expect("canonical proof message");
    verify_for(
        input.pre_state.as_ref(),
        &input.receiver.account_id.to_bytes(),
        &proof,
    )
    .expect("approved public trader required");
    Response::keep_state()
}
