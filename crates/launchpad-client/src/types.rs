use std::str::FromStr;

use lee::AccountId;

use crate::{ClientError, Result};

/// An account ID and the ledger where its holding exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccountLocation {
    Public(AccountId),
    Private(AccountId),
}

impl AccountLocation {
    #[must_use]
    pub fn account_id(self) -> AccountId {
        match self {
            Self::Public(account_id) | Self::Private(account_id) => account_id,
        }
    }

    #[must_use]
    pub fn is_private(self) -> bool {
        matches!(self, Self::Private(_))
    }
}

impl FromStr for AccountLocation {
    type Err = ClientError;

    fn from_str(raw: &str) -> Result<Self> {
        let (is_private, account) = match raw.split_once('/') {
            Some(("Public", account)) => (false, account),
            Some(("Private", account)) => (true, account),
            Some(_) => {
                return Err(ClientError::UnknownAccountLocationPrefix {
                    input: raw.to_owned(),
                });
            }
            None => (false, raw),
        };
        let account_id: AccountId =
            account
                .parse()
                .map_err(|source| ClientError::InvalidAccountId {
                    input: raw.to_owned(),
                    source,
                })?;
        Ok(if is_private {
            Self::Private(account_id)
        } else {
            Self::Public(account_id)
        })
    }
}

/// An account ID known to represent a private holding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrivateAccountId(AccountId);

impl PrivateAccountId {
    #[must_use]
    pub fn account_id(self) -> AccountId {
        self.0
    }
}

impl From<PrivateAccountId> for AccountId {
    fn from(value: PrivateAccountId) -> Self {
        value.0
    }
}

impl FromStr for PrivateAccountId {
    type Err = ClientError;

    fn from_str(raw: &str) -> Result<Self> {
        let account = match raw.split_once('/') {
            Some(("Private", account)) => account,
            Some(("Public", _)) => {
                return Err(ClientError::PrivateAccountIdRequiresPrivateLocation {
                    input: raw.to_owned(),
                });
            }
            Some(_) => {
                return Err(ClientError::UnknownAccountLocationPrefix {
                    input: raw.to_owned(),
                });
            }
            None => raw,
        };
        account
            .parse()
            .map(Self)
            .map_err(|source| ClientError::InvalidAccountId {
                input: raw.to_owned(),
                source,
            })
    }
}

/// Stable identifiers for one factory-created curve sale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FactorySale {
    pub namespace: AccountId,
    pub factory_program_id: AccountId,
    pub curve_program_id: AccountId,
    pub launch_salt: [u8; 32],
    pub collateral_definition: AccountId,
}

/// Stable identifiers for one factory-created fixed-price or weighted sale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LbpSale {
    pub namespace: AccountId,
    pub namespace_program_id: AccountId,
    pub factory_program_id: AccountId,
    pub lbp_program_id: AccountId,
    pub launch_salt: [u8; 32],
    pub collateral_definition: AccountId,
}

/// Exact-output trade limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExactOutputTradeTerms {
    pub amount_out: u128,
    pub max_amount_in: u128,
}

/// Exact-input trade limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExactInputTradeTerms {
    pub amount_in: u128,
    pub min_amount_out: u128,
}

/// A pool and collateral definition that receive a fee collection call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolFeeTarget {
    pub pool: AccountId,
    pub collateral_definition: AccountId,
}
