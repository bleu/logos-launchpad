use eligibility_core::{
    AccountControl, ApprovedAccount, Digest, EligibilityInput, EligibilityWitness, MembershipTree,
    Purchase, SaleContext, SalePolicy, TreeHash,
};
use eyre::Result;
use lee_core::{NullifierPublicKey, account::AccountId};
use std::time::Instant;

pub struct Prepared {
    pub public: EligibilityInput,
    pub private: EligibilityInput,
    pub account_seconds: f64,
    pub tree_seconds: f64,
    pub membership_seconds: f64,
}

pub fn prepare(hash: TreeHash) -> Result<Prepared> {
    let sale = SaleContext {
        deployment: [1; 32],
        factory: [2; 32],
        namespace: [3; 32],
        sale_account: [4; 32],
    };
    let started = Instant::now();
    let public_key = digest("0000000000000000000000000000000000000000000000000000000000000003");
    let private_key = digest("390540739938b833cfee63a593d6d5971efb1ec48616e0d3ed7888e1bcdcf91c");
    let npk = NullifierPublicKey::from(&private_key);
    let entries = (0_u32..100_000)
        .map(|index| {
            let account = if index == 0 {
                digest("ec2cbcd4488075c4b30678c30351327fdf99f3fe703edc2999b68f8fd12bdbc8")
            } else {
                *AccountId::for_regular_private_account(&npk, u128::from(index - 1)).value()
            };
            // Predictable blinding is for this synthetic benchmark only.
            let mut blinder = [5; 32];
            blinder[..4].copy_from_slice(&index.to_le_bytes());
            ApprovedAccount { account, blinder }
        })
        .collect();
    let account_seconds = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let tree = MembershipTree::prepare(&sale, hash, 17, entries)
        .map_err(|_| eyre::eyre!("synthetic tree preparation failed"))?;
    let tree_seconds = started.elapsed().as_secs_f64();
    let policy = SalePolicy {
        sale,
        root: tree.root(),
        hash,
        depth: 17,
        collateral_limit: 100,
    };
    let purchase = Purchase {
        trading_account: [6; 32],
        recipient: [7; 32],
        collateral_in: 10,
        tokens_out: 20,
    };
    let started = Instant::now();
    let public = EligibilityInput {
        policy: policy.clone(),
        purchase,
        witness: EligibilityWitness {
            control: AccountControl::Public { key: public_key },
            membership: tree
                .membership(0)
                .map_err(|_| eyre::eyre!("synthetic membership failed"))?,
        },
    };
    let private = EligibilityInput {
        policy,
        purchase,
        witness: EligibilityWitness {
            control: AccountControl::Private {
                nullifier_key: private_key,
                identifier: 0,
            },
            membership: tree
                .membership(1)
                .map_err(|_| eyre::eyre!("synthetic membership failed"))?,
        },
    };
    Ok(Prepared {
        public,
        private,
        account_seconds,
        tree_seconds,
        membership_seconds: started.elapsed().as_secs_f64(),
    })
}

fn digest(value: &str) -> Digest {
    hex::decode(value).unwrap().try_into().unwrap()
}
