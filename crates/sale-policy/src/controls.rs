use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum PausePolicy {
    Disabled,
    AuthorityControlled,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum TradeControl {
    Unrestricted,
    Running,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseError {
    Disabled,
}

impl TradeControl {
    pub fn new(policy: PausePolicy) -> Self {
        match policy {
            PausePolicy::Disabled => Self::Unrestricted,
            PausePolicy::AuthorityControlled => Self::Running,
        }
    }

    pub fn policy(self) -> PausePolicy {
        match self {
            Self::Unrestricted => PausePolicy::Disabled,
            Self::Running | Self::Paused => PausePolicy::AuthorityControlled,
        }
    }

    pub fn is_paused(self) -> bool {
        self == Self::Paused
    }

    /// Callers must authenticate authority and enforce closure and inclusion bounds first.
    pub fn set_paused(&mut self, paused: bool) -> Result<bool, PauseError> {
        if *self == Self::Unrestricted {
            return Err(PauseError::Disabled);
        }
        let next = if paused { Self::Paused } else { Self::Running };
        let changed = *self != next;
        *self = next;
        Ok(changed)
    }
}
