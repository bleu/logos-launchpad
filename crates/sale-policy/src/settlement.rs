//! Route selection and completion rules. Callers retain authority and custody checks.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum SettlementRoute {
    Manual,
    Graduation,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum SettlementProgress {
    Unselected,
    ManualStarted,
    ManualComplete,
    Graduated,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub struct SettlementAmounts {
    pub launch_tokens: u128,
    pub collateral: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlementError {
    AlreadySettled,
    ManualPayoutStarted,
}

impl SettlementProgress {
    pub fn route(self) -> Option<SettlementRoute> {
        match self {
            Self::Unselected => None,
            Self::ManualStarted | Self::ManualComplete => Some(SettlementRoute::Manual),
            Self::Graduated => Some(SettlementRoute::Graduation),
        }
    }

    pub fn is_complete(self) -> bool {
        matches!(self, Self::ManualComplete | Self::Graduated)
    }

    pub fn ensure_available(self, route: SettlementRoute) -> Result<(), SettlementError> {
        match (self, route) {
            (Self::Unselected | Self::ManualStarted, SettlementRoute::Manual)
            | (Self::Unselected, SettlementRoute::Graduation) => Ok(()),
            (Self::ManualStarted | Self::ManualComplete, SettlementRoute::Graduation) => {
                Err(SettlementError::ManualPayoutStarted)
            }
            _ => Err(SettlementError::AlreadySettled),
        }
    }

    /// Record route-specific assets only; fees, burns, allocation and seed return are separate.
    pub fn record_manual_payout(
        &mut self,
        amounts: SettlementAmounts,
    ) -> Result<(), SettlementError> {
        self.ensure_available(SettlementRoute::Manual)?;
        if amounts.launch_tokens != 0 || amounts.collateral != 0 {
            *self = Self::ManualStarted;
        }
        Ok(())
    }

    pub fn finish_manual(&mut self) -> Result<(), SettlementError> {
        self.ensure_available(SettlementRoute::Manual)?;
        *self = Self::ManualComplete;
        Ok(())
    }

    /// Call only after the adapter callback succeeds, including for empty adapter amounts.
    pub fn finish_graduation(&mut self) -> Result<(), SettlementError> {
        self.ensure_available(SettlementRoute::Graduation)?;
        *self = Self::Graduated;
        Ok(())
    }
}
