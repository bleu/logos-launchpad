//! Route selection and completion rules. Callers retain authority and custody checks.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{BASIS_POINTS, Rounding, mul_div};

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
)]
pub struct GraduationPolicy {
    pub launch_bps: u16,
    pub collateral_bps: u16,
}

impl GraduationPolicy {
    pub fn validate(self, has_adapter: bool) -> Result<(), SettlementError> {
        if u128::from(self.launch_bps) > BASIS_POINTS
            || u128::from(self.collateral_bps) > BASIS_POINTS
        {
            return Err(SettlementError::InvalidGraduationPolicy);
        }
        if !has_adapter && (self.launch_bps != 0 || self.collateral_bps != 0) {
            return Err(SettlementError::AdapterRequired);
        }
        Ok(())
    }

    pub fn validate_route(
        self,
        route: SettlementRoute,
        has_adapter: bool,
    ) -> Result<(), SettlementError> {
        self.validate(has_adapter)?;
        if has_adapter != (route == SettlementRoute::Graduation) {
            return Err(SettlementError::RouteMismatch);
        }
        Ok(())
    }

    pub fn payouts(
        self,
        amounts: SettlementAmounts,
        route: SettlementRoute,
        has_adapter: bool,
    ) -> Result<SettlementPayouts, SettlementError> {
        self.validate_route(route, has_adapter)?;
        let share = |amount, bps| {
            mul_div(amount, u128::from(bps), BASIS_POINTS, Rounding::Floor)
                .map_err(|_| SettlementError::Arithmetic)
        };
        let adapter = SettlementAmounts {
            launch_tokens: share(amounts.launch_tokens, self.launch_bps)?,
            collateral: share(amounts.collateral, self.collateral_bps)?,
        };
        let creator = SettlementAmounts {
            launch_tokens: amounts
                .launch_tokens
                .checked_sub(adapter.launch_tokens)
                .ok_or(SettlementError::Arithmetic)?,
            collateral: amounts
                .collateral
                .checked_sub(adapter.collateral)
                .ok_or(SettlementError::Arithmetic)?,
        };
        Ok(SettlementPayouts { creator, adapter })
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
)]
pub struct SettlementPayouts {
    pub creator: SettlementAmounts,
    pub adapter: SettlementAmounts,
}

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
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    BorshSerialize,
    BorshDeserialize,
    Serialize,
    Deserialize,
)]
pub struct SettlementAmounts {
    pub launch_tokens: u128,
    pub collateral: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlementError {
    AlreadySettled,
    ManualPayoutStarted,
    InvalidGraduationPolicy,
    AdapterRequired,
    RouteMismatch,
    Arithmetic,
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

    /// Record route-specific assets only; fees and creator allocation are separate.
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
