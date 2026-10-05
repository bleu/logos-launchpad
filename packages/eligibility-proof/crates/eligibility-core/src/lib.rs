use ark_bn254::Fr;
use ark_ff::{BigInteger as _, PrimeField as _};
use k256::schnorr::SigningKey;
use lee_core::{NullifierPublicKey, account::AccountId};
use light_poseidon::{Poseidon, PoseidonHasher as _};
use risc0_zkvm::sha::{Impl, Sha256 as _};
use serde::{Deserialize, Serialize};

pub type Digest = [u8; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TreeHash {
    Sha256,
    Poseidon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaleContext {
    pub deployment: Digest,
    pub factory: Digest,
    pub namespace: Digest,
    pub sale_account: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalePolicy {
    pub sale: SaleContext,
    pub root: Digest,
    pub hash: TreeHash,
    pub depth: u8,
    pub collateral_limit: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Purchase {
    pub trading_account: Digest,
    pub recipient: Digest,
    pub collateral_in: u128,
    pub tokens_out: u128,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum AccountControl {
    Public {
        key: Digest,
    },
    Private {
        nullifier_key: Digest,
        identifier: u128,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Membership {
    pub blinder: Digest,
    pub index: u32,
    pub siblings: Vec<Digest>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct EligibilityWitness {
    pub control: AccountControl,
    pub membership: Membership,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct EligibilityInput {
    pub policy: SalePolicy,
    pub purchase: Purchase,
    pub witness: EligibilityWitness,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EligibilityPermit {
    pub sale: Digest,
    pub root: Digest,
    pub hash: TreeHash,
    pub collateral_limit: u128,
    pub purchase: Digest,
    pub nullifier: Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EligibilityError {
    InvalidMembership,
    InvalidAccountControl,
    InvalidPurchase,
}

#[derive(Clone, Copy)]
pub enum Route {
    Public,
    Private,
}

#[derive(Clone, Copy)]
pub enum TradeOutcome {
    Succeeded,
    Rejected,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PurchaseError {
    Eligibility(EligibilityError),
    AlreadyUsed,
    TradeRejected,
}

#[derive(Default)]
pub struct AllowanceBook {
    spent: std::collections::BTreeSet<Digest>,
}

impl AllowanceBook {
    pub fn attempt_purchase(
        &mut self,
        policy: &SalePolicy,
        purchase: &Purchase,
        witness: &EligibilityWitness,
        _route: Route,
        outcome: TradeOutcome,
    ) -> Result<EligibilityPermit, PurchaseError> {
        let permit =
            check_eligibility(policy, purchase, witness).map_err(PurchaseError::Eligibility)?;
        if self.spent.contains(&permit.nullifier) {
            return Err(PurchaseError::AlreadyUsed);
        }
        if matches!(outcome, TradeOutcome::Rejected) {
            return Err(PurchaseError::TradeRejected);
        }
        self.spent.insert(permit.nullifier);
        Ok(permit)
    }
}

pub struct ApprovedAccount {
    pub account: Digest,
    pub blinder: Digest,
}

pub struct MembershipTree {
    levels: Vec<Vec<Digest>>,
    blinders: Vec<Digest>,
}

impl MembershipTree {
    pub fn prepare(
        sale: &SaleContext,
        kind: TreeHash,
        depth: u8,
        entries: Vec<ApprovedAccount>,
    ) -> Result<Self, EligibilityError> {
        if depth > 17 || entries.is_empty() || entries.len() > 1_usize << depth {
            return Err(EligibilityError::InvalidMembership);
        }
        let sale = sale_digest(sale);
        let blinders = entries.iter().map(|entry| entry.blinder).collect();
        let mut leaves: Vec<_> = entries
            .iter()
            .map(|entry| {
                tree_hash(
                    kind,
                    b"logos/lbp/eligibility/v1/leaf",
                    1,
                    &[&sale, &entry.account, &entry.blinder],
                )
            })
            .collect();
        for index in entries.len()..1_usize << depth {
            leaves.push(tree_hash(
                kind,
                b"logos/lbp/eligibility/v1/padding",
                3,
                &[&sale, &(index as u32).to_le_bytes()],
            ));
        }
        let mut levels = vec![leaves];
        for level in 0..depth {
            let parents = levels[usize::from(level)]
                .chunks_exact(2)
                .map(|pair| {
                    tree_hash(
                        kind,
                        b"logos/lbp/eligibility/v1/node",
                        2,
                        &[&pair[0], &pair[1]],
                    )
                })
                .collect();
            levels.push(parents);
        }
        Ok(Self { levels, blinders })
    }

    pub fn root(&self) -> Digest {
        self.levels.last().unwrap()[0]
    }

    pub fn membership(&self, index: u32) -> Result<Membership, EligibilityError> {
        let blinder = *self
            .blinders
            .get(index as usize)
            .ok_or(EligibilityError::InvalidMembership)?;
        let mut position = index as usize;
        let siblings = self.levels[..self.levels.len() - 1]
            .iter()
            .map(|level| {
                let sibling = level[position ^ 1];
                position >>= 1;
                sibling
            })
            .collect();
        Ok(Membership {
            blinder,
            index,
            siblings,
        })
    }
}

pub fn check_public_binding(
    policy: &SalePolicy,
    purchase: &Purchase,
    permit: &EligibilityPermit,
) -> Result<(), EligibilityError> {
    if permit.sale != sale_digest(&policy.sale)
        || permit.root != policy.root
        || permit.hash != policy.hash
        || permit.collateral_limit != policy.collateral_limit
        || permit.purchase != purchase_digest(policy, purchase)
    {
        return Err(EligibilityError::InvalidPurchase);
    }
    Ok(())
}

pub fn check_eligibility(
    policy: &SalePolicy,
    purchase: &Purchase,
    witness: &EligibilityWitness,
) -> Result<EligibilityPermit, EligibilityError> {
    if purchase.collateral_in == 0
        || purchase.tokens_out == 0
        || purchase.collateral_in > policy.collateral_limit
    {
        return Err(EligibilityError::InvalidPurchase);
    }
    if policy.depth > 17
        || witness.membership.siblings.len() != usize::from(policy.depth)
        || u64::from(witness.membership.index) >= 1_u64 << policy.depth
    {
        return Err(EligibilityError::InvalidMembership);
    }
    let (account, key) = match &witness.control {
        AccountControl::Public { key } => {
            let signing_key =
                SigningKey::from_bytes(key).map_err(|_| EligibilityError::InvalidAccountControl)?;
            // LEZ a58fbce: lee/state_machine/src/signature/public_key.rs.
            let account = hash(
                b"/LEE/v0.3/AccountId/Public/\0\0\0\0\0",
                &[&signing_key.verifying_key().to_bytes()],
            );
            (account, signing_key.to_bytes().into())
        }
        AccountControl::Private {
            nullifier_key,
            identifier,
        } => {
            let npk = NullifierPublicKey::from(nullifier_key);
            let account = AccountId::for_regular_private_account(&npk, *identifier);
            (*account.value(), *nullifier_key)
        }
    };
    let sale = sale_digest(&policy.sale);
    let mut root = tree_hash(
        policy.hash,
        b"logos/lbp/eligibility/v1/leaf",
        1,
        &[&sale, &account, &witness.membership.blinder],
    );
    let mut index = witness.membership.index;
    for sibling in &witness.membership.siblings {
        let (left, right) = if index & 1 == 0 {
            (&root, sibling)
        } else {
            (sibling, &root)
        };
        root = tree_hash(
            policy.hash,
            b"logos/lbp/eligibility/v1/node",
            2,
            &[left, right],
        );
        index >>= 1;
    }
    if root != policy.root {
        return Err(EligibilityError::InvalidMembership);
    }
    Ok(EligibilityPermit {
        sale,
        root,
        hash: policy.hash,
        collateral_limit: policy.collateral_limit,
        purchase: purchase_digest(policy, purchase),
        nullifier: hash(
            b"logos/lbp/eligibility/v1/nullifier",
            &[&sale, &account, &key],
        ),
    })
}

fn purchase_digest(policy: &SalePolicy, purchase: &Purchase) -> Digest {
    hash(
        b"logos/lbp/eligibility/v1/purchase",
        &[
            &sale_digest(&policy.sale),
            &policy.root,
            &[policy.hash as u8, policy.depth],
            &policy.collateral_limit.to_le_bytes(),
            &purchase.trading_account,
            &purchase.recipient,
            &purchase.collateral_in.to_le_bytes(),
            &purchase.tokens_out.to_le_bytes(),
        ],
    )
}

fn sale_digest(sale: &SaleContext) -> Digest {
    hash(
        b"logos/lbp/eligibility/v1/sale",
        &[
            &sale.deployment,
            &sale.factory,
            &sale.namespace,
            &sale.sale_account,
        ],
    )
}

fn tree_hash(kind: TreeHash, domain: &[u8], tag: u64, parts: &[&[u8]]) -> Digest {
    if kind == TreeHash::Sha256 {
        return hash(domain, parts);
    }
    let mut inputs = vec![Fr::from(tag)];
    for part in parts {
        for chunk in part.chunks(16) {
            let mut limb = [0; 16];
            limb[..chunk.len()].copy_from_slice(chunk);
            inputs.push(Fr::from(u128::from_le_bytes(limb)));
        }
    }
    Poseidon::<Fr>::new_circom(inputs.len())
        .unwrap()
        .hash(&inputs)
        .unwrap()
        .into_bigint()
        .to_bytes_be()
        .try_into()
        .unwrap()
}

fn hash(domain: &[u8], parts: &[&[u8]]) -> Digest {
    let mut bytes = domain.to_vec();
    for part in parts {
        bytes.extend_from_slice(part);
    }
    Impl::hash_bytes(&bytes).as_bytes().try_into().unwrap()
}
