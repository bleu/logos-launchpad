use authority_probe::{Instruction, dispatch};
use spel_framework::prelude::{ProgramOutput, read_lee_inputs};

fn main() {
    let (input, words) = read_lee_inputs::<([u8; 32], Instruction)>();
    let output = dispatch(
        input.self_program_id,
        input.instruction.0,
        input.pre_states.clone(),
        input.instruction.1,
    )
    .expect("authority probe rejected instruction");
    ProgramOutput::new(
        input.self_program_id,
        input.caller_program_id,
        words,
        input.pre_states,
        output.post_states,
    )
    .write();
}
