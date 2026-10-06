//! Platform clock-read probe; this is not a sale program.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use clock_core::{CLOCK_01_PROGRAM_ACCOUNT_ID, ClockAccountData};
use lee_core::{
    account::AccountWithMetadata,
    program::{AccountPostState, ProgramOutput, read_lee_inputs},
};
use {
    associated_token_account_core as _, authenticated_transfer_core as _, curve_core as _,
    factory_core as _, private_flow_core as _, risc0_zkvm as _, token_core as _,
};

fn main() {
    let (input, instruction_words) = read_lee_inputs::<()>();
    let [clock, result]: [AccountWithMetadata; 2] = input
        .pre_states
        .clone()
        .try_into()
        .expect("clock probe requires clock and result accounts");
    assert_eq!(
        clock.account_id, CLOCK_01_PROGRAM_ACCOUNT_ID,
        "invalid clock"
    );
    let timestamp =
        ClockAccountData::from_bytes(&clock.account.data.clone().into_inner()).timestamp;
    let mut observed = result.account;
    observed.data = timestamp.to_le_bytes().to_vec().try_into().unwrap();
    let posts = vec![
        AccountPostState::new(clock.account),
        AccountPostState::new(observed),
    ];
    ProgramOutput::new(
        input.self_program_id,
        input.caller_program_id,
        instruction_words,
        input.pre_states,
        posts,
    )
    .write();
}
