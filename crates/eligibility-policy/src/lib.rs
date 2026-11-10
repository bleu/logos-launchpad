use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const DEPTH: usize = 17;
pub const CAPACITY: u32 = 1 << DEPTH;
pub const CONTEXT_BYTES: usize = 65;
pub const POLICY_BYTES: usize = CONTEXT_BYTES + 32;
pub const PROOF_BYTES: usize = 4 + 32 + DEPTH * 32;

#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize, Deserialize, Serialize)]
pub enum BuyPolicy {
    Open,
    Allowlist(AllowlistPolicy),
}

#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize, Deserialize, Serialize)]
pub struct AllowlistPolicy {
    pub version: u8,
    pub namespace: [u8; 32],
    pub sale_policy_id: [u8; 32],
    pub root: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize, Deserialize, Serialize)]
pub struct MembershipPath {
    pub index: u32,
    pub salt: [u8; 32],
    pub siblings: [[u8; 32]; DEPTH],
}

#[derive(Debug, PartialEq, Eq)]
pub enum VerifyError {
    InvalidEncoding,
    UnsupportedVersion,
    InvalidProof,
    MissingPath,
    InvalidBinding,
}

impl BuyPolicy {
    pub fn validate_binding(
        &self,
        namespace: &[u8; 32],
        sale_policy_id: &[u8; 32],
    ) -> Result<(), VerifyError> {
        if let Self::Allowlist(policy) = self {
            if policy.version != 1 {
                return Err(VerifyError::UnsupportedVersion);
            }
            if &policy.namespace != namespace || &policy.sale_policy_id != sale_policy_id {
                return Err(VerifyError::InvalidBinding);
            }
        }
        Ok(())
    }

    /// The stored policy and actual trader must come from trusted execution state.
    pub fn admit(
        &self,
        trader: &[u8; 32],
        path: Option<&MembershipPath>,
    ) -> Result<(), VerifyError> {
        match self {
            Self::Open => Ok(()),
            Self::Allowlist(policy) => {
                if policy.version != 1 {
                    return Err(VerifyError::UnsupportedVersion);
                }
                let path = path.ok_or(VerifyError::MissingPath)?;
                if path.index >= CAPACITY {
                    return Err(VerifyError::InvalidEncoding);
                }
                let mut context = [0; CONTEXT_BYTES];
                context[0] = policy.version;
                context[1..33].copy_from_slice(&policy.namespace);
                context[33..].copy_from_slice(&policy.sale_policy_id);
                let mut digest = hash(&[b"LP/allowlist/leaf/v1\0", &context, trader, &path.salt]);
                let mut index = path.index;
                for sibling in &path.siblings {
                    digest = if index & 1 == 0 {
                        hash(&[b"LP/allowlist/node/v1\0", &context, &digest, sibling])
                    } else {
                        hash(&[b"LP/allowlist/node/v1\0", &context, sibling, &digest])
                    };
                    index >>= 1;
                }
                if digest == policy.root {
                    Ok(())
                } else {
                    Err(VerifyError::InvalidProof)
                }
            }
        }
    }
}

/// Accepts the fixed allowlist-policy record without the `BuyPolicy` enum tag.
/// Policy and trader must come from trusted execution state.
pub fn verify_for(policy: &[u8], trader: &[u8], proof: &[u8]) -> Result<(), VerifyError> {
    if policy.len() != POLICY_BYTES || trader.len() != 32 || proof.len() != PROOF_BYTES {
        return Err(VerifyError::InvalidEncoding);
    }
    let policy =
        borsh::from_slice::<AllowlistPolicy>(policy).map_err(|_| VerifyError::InvalidEncoding)?;
    let trader = trader
        .try_into()
        .map_err(|_| VerifyError::InvalidEncoding)?;
    let path =
        borsh::from_slice::<MembershipPath>(proof).map_err(|_| VerifyError::InvalidEncoding)?;
    BuyPolicy::Allowlist(policy).admit(trader, Some(&path))
}

fn hash(parts: &[&[u8]]) -> [u8; 32] {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update(part);
    }
    digest.finalize().into()
}
