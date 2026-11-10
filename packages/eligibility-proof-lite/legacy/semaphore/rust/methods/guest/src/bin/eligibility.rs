use lee_core::program::{ReceiveInput, Response};
use serde::Deserialize;

lee_core::actor_entrypoint!(raw handle);

#[derive(Deserialize)]
struct FixturePolicy {
    #[serde(rename = "publicSignals")]
    public_signals: [String; 4],
}

fn handle(input: &ReceiveInput) -> Response {
    let policy: FixturePolicy = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../fixtures/proof.json"
    )))
    .expect("fixed fixture policy");
    let proof: Vec<u8> = borsh::from_slice(&input.message).expect("proof message");
    let expected = policy.public_signals.each_ref().map(String::as_str);
    eligibility_proof_verifier::verify_for(&proof, &expected).expect("eligibility proof rejected");
    Response::keep_state()
}
