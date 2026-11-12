use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum EndingPolicy {
    /// No automatic ending; direct pool owner permissions stay with the engine.
    Manual,
    Scheduled,
    /// Selected recorded reserve depletion without a time limit.
    Depletion,
    Either,
}

/// Unix seconds, with an inclusive start and an exclusive end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeBounds {
    pub at_least: Option<u64>,
    pub before: Option<u64>,
}

impl TimeBounds {
    pub fn contains(self, seconds: u64) -> bool {
        self.at_least.is_none_or(|start| seconds >= start)
            && self.before.is_none_or(|end| seconds < end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndingReason {
    Manual,
    Depleted,
    ScheduledEnd,
}

/// A decision must be enforced with its inclusion bounds by the custody adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndingDecision {
    Manual,
    Depleted,
    ScheduledEnd { timestamp: u64 },
}

impl EndingDecision {
    pub fn reason(self) -> EndingReason {
        match self {
            Self::Manual => EndingReason::Manual,
            Self::Depleted => EndingReason::Depleted,
            Self::ScheduledEnd { .. } => EndingReason::ScheduledEnd,
        }
    }

    pub fn inclusion_bounds(self) -> TimeBounds {
        TimeBounds {
            at_least: match self {
                Self::Manual | Self::Depleted => None,
                Self::ScheduledEnd { timestamp } => Some(timestamp),
            },
            before: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndingError {
    InvalidWindow,
    Ended,
    NotEnded,
}

impl std::fmt::Display for EndingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidWindow => "the ending policy and time window do not match",
            Self::Ended => "the selected reserve is depleted",
            Self::NotEnded => "no automatic ending permits settlement",
        })
    }
}

impl std::error::Error for EndingError {}

impl EndingPolicy {
    /// Adapters also check timestamp conversion before funding.
    pub fn validate(self, start: Option<u64>, end: Option<u64>) -> Result<(), EndingError> {
        match (self, end) {
            (Self::Manual | Self::Depletion, Some(_)) | (Self::Scheduled | Self::Either, None) => {
                return Err(EndingError::InvalidWindow);
            }
            _ => {}
        }
        if let (Some(start), Some(end)) = (start, end)
            && start >= end
        {
            return Err(EndingError::InvalidWindow);
        }
        Ok(())
    }

    /// The adapter selects remaining inventory or reserves from its saved custody state.
    pub fn trade_bounds(
        self,
        start: Option<u64>,
        end: Option<u64>,
        remaining: u128,
    ) -> Result<TimeBounds, EndingError> {
        self.validate(start, end)?;
        if matches!(self, Self::Depletion | Self::Either) && remaining == 0 {
            return Err(EndingError::Ended);
        }
        Ok(TimeBounds {
            at_least: start,
            before: end,
        })
    }

    /// Scheduled settlement remains conditional on the returned inclusion lower bound.
    pub fn settlement(
        self,
        end: Option<u64>,
        remaining: u128,
    ) -> Result<EndingDecision, EndingError> {
        self.validate(None, end)?;
        if matches!(self, Self::Depletion | Self::Either) && remaining == 0 {
            return Ok(EndingDecision::Depleted);
        }
        end.map(|timestamp| EndingDecision::ScheduledEnd { timestamp })
            .ok_or(EndingError::NotEnded)
    }
}
