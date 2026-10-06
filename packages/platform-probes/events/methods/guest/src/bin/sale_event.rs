use event_probe::ProbeInstruction;
use lee_core::{
    account::ProgramShardSelector,
    program::{ChainedCall, Plan, ProgramCall, ProgramEvent, read_program_call},
};

fn main() {
    let ProgramCall::Plan(input, instruction) = read_program_call::<ProbeInstruction>() else {
        panic!("event probe has no apply effects")
    };
    let mut plan = Plan::new(&input);
    let event = match instruction {
        ProbeInstruction::Emit(event) => event,
        ProbeInstruction::EmitThenFail(event) => {
            plan.call(ChainedCall {
                program_account_id: input.self_account_id,
                shard_selectors: input
                    .accounts
                    .iter()
                    .map(ProgramShardSelector::from)
                    .collect(),
                instruction_data: borsh::to_vec(&ProbeInstruction::Fail).unwrap(),
                pda_seeds: vec![],
            });
            event
        }
        ProbeInstruction::Fail => risc0_zkvm::guest::env::exit(7),
    };
    plan.event(ProgramEvent {
        selector: *b"SALEOP01",
        data: borsh::to_vec(&event).unwrap(),
    });
    plan.write();
}
