//! Temporary actor support. See the package README for sources and replacement rules.

pub mod authority;
pub mod clock;
pub mod token;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidMessage,
    OutsideTimeBounds,
    InvalidState,
    Unauthorized,
    AlreadyInitialized,
    WrongAccount,
    WrongDefinition,
    InsufficientBalance,
    Overflow,
    InvalidNft,
    Renounced,
}
