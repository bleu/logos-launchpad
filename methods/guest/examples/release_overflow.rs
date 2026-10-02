#![no_main]

use risc0_zkvm::guest::env;

risc0_zkvm::guest::entry!(main);

fn main() {
    let (left, right): (u64, u64) = env::read();
    env::commit(&(left + right));
}
