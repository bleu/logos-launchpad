#[allow(dead_code)] // The build uses only key-decoding error variants.
#[path = "src/encoding.rs"]
mod encoding;

use ark_bn254::Bn254;
use ark_groth16::{prepare_verifying_key, VerifyingKey};
use ark_serialize::CanonicalSerialize;
use encoding::{g1, g2, VerifyError};
use serde::Deserialize;
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=../../fixtures/verification-key.json");
    println!("cargo:rerun-if-changed=src/encoding.rs");
    let key = verification_key().expect("valid fixed Semaphore verification key");
    let prepared = prepare_verifying_key(&key);
    let mut bytes = Vec::new();
    prepared
        .serialize_uncompressed(&mut bytes)
        .expect("serialize fixed key");
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").expect("build output directory"))
            .join("prepared-key.bin"),
        bytes,
    )
    .expect("write prepared key");
}

#[derive(Deserialize)]
struct SnarkjsVerificationKey {
    protocol: String,
    curve: String,
    #[serde(rename = "nPublic")]
    n_public: usize,
    vk_alpha_1: [String; 3],
    vk_beta_2: [[String; 2]; 3],
    vk_gamma_2: [[String; 2]; 3],
    vk_delta_2: [[String; 2]; 3],
    #[serde(rename = "IC")]
    ic: [[String; 3]; 5],
}

fn verification_key() -> Result<VerifyingKey<Bn254>, VerifyError> {
    let key: SnarkjsVerificationKey =
        serde_json::from_str(include_str!("../../fixtures/verification-key.json"))
            .map_err(|_| VerifyError::InvalidVerificationKey)?;
    if key.protocol != "groth16" || key.curve != "bn128" || key.n_public != 4 {
        return Err(VerifyError::InvalidVerificationKey);
    }
    Ok(VerifyingKey {
        alpha_g1: g1(&key.vk_alpha_1)?,
        beta_g2: g2(&key.vk_beta_2)?,
        gamma_g2: g2(&key.vk_gamma_2)?,
        delta_g2: g2(&key.vk_delta_2)?,
        gamma_abc_g1: key
            .ic
            .iter()
            .map(|point| g1(point))
            .collect::<Result<_, _>>()?,
    })
}
