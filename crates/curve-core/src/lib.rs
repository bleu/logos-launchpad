//! The curve program: instruction enum, Borsh state, PDA derivation, and the handlers.
//!
//! Modelled on `lez/programs/amm/core/src/lib.rs` plus `lez/programs/amm/src/`, with one
//! deliberate difference. Upstream puts the handlers in the guest binary crate; here they
//! live in the host workspace and `methods/guest/src/bin/curve.rs` is a dispatch shim, so
//! the AMM-style tests run under `cargo test --workspace`.
//!
//! The handlers stay shallow. They deserialize accounts, call `pool`, and translate the
//! returned outcome into account post-states plus typed token-settlement values. The guest/token
//! adapter turns those settlements into chained calls.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, Data},
    program::{PdaSeed, ProgramId},
};
use pool::Pool;
use serde::{Deserialize, Serialize};

mod authority;
mod dispatch;
mod pool_create;
mod pool_lifecycle;
mod pool_swap;
mod update_config;

pub use authority::{TOKEN_PROGRAM_ID, identity};
pub use dispatch::{process_instruction, use_public_authorizations};
pub use pool_create::after_ata_creation;
pub use pool_swap::validated_config;

#[cfg(test)]
mod tests;

/// Curve program instruction. Token accounts are supplied by the guest/token adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Instruction {
    /// Retired wire tag. Always rejects; use explicit namespace operations.
    ///
    /// Required accounts:
    /// - Config PDA
    /// - Authority (authorized): the namespace identity on the first call, the stored
    ///   admin after
    UpdateConfig {
        namespace: AccountId,
        admin: AccountId,
        protocol_fee_bps: u32,
        treasury: AccountId,
    },
    /// Permanently disables namespace administration; swaps retain the last settings.
    RenounceAdmin { namespace: AccountId },
    /// Creates a bounded pool over an ordered token pair.
    ///
    /// Required accounts:
    /// - Pool PDA (uninitialized)
    /// - Owner authority (authorized)
    /// - Token 0 definition
    /// - Token 1 definition
    /// - Owner's token 0 ATA
    /// - Owner's token 1 ATA
    /// - Pool PDA's token 0 ATA (uninitialized)
    /// - Pool PDA's token 1 ATA (uninitialized)
    CreatePool {
        allowed_input_token: Option<u8>,
        defer_funding: bool,
        namespace: AccountId,
        token0_amount: u128,
        token1_amount: u128,
        virtual_reserve0: u128,
        virtual_reserve1: u128,
        close_timestamp: Option<u64>,
        close_on_depletion: Option<u8>,
        owner: AccountId,
        owner_program_id: Option<ProgramId>,
        owner_program_seed: Option<[u8; 32]>,
        /// Included in the wire instruction following the LEZ program convention;
        /// dispatch verifies it against the executing program id.
        curve_program_id: ProgramId,
    },
    /// Swaps an exact input amount; `token_in` selects the direction.
    ///
    /// Required accounts, in order:
    /// - Pool PDA
    /// - Config PDA
    /// - Participant (authorized)
    /// - Participant input ATA
    /// - Pool input ATA
    /// - Pool output ATA
    /// - Participant output ATA
    /// - Trusted LEZ clock
    SwapExactInput {
        amount_in: u128,
        min_amount_out: u128,
        token_in: AccountId,
    },
    /// Receives an exact output amount while capping fee-inclusive input.
    SwapExactOutput {
        amount_out: u128,
        max_amount_in: u128,
        token_in: AccountId,
    },
    /// Owner-only logical closure.
    ClosePool,
    /// Funds a prepared pool; same ten accounts as CreatePool.
    ActivatePool,
    /// Owner-only full withdrawal after manual closure or expiry, collecting fees atomically.
    /// Accounts: pool, owner, owner ATA0/ATA1, pool ATA0/ATA1, clock, config, treasury ATA1.
    /// Omit the last account when it aliases owner ATA1 (LEZ requires unique IDs).
    WithdrawReserves,
    /// Permissionless collection: pool, namespace config, pool token1 ATA, treasury token1 ATA.
    CollectFees,
    /// Select the pinned privacy circuit's transaction-wide PDA authorization rules.
    Private { instruction: Box<Instruction> },
    /// Creates namespace settings and both authority roles atomically.
    InitializeNamespace {
        namespace: AccountId,
        admin: AccountId,
        protocol_fee_bps: u32,
        treasury: AccountId,
        emergency_authority: AccountId,
        emergency_candidate: EmergencyCandidate,
    },
    UpdateFee {
        namespace: AccountId,
        protocol_fee_bps: u32,
    },
    UpdateTreasury {
        namespace: AccountId,
        treasury: AccountId,
    },
    TransferAdmin {
        namespace: AccountId,
        new_admin: AccountId,
        candidate: AdminCandidate,
    },
    /// Emergency-authority-only stop for namespace swaps.
    FreezeNamespace { namespace: AccountId },
    /// Removes the namespace swap stop without changing sale closing rules.
    UnfreezeNamespace { namespace: AccountId },
}

/// Acceptance evidence for a namespace administrator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdminCandidate {
    Signer,
    Pda {
        program_id: ProgramId,
        seed: [u8; 32],
    },
}

/// Acceptance evidence for the fixed namespace emergency authority.
pub type EmergencyCandidate = AdminCandidate;

/// The protocol fee denominator. A fee at this rate consumes its buy-side input,
/// so the swap state machine rejects it as untradeable.
pub const MAX_FEE_BPS: u16 = 10_000;

/// The ATA program shipped by the pinned LEZ revision. Program IDs are risc0
/// image IDs, so this binds custody calls to that exact guest implementation.
pub const ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID: ProgramId = [
    0xc81c_8495,
    0xd787_2cbd,
    0xc7c5_1b11,
    0x852a_aaf3,
    0xf790_5ed3,
    0xa382_7e21,
    0xac05_aa97,
    0xf800_15d5,
];

/// Live namespace settings with embedded administration and emergency components.
#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct Config {
    format: [u8; 8],
    version: u16,
    administration: admin_authority::AdminConfig,
    pub protocol_fee_bps: u16,
    pub treasury: AccountId,
    emergency: freeze_authority::FreezeConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            format: *b"LZNSCONF",
            version: 2,
            administration: Default::default(),
            protocol_fee_bps: 0,
            treasury: AccountId::default(),
            emergency: Default::default(),
        }
    }
}

impl Config {
    pub(crate) const ADMIN_OFFSET: usize = 10;
    pub(crate) const FREEZE_OFFSET: usize = Self::ADMIN_OFFSET
        + <admin_authority::AdminConfig as spel_framework::FixedBorshSize>::SIZE
        + 2
        + 32;

    /// Constructs a config value; namespace creation must also validate account evidence.
    pub fn new(
        admin: AccountId,
        protocol_fee_bps: u16,
        treasury: AccountId,
        emergency_authority: AccountId,
    ) -> Self {
        use spel_framework::prelude as spel;

        let emergency_account = spel::AccountWithMetadata {
            account_id: spel::AccountId::new(emergency_authority.to_bytes()),
            is_authorized: true,
            account: spel::Account::default(),
        };
        let mut embedded = emergency_account.clone();
        freeze_authority::FreezeConfig::bootstrap(
            &mut embedded,
            freeze_authority::FreezeCandidate::Signer,
            &emergency_account,
        )
        .expect("Non-default emergency authority");
        Self {
            administration: admin_authority::AdminConfig::initialize(
                spel_framework::prelude::AccountId::new(admin.to_bytes()),
            )
            .expect("Non-default initial administrator"),
            emergency: freeze_authority::FreezeConfig::from_account(&embedded)
                .expect("Initialized emergency state"),
            protocol_fee_bps,
            treasury,
            ..Self::default()
        }
    }

    /// Returns no administrator after terminal renunciation.
    pub fn current_admin(&self) -> Option<AccountId> {
        self.administration
            .current_admin()
            .map(|id| AccountId::new(id.to_bytes()))
    }

    /// Absent only in an uninitialized config, which the account reader rejects.
    pub fn emergency_authority(&self) -> Option<AccountId> {
        self.emergency
            .current_authority()
            .map(|id| AccountId::new(id.to_bytes()))
    }

    pub fn is_frozen(&self) -> bool {
        self.emergency.is_frozen
    }

    pub(crate) fn assert_admin(&self, account: &lee_core::account::AccountWithMetadata) {
        self.administration
            .assert_admin(&update_config::adapt(account))
            .expect("Namespace administrator authorization failed");
    }
}

impl TryFrom<&Data> for Config {
    type Error = std::io::Error;

    fn try_from(data: &Data) -> Result<Self, Self::Error> {
        let config = Self::try_from_slice(data.as_ref())?;
        if config.format != *b"LZNSCONF"
            || config.version != 2
            || config.emergency_authority().is_none()
            || config.protocol_fee_bps >= MAX_FEE_BPS
            || config.treasury == AccountId::default()
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unsupported or invalid namespace config",
            ));
        }
        Ok(config)
    }
}

impl From<&Config> for Data {
    fn from(config: &Config) -> Self {
        let mut data = Vec::with_capacity(std::mem::size_of_val(config));

        BorshSerialize::serialize(config, &mut data).expect("Serialization to Vec should not fail");

        Self::try_from(data).expect("Config encoded data should fit into Data")
    }
}

#[must_use]
pub fn compute_config_pda(namespace: AccountId, curve_program_id: ProgramId) -> AccountId {
    AccountId::for_public_pda(&curve_program_id, &compute_config_pda_seed(namespace))
}

#[must_use]
pub fn compute_config_pda_seed(namespace: AccountId) -> PdaSeed {
    PdaSeed::new(namespace.to_bytes())
}

/// The pool PDA's contents: ordered token roles, owner, and bounded-AMM state.
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PoolAccount {
    pub allowed_input_token: Option<pool::TokenSide>,
    pub funded: bool,
    pub namespace: AccountId,
    pub token0_definition_id: AccountId,
    pub token1_definition_id: AccountId,
    pub owner: AccountId,
    pub owner_program: Option<(ProgramId, [u8; 32])>,
    pub pool: Pool,
}

impl TryFrom<&Data> for PoolAccount {
    type Error = std::io::Error;

    fn try_from(data: &Data) -> Result<Self, Self::Error> {
        Self::try_from_slice(data.as_ref())
    }
}

impl From<&PoolAccount> for Data {
    fn from(pool_account: &PoolAccount) -> Self {
        let mut bytes = Vec::with_capacity(std::mem::size_of_val(pool_account));
        BorshSerialize::serialize(pool_account, &mut bytes)
            .expect("serialisation to a Vec cannot fail");
        Self::try_from(bytes).expect("a pool account is far below the account data size limit")
    }
}

/// One pool per namespace, ordered pair and owner. The pair hashes in fixed order.
#[must_use]
pub fn compute_pool_pda(
    namespace: AccountId,
    curve_program_id: ProgramId,
    token0_definition_id: AccountId,
    token1_definition_id: AccountId,
    owner: AccountId,
) -> AccountId {
    AccountId::for_public_pda(
        &curve_program_id,
        &compute_pool_pda_seed(namespace, token0_definition_id, token1_definition_id, owner),
    )
}

#[must_use]
pub fn compute_pool_pda_seed(
    namespace: AccountId,
    token0_definition_id: AccountId,
    token1_definition_id: AccountId,
    owner: AccountId,
) -> PdaSeed {
    use risc0_zkvm::sha::{Impl, Sha256 as _};

    let mut bytes = [0; 128];
    bytes[0..32].copy_from_slice(&token0_definition_id.to_bytes());
    bytes[32..64].copy_from_slice(&token1_definition_id.to_bytes());
    bytes[64..96].copy_from_slice(&owner.to_bytes());
    bytes[96..].copy_from_slice(&namespace.to_bytes());

    PdaSeed::new(
        Impl::hash_bytes(&bytes)
            .as_bytes()
            .try_into()
            .expect("Hash output must be exactly 32 bytes long"),
    )
}
