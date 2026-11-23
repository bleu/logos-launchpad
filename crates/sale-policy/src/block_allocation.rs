//! Aggregate gross token sales per inclusion block. Sells do not restore allowance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidTerms,
    InvalidState,
    BlockAllocationExceeded,
}
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockAllocation {
    pub limit: Option<u128>,
    pub observed_block: Option<u64>,
    pub tokens_sold: u128,
}

impl BlockAllocation {
    pub fn new(limit: Option<u128>) -> Result<Self, Error> {
        let value = Self {
            limit,
            ..Self::default()
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), Error> {
        match self.limit {
            Some(limit) if limit == 0 || self.tokens_sold > limit => Err(Error::InvalidTerms),
            None if self.observed_block.is_some() || self.tokens_sold != 0 => {
                Err(Error::InvalidState)
            }
            _ if self.observed_block.is_none() && self.tokens_sold != 0 => Err(Error::InvalidState),
            _ => Ok(()),
        }
    }

    /// Tokens still available to buys that observe `block`. `None` means no ceiling.
    pub fn remaining(&self, block: u64) -> Option<u128> {
        let limit = self.limit?;
        if self.observed_block == Some(block) {
            Some(limit.saturating_sub(self.tokens_sold))
        } else {
            Some(limit)
        }
    }

    pub fn record(&mut self, block: u64, tokens: u128) -> Result<(), Error> {
        self.validate()?;
        let limit = self.limit.ok_or(Error::InvalidState)?;
        if self.observed_block.is_some_and(|previous| block < previous) {
            return Err(Error::InvalidState);
        }
        let sold = if self.observed_block == Some(block) {
            self.tokens_sold
        } else {
            0
        };
        let next = sold
            .checked_add(tokens)
            .ok_or(Error::BlockAllocationExceeded)?;
        if next > limit {
            return Err(Error::BlockAllocationExceeded);
        }
        self.observed_block = Some(block);
        self.tokens_sold = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aggregates_buys_and_resets_only_on_a_later_block() {
        let mut allocation = BlockAllocation::new(Some(100)).unwrap();
        allocation.record(8, 40).unwrap();
        allocation.record(8, 60).unwrap();
        let full = allocation;
        assert_eq!(allocation.record(8, 1), Err(Error::BlockAllocationExceeded));
        assert_eq!(allocation, full);
        assert_eq!(allocation.record(7, 1), Err(Error::InvalidState));
        assert_eq!(allocation.remaining(8), Some(0));
        assert_eq!(allocation.remaining(9), Some(100));
        allocation.record(9, 100).unwrap();
        assert_eq!(allocation.tokens_sold, 100);
        assert_eq!(BlockAllocation::new(None).unwrap().remaining(9), None);
    }
    #[test]
    fn validates_disabled_zero_and_full_width_limits() {
        assert!(BlockAllocation::new(None).is_ok());
        assert_eq!(BlockAllocation::new(Some(0)), Err(Error::InvalidTerms));
        let mut allocation = BlockAllocation::new(Some(u128::MAX)).unwrap();
        allocation.record(0, u128::MAX).unwrap();
        let before = allocation;
        assert_eq!(allocation.record(0, 1), Err(Error::BlockAllocationExceeded));
        assert_eq!(allocation, before);
    }
}
