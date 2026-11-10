mod encoding;

use ark_bn254::{Bn254, Fr};
use ark_groth16::{Groth16, PreparedVerifyingKey, Proof};
use ark_serialize::CanonicalDeserialize;
use encoding::{decimal, g1, g2};
use serde::Deserialize;

pub use encoding::VerifyError;

#[derive(Deserialize)]
struct Request {
    proof: SnarkjsProof,
    #[serde(rename = "publicSignals")]
    public_signals: [String; 4],
}

#[derive(Deserialize)]
struct SnarkjsProof {
    pi_a: Vec<String>,
    pi_b: Vec<[String; 2]>,
    pi_c: Vec<String>,
    protocol: String,
    curve: String,
}

/// Check a proof against the compiled depth-17 Semaphore verification key.
/// This checks cryptography only. Callers own root policy and nullifier state.
pub fn verify(input: &[u8]) -> Result<(), VerifyError> {
    let request: Request = serde_json::from_slice(input).map_err(|_| VerifyError::MalformedJson)?;
    let signals = [
        decimal::<Fr>(&request.public_signals[0])?,
        decimal::<Fr>(&request.public_signals[1])?,
        decimal::<Fr>(&request.public_signals[2])?,
        decimal::<Fr>(&request.public_signals[3])?,
    ];
    if request.proof.protocol != "groth16" || request.proof.curve != "bn128" {
        return Err(VerifyError::InvalidEncoding);
    }
    let proof = Proof {
        a: g1(&request.proof.pi_a)?,
        b: g2(&request.proof.pi_b)?,
        c: g1(&request.proof.pi_c)?,
    };
    // Only immutable bytes validated and prepared by build.rs use unchecked decoding.
    let mut bytes = include_bytes!(concat!(env!("OUT_DIR"), "/prepared-key.bin")).as_slice();
    let key = PreparedVerifyingKey::<Bn254>::deserialize_uncompressed_unchecked(&mut bytes)
        .map_err(|_| VerifyError::InvalidVerificationKey)?;
    if !bytes.is_empty() || key.vk.gamma_abc_g1.len() != 5 {
        return Err(VerifyError::InvalidVerificationKey);
    }
    let valid = Groth16::<Bn254>::verify_proof(&key, &proof, &signals)
        .map_err(|_| VerifyError::InvalidProof)?;
    if valid {
        Ok(())
    } else {
        Err(VerifyError::InvalidProof)
    }
}

/// Bind public inputs in order: root, nullifier, hash(message), hash(scope).
/// Expected inputs must come from the caller's policy, not the request.
pub fn verify_for(input: &[u8], expected_signals: &[&str; 4]) -> Result<(), VerifyError> {
    let request: Request = serde_json::from_slice(input).map_err(|_| VerifyError::MalformedJson)?;
    for (actual, expected) in request.public_signals.iter().zip(expected_signals) {
        decimal::<Fr>(expected)?;
        if actual != expected {
            return Err(VerifyError::PublicInputsMismatch);
        }
    }
    verify(input)
}
