use admin_authority::{AdminCandidate, AdminConfig, AdminError};
use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, ShardData},
    program::{PdaSeed, Plan, PlanInput, run_program},
};
use spel_framework::prelude as legacy;

type Record = (Vec<u8>, bool, bool, u8);

#[derive(BorshSerialize, BorshDeserialize)]
enum Effect {
    Initialize(AccountId),
    CheckOwnerShard,
    TryTransfer {
        current: AccountId,
        successor: AccountId,
        owner: AccountId,
        seed: [u8; 32],
        owner_approved: bool,
    },
}

fn plan(input: &PlanInput, (operation, seed): (u8, [u8; 32])) -> Plan {
    let mut plan = Plan::new(input);
    match operation {
        0 => {
            let [current, config] = <&[_; 2]>::try_from(input.accounts.as_slice()).unwrap();
            assert!(current.is_authorized);
            assert_eq!(current.account_id, config.account_id);
            plan.effect(config, &Effect::Initialize(current.account_id));
        }
        1 | 2 => {
            let [current, successor, config] =
                <&[_; 3]>::try_from(input.accounts.as_slice()).unwrap();
            assert!(current.is_authorized);
            assert!(successor.is_authorized);
            assert_eq!(current.account_id, config.account_id);
            let owner = input
                .caller_account_id
                .expect("owner-program call required");
            assert_eq!(
                successor.account_id,
                AccountId::for_public_pda(&owner, &PdaSeed::new(seed)),
            );
            if operation == 1 {
                plan.inspect(successor, owner, &Effect::CheckOwnerShard);
            }
            plan.effect(
                config,
                &Effect::TryTransfer {
                    current: current.account_id,
                    successor: successor.account_id,
                    owner,
                    seed,
                    owner_approved: operation == 2,
                },
            );
        }
        _ => panic!("unknown admission probe operation"),
    }
    plan
}

fn authorized(id: AccountId) -> legacy::AccountWithMetadata {
    legacy::AccountWithMetadata {
        account_id: legacy::AccountId::new(*id.value()),
        is_authorized: true,
        // A selected shard is not a legacy whole-account owner.
        account: legacy::Account::default(),
    }
}

fn apply(effect: Effect, pre_data: &ShardData) -> Option<ShardData> {
    let record: Record = match effect {
        Effect::Initialize(current) => {
            assert!(pre_data.is_empty());
            let admin = AdminConfig::initialize(legacy::AccountId::new(*current.value())).unwrap();
            (admin.encode().unwrap(), false, false, 0)
        }
        Effect::CheckOwnerShard => {
            assert_eq!(pre_data.as_ref(), &[1_u8]);
            return None;
        }
        Effect::TryTransfer {
            current,
            successor,
            owner,
            seed,
            owner_approved,
        } => {
            let (bytes, _, _, _): Record = borsh::from_slice(pre_data.as_ref()).unwrap();
            let mut admin = AdminConfig::decode(&bytes).unwrap();
            let program_id = std::array::from_fn(|i| {
                u32::from_le_bytes(owner.value()[i * 4..i * 4 + 4].try_into().unwrap())
            });
            // Plan checks PDA derivation and owner authorization before this signer check.
            let candidate = if owner_approved {
                AdminCandidate::Signer
            } else {
                AdminCandidate::Pda { program_id, seed }
            };
            let result = admin.transfer(&authorized(current), candidate, &authorized(successor));
            if owner_approved {
                result.unwrap();
                (admin.encode().unwrap(), true, true, 2)
            } else {
                assert_eq!(result, Err(AdminError::UndeployedPda));
                assert_eq!(admin.encode().unwrap(), bytes);
                (bytes, true, true, 1)
            }
        }
    };
    Some(borsh::to_vec(&record).unwrap().try_into().unwrap())
}

fn main() {
    run_program(plan, apply)
}
