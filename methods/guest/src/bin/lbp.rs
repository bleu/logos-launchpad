//! Fixed-price sale guest dispatch shim. Domain handlers live in `lbp-core`.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use lbp_core::{Instruction, process_instruction};
use lee_core::program::{ProgramInput, ProgramOutput, read_lee_inputs};
use {
    associated_token_account_core as _, authenticated_transfer_core as _, curve_core as _,
    factory_core as _, private_flow_core as _, risc0_zkvm as _, token_core as _,
};

fn main() {
    let (input, instruction_words) = read_lee_inputs::<Instruction>();
    let ProgramInput {
        self_program_id,
        caller_program_id,
        pre_states,
        instruction,
    } = input;
    let pre_states_clone = pre_states.clone();
    let (post_states, chained_calls) =
        process_instruction(pre_states, instruction, self_program_id);
    ProgramOutput::new(
        self_program_id,
        caller_program_id,
        instruction_words,
        pre_states_clone,
        post_states,
    )
    .with_chained_calls(chained_calls)
    .write();
}
