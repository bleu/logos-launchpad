//! Test-only owner program for signed namespace PDA administration.
use curve_core::Instruction;
use lee_core::{
    account::{Account, AccountId},
    program::{
        AccountPostState, ChainedCall, Claim, PdaSeed, ProgramId, ProgramOutput, read_lee_inputs,
    },
};
use {
    associated_token_account_core as _, authenticated_transfer_core as _, factory_core as _,
    private_flow_core as _, risc0_zkvm as _, token_core as _,
};

fn main() {
    let (input, words) = read_lee_inputs::<(ProgramId, [u8; 32], Option<Instruction>)>();
    let (program, seed, action) = &input.instruction;
    let pda_seed = PdaSeed::new(*seed);
    let pda = &input.pre_states[0];
    let controller = &input.pre_states[1];
    assert_eq!(
        pda.account_id,
        AccountId::for_public_pda(&input.self_program_id, &pda_seed)
    );
    assert!(controller.is_authorized, "Controller must sign");
    let mut posts: Vec<_> = input
        .pre_states
        .iter()
        .map(|a| AccountPostState::new(a.account.clone()))
        .collect();
    let mut calls = vec![];
    if action.is_none() {
        assert_eq!(pda.account, Account::default());
        let mut account = pda.account.clone();
        account.data = controller
            .account_id
            .to_bytes()
            .to_vec()
            .try_into()
            .unwrap();
        posts[0] = AccountPostState::new_claimed_if_default(account, Claim::Pda(pda_seed));
    } else {
        assert_eq!(pda.account.program_owner, input.self_program_id);
        assert_eq!(pda.account.data.as_ref(), controller.account_id.to_bytes());
        let mut signer = pda.clone();
        signer.is_authorized = true;
        let action = action.as_ref().unwrap();
        let mut accounts = vec![input.pre_states[2].clone()];
        if matches!(action, Instruction::InitializeNamespace { .. }) {
            accounts.push(input.pre_states[3].clone());
        }
        accounts.push(signer);
        calls.push(ChainedCall::new(*program, accounts, action).with_pda_seeds(vec![pda_seed]));
    }
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
