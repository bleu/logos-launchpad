use eligibility_core::native::{PurchaseInstruction, purchase};
use lee_core::program::read_lee_inputs;

fn main() {
    std::panic::set_hook(Box::new(|_| eprintln!("native guest rejected")));
    let (input, words) = read_lee_inputs::<PurchaseInstruction>();
    match purchase(input, words) {
        Some(output) => output.write(),
        None => risc0_zkvm::guest::env::exit(1),
    }
}
