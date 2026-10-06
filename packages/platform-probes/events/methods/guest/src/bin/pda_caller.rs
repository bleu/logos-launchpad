use lee_core::{
    account::{AccountId, ProgramShardSelector},
    program::{ChainedCall, PdaSeed, Plan, ProgramCall, apply_write, read_program_call},
};

fn main() {
    match read_program_call::<(AccountId, [u8; 32], u8)>() {
        ProgramCall::Plan(input, (callee, seed, operation)) => {
            assert_eq!(input.accounts.len(), 3);
            assert!(input.accounts[0].is_authorized);
            let pda = &input.accounts[1];
            assert_eq!(
                pda.account_id,
                AccountId::for_public_pda(&input.self_account_id, &PdaSeed::new(seed))
            );
            assert_eq!(pda.program_account_id, input.self_account_id);
            assert_eq!(input.accounts[2].program_account_id, callee);
            let mut plan = Plan::new(&input);
            plan.effect(pda, &());
            plan.call(
                ChainedCall::new(
                    callee,
                    input
                        .accounts
                        .iter()
                        .map(ProgramShardSelector::from)
                        .collect(),
                    &(operation, seed),
                )
                .with_pda_seeds(vec![PdaSeed::new(seed)]),
            );
            plan.write();
        }
        ProgramCall::Apply(input) => {
            assert_eq!(input.selector.program_account_id, input.self_account_id);
            apply_write(input, vec![1].try_into().unwrap());
        }
    }
}
