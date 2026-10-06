use borsh::{BorshDeserialize, BorshSerialize};
use event_probe::compatibility::{Instruction, State};
use lee_core::{
    account::{AccountId, ShardData},
    program::{Plan, PlanInput, run_program},
};
use token_core::{TokenHolding, token_account_id};

#[derive(BorshSerialize, BorshDeserialize)]
enum Effect {
    CheckAuthority(AccountId),
    Authorize(AccountId),
}

fn plan(input: &PlanInput, instruction: Instruction) -> Plan {
    let [config, holder] = <&[_; 2]>::try_from(input.accounts.as_slice())
        .expect("expected config and authority holder");
    assert!(holder.is_authorized, "authority signature is missing");
    let Instruction::Authorize { definition } = instruction;
    let mut plan = Plan::new(input);
    plan.inspect(
        holder,
        token_account_id(),
        &Effect::CheckAuthority(definition),
    );
    plan.effect(config, &Effect::Authorize(definition));
    plan
}

fn apply(effect: Effect, pre_data: &ShardData) -> Option<ShardData> {
    match effect {
        Effect::CheckAuthority(definition) => {
            assert_eq!(
                TokenHolding::try_from(pre_data).expect("invalid authority token holding"),
                TokenHolding::NftMaster {
                    definition_id: definition,
                    print_balance: 1,
                },
                "authority requires the expected one-unit NFT master",
            );
            None
        }
        Effect::Authorize(definition) => {
            let mut config: State = borsh::from_slice(pre_data.as_ref()).expect("invalid config");
            assert_eq!(
                config.authority_definition, definition,
                "wrong authority definition"
            );
            config.authorizations = config.authorizations.checked_add(1).unwrap();
            Some(borsh::to_vec(&config).unwrap().try_into().unwrap())
        }
    }
}

fn main() {
    run_program(plan, apply)
}
