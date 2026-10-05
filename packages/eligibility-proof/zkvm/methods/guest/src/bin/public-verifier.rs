#![no_main]

#[path = "../../../../host/src/public_verification.rs"]
mod public_verification;
#[path = "../trusted_image.rs"]
mod trusted_image;

use risc0_zkvm::guest::env;

risc0_zkvm::guest::entry!(main);

fn main() {
    let input: public_verification::PublicVerification = env::read();
    env::commit(
        &public_verification::verify(&input, trusted_image::EXPECTED_ELIGIBILITY_ID).is_ok(),
    );
}
