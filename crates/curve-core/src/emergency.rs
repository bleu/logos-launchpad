// Adapted from SPEL authority and freeze helpers; see third-party/spel-emergency/PROVENANCE.
// Copyright (c) 2026 mmlado. Licensed under MIT.

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, AccountWithMetadata, Data},
    program::{AccountPostState, ProgramId},
};

use crate::validated_config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorityError {
    InvalidCandidate,
    NotHolder,
    Renounced,
    MissingSignature,
}

impl core::fmt::Display for AuthorityError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidCandidate => write!(f, "invalid authority candidate"),
            Self::NotHolder => write!(f, "signer is not the current authority"),
            Self::Renounced => write!(f, "authority renounced"),
            Self::MissingSignature => write!(f, "authority signature missing"),
        }
    }
}

impl std::error::Error for AuthorityError {}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, PartialEq, Eq)]
struct AuthoritySlot {
    holder: AccountId,
}

impl AuthoritySlot {
    fn initialize(holder: AccountId) -> Result<Self, AuthorityError> {
        if holder == AccountId::default() {
            return Err(AuthorityError::InvalidCandidate);
        }
        Ok(Self { holder })
    }

    fn assert(&self, signer: &AccountWithMetadata) -> Result<(), AuthorityError> {
        if self.holder == AccountId::default() {
            return Err(AuthorityError::Renounced);
        }
        if !signer.is_authorized {
            return Err(AuthorityError::MissingSignature);
        }
        if signer.account_id != self.holder {
            return Err(AuthorityError::NotHolder);
        }
        Ok(())
    }

    fn transfer(
        &mut self,
        current: &AccountWithMetadata,
        new_authority: AccountId,
    ) -> Result<(), AuthorityError> {
        self.assert(current)?;
        if new_authority == AccountId::default() {
            return Err(AuthorityError::InvalidCandidate);
        }
        self.holder = new_authority;
        Ok(())
    }

    fn holder(&self) -> AccountId {
        self.holder
    }
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct FreezeConfig {
    slot: AuthoritySlot,
    pub is_frozen: bool,
}

impl FreezeConfig {
    pub fn initialize(authority: AccountId) -> Result<Self, AuthorityError> {
        Ok(Self {
            slot: AuthoritySlot::initialize(authority)?,
            is_frozen: false,
        })
    }

    pub fn authority(&self) -> AccountId {
        self.slot.holder()
    }

    pub fn transfer(
        &mut self,
        current: &AccountWithMetadata,
        new_authority: AccountId,
    ) -> Result<(), AuthorityError> {
        self.slot.transfer(current, new_authority)
    }

    pub fn set_is_frozen(
        &mut self,
        current: &AccountWithMetadata,
        frozen: bool,
    ) -> Result<(), AuthorityError> {
        self.slot.assert(current)?;
        self.is_frozen = frozen;
        Ok(())
    }
}

pub(crate) fn transfer_emergency_authority(
    namespace: AccountId,
    config: AccountWithMetadata,
    authority: AccountWithMetadata,
    new_authority: AccountId,
    program: ProgramId,
) -> Vec<AccountPostState> {
    let mut state = validated_config(&config, namespace, program);
    state
        .emergency
        .transfer(&authority, new_authority)
        .unwrap_or_else(|error| panic!("Emergency control rejected: {error}"));
    let mut post = config.account;
    post.data = Data::from(&state);
    vec![
        AccountPostState::new(post),
        AccountPostState::new(authority.account),
    ]
}

pub(crate) fn set_frozen(
    namespace: AccountId,
    config: AccountWithMetadata,
    authority: AccountWithMetadata,
    frozen: bool,
    program: ProgramId,
) -> Vec<AccountPostState> {
    let mut state = validated_config(&config, namespace, program);
    state
        .emergency
        .set_is_frozen(&authority, frozen)
        .unwrap_or_else(|error| panic!("Emergency control rejected: {error}"));
    let mut post = config.account;
    post.data = Data::from(&state);
    vec![
        AccountPostState::new(post),
        AccountPostState::new(authority.account),
    ]
}
