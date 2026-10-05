#![no_main]

use eligibility_core::{EligibilityInput, check_eligibility};
use risc0_zkvm::guest::env;

risc0_zkvm::guest::entry!(main);

fn main() {
    let input: EligibilityInput = env::read();
    env::commit(&check_eligibility(&input.policy, &input.purchase, &input.witness).ok());
}
