//! Atomic deshield, authorized app action, and re-shield of an authority NFT.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use lee_core::program::{ProgramOutput, read_lee_inputs};
use private_flow_core::{PrivateAuthorityInstruction, authority_action};
use {
    associated_token_account_core as _, authenticated_transfer_core as _, curve_core as _,
    factory_core as _, risc0_zkvm as _, token_core as _,
};

fn main() {
    let (input, words) = read_lee_inputs::<PrivateAuthorityInstruction>();
    let (posts, calls) = authority_action(input.pre_states.clone(), input.instruction);
    ProgramOutput::new(
        input.self_program_id,
        input.caller_program_id,
        words,
        input.pre_states,
        posts,
    )
    .with_chained_calls(calls)
    .write();
}
