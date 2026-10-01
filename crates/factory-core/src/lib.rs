//! Launch policy layered over the neutral curve pool.
//!
//! The factory creates one fixed-supply launch token, records the public split, and
//! prepares and funds the curve pool across four resumable, individually atomic stages.
//! The factory owns the token-definition PDA and exposes no mint or metadata-update
//! instruction: this is the authority-revocation boundary for the pinned token API.
//! The existing guest error API keeps hand-written implementations to avoid changing
//! its dependency boundary; SDK errors use host-side derives.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use borsh::{BorshDeserialize, BorshSerialize};
use curve_core::{Instruction as CurveInstruction, PoolAccount, compute_pool_pda};
use lee_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{AccountPostState, ChainedCall, Claim, PdaSeed, ProgramId},
};
use pool::PoolLifecycle;
use serde::{Deserialize, Serialize};
use token_core::{MetadataStandard, NewTokenDefinition, NewTokenMetadata, TokenHolding};

pub const ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID: ProgramId =
    curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
pub enum TradeMode {
    BuyOnly = 0,
    BuyAndSell = 1,
}

impl TryFrom<u8> for TradeMode {
    type Error = CreateError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::BuyOnly),
            1 => Ok(Self::BuyAndSell),
            _ => Err(CreateError::InvalidTradeMode),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum CreationStage {
    Minted,
    Allocated,
    PoolPrepared,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum SettlementStage {
    Unstarted,
    Ready,
    Withdrawn { unsold: u128, collateral_due: u128 },
    Burned { collateral_due: u128 },
    TokenPaid { collateral_due: u128 },
    Complete,
    HookReady { collateral_due: u128 },
    Graduated,
    NoPayout,
}

impl SettlementStage {
    /// NoPayout completes manual settlement but still permits a saved hook.
    pub fn is_manual_settlement_complete(self) -> bool {
        matches!(self, Self::Complete | Self::NoPayout | Self::Graduated)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct GraduationHook {
    pub program_id: ProgramId,
    pub configuration: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraduationRequest {
    pub factory_program_id: ProgramId,
    pub namespace: AccountId,
    pub launch_salt: [u8; 32],
    pub token_definition_id: AccountId,
    pub collateral_definition_id: AccountId,
    pub dex_seed_reserve: u128,
    pub collateral_amount: u128,
    pub configuration: Vec<u8>,
}

mod hook;

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FactoryState {
    pub trade_mode: TradeMode,
    pub graduation_hook: Option<GraduationHook>,
    pub creation_stage: CreationStage,
    pub settlement_stage: SettlementStage,
    pub end_timestamp: Option<u64>,
    pub namespace: AccountId,
    pub launch_salt: [u8; 32],
    pub token_definition_id: AccountId,
    pub collateral_definition_id: AccountId,
    pub sale_reserve: u128,
    pub dex_seed_reserve: u128,
    pub creator_allocation: u128,
    pub total_supply: u128,
    pub virtual_token_reserve: u128,
    pub virtual_collateral_reserve: u128,
    pub curve_program_id: ProgramId,
    pub creator_commitment: [u8; 32],
    pub creator_escrow_id: AccountId,
    pub pool_id: AccountId,
    pub creator_allocation_claimed: bool,
}

impl TryFrom<&Data> for FactoryState {
    type Error = std::io::Error;
    fn try_from(data: &Data) -> Result<Self, Self::Error> {
        Self::try_from_slice(data.as_ref())
    }
}

impl From<&FactoryState> for Data {
    fn from(state: &FactoryState) -> Self {
        let mut bytes = Vec::with_capacity(std::mem::size_of_val(state));
        BorshSerialize::serialize(state, &mut bytes).expect("factory state serialises");
        Self::try_from(bytes).expect("factory state fits account data")
    }
}

#[expect(
    clippy::large_enum_variant,
    reason = "the compact wire enum keeps guest serialization direct"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Instruction {
    CreateFactoryPool {
        trade_mode: u8,
        graduation_hook_program_id: Option<ProgramId>,
        graduation_hook_configuration: Vec<u8>,
        namespace: AccountId,
        launch_salt: [u8; 32],
        name: String,
        uri: String,
        sale_reserve: u128,
        dex_seed_reserve: u128,
        creator_allocation: u128,
        virtual_token_reserve: u128,
        virtual_collateral_reserve: u128,
        end_timestamp: Option<u64>,
        curve_program_id: ProgramId,
    },
    ContinueCreation,
    ClaimCreatorAllocation,
    WithdrawFactoryProceeds,
    ExecuteGraduationHook {
        remaining_accounts: u32,
    },
    /// Select the pinned privacy circuit's transaction-wide PDA authorization rules.
    Private {
        instruction: Box<Instruction>,
    },
}

/// The only factory-facing validation error. Account and token adapter violations use
/// explicit panics, matching the neighbouring curve-core handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateError {
    InvalidTradeMode,
    SaleReserveZero,
    VirtualTokenReserveNotAboveSaleReserve,
    VirtualReserveZero,
    VirtualReserveAboveBound,
    SupplyOverflow,
    EmptyName,
    EmptyUri,
}

impl std::fmt::Display for CreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::InvalidTradeMode => "trade mode must be 0 or 1",
            Self::SaleReserveZero => "a factory launch needs a non-zero sale reserve",
            Self::VirtualTokenReserveNotAboveSaleReserve => {
                "virtual token reserve must exceed the tradeable sale reserve"
            }
            Self::VirtualReserveZero => "virtual reserves must be non-zero",
            Self::VirtualReserveAboveBound => {
                "virtual reserves must stay below 2^64 for checked curve arithmetic"
            }
            Self::SupplyOverflow => "launch allocations exceed u128 total supply",
            Self::EmptyName => "token name must not be empty",
            Self::EmptyUri => "token metadata URI must not be empty",
        };
        f.write_str(text)
    }
}
impl std::error::Error for CreateError {}

pub fn total_supply(
    sale_reserve: u128,
    dex_seed_reserve: u128,
    creator_allocation: u128,
) -> Result<u128, CreateError> {
    if sale_reserve == 0 {
        return Err(CreateError::SaleReserveZero);
    }
    sale_reserve
        .checked_add(dex_seed_reserve)
        .and_then(|value| value.checked_add(creator_allocation))
        .ok_or(CreateError::SupplyOverflow)
}

/// Checks the supply-side curve condition required for a factory launch.
///
/// The virtual token reserve is a pricing parameter, while `sale_reserve` is
/// real tradeable inventory. Keeping the former strictly larger ensures the
/// curve reaches its supply target before its asymptote.
pub fn validate_curve_parameters(
    sale_reserve: u128,
    virtual_token_reserve: u128,
    virtual_collateral_reserve: u128,
) -> Result<(), CreateError> {
    if virtual_token_reserve <= sale_reserve {
        return Err(CreateError::VirtualTokenReserveNotAboveSaleReserve);
    }
    if virtual_token_reserve == 0 || virtual_collateral_reserve == 0 {
        return Err(CreateError::VirtualReserveZero);
    }
    if virtual_token_reserve >= pool::VIRTUAL_RESERVE_BOUND
        || virtual_collateral_reserve >= pool::VIRTUAL_RESERVE_BOUND
    {
        return Err(CreateError::VirtualReserveAboveBound);
    }
    Ok(())
}

fn seed(namespace: AccountId, tag: &[u8], launch_salt: [u8; 32]) -> PdaSeed {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let mut bytes = [0_u8; 96];
    bytes[..tag.len()].copy_from_slice(tag);
    bytes[32..64].copy_from_slice(&launch_salt);
    bytes[64..].copy_from_slice(&namespace.to_bytes());
    PdaSeed::new(
        Impl::hash_bytes(&bytes)
            .as_bytes()
            .try_into()
            .expect("sha256 is 32 bytes"),
    )
}
pub fn compute_graduation_seed(factory: AccountId) -> PdaSeed {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let mut bytes = b"launchpad-graduation-v1".to_vec();
    bytes.extend_from_slice(&factory.to_bytes());
    PdaSeed::new(
        Impl::hash_bytes(&bytes)
            .as_bytes()
            .try_into()
            .expect("sha256 is 32 bytes"),
    )
}

pub fn compute_graduation_pda(adapter: ProgramId, factory: AccountId) -> AccountId {
    AccountId::for_public_pda(&adapter, &compute_graduation_seed(factory))
}

pub fn compute_factory_seed(namespace: AccountId, launch_salt: [u8; 32]) -> PdaSeed {
    seed(namespace, b"factory", launch_salt)
}
pub fn compute_definition_seed(namespace: AccountId, launch_salt: [u8; 32]) -> PdaSeed {
    seed(namespace, b"definition", launch_salt)
}
pub fn compute_mint_seed(namespace: AccountId, launch_salt: [u8; 32]) -> PdaSeed {
    seed(namespace, b"mint", launch_salt)
}
pub fn compute_metadata_seed(namespace: AccountId, launch_salt: [u8; 32]) -> PdaSeed {
    seed(namespace, b"metadata", launch_salt)
}
pub fn compute_escrow_seed(namespace: AccountId, launch_salt: [u8; 32]) -> PdaSeed {
    seed(namespace, b"escrow", launch_salt)
}
pub fn compute_factory_pda(
    namespace: AccountId,
    factory_program_id: ProgramId,
    launch_salt: [u8; 32],
) -> AccountId {
    AccountId::for_public_pda(
        &factory_program_id,
        &compute_factory_seed(namespace, launch_salt),
    )
}
pub fn compute_definition_pda(
    namespace: AccountId,
    factory_program_id: ProgramId,
    launch_salt: [u8; 32],
) -> AccountId {
    AccountId::for_public_pda(
        &factory_program_id,
        &compute_definition_seed(namespace, launch_salt),
    )
}
pub fn compute_mint_pda(
    namespace: AccountId,
    factory_program_id: ProgramId,
    launch_salt: [u8; 32],
) -> AccountId {
    AccountId::for_public_pda(
        &factory_program_id,
        &compute_mint_seed(namespace, launch_salt),
    )
}
pub fn compute_metadata_pda(
    namespace: AccountId,
    factory_program_id: ProgramId,
    launch_salt: [u8; 32],
) -> AccountId {
    AccountId::for_public_pda(
        &factory_program_id,
        &compute_metadata_seed(namespace, launch_salt),
    )
}
pub fn compute_escrow_pda(
    namespace: AccountId,
    factory_program_id: ProgramId,
    launch_salt: [u8; 32],
) -> AccountId {
    AccountId::for_public_pda(
        &factory_program_id,
        &compute_escrow_seed(namespace, launch_salt),
    )
}

/// Commits a private creator account to one launch without recording its account ID in factory
/// state. The creator account itself must be authorized whenever this commitment is used.
pub fn compute_creator_commitment(
    namespace: AccountId,
    creator_id: AccountId,
    launch_salt: [u8; 32],
) -> [u8; 32] {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let mut bytes = [0_u8; 96];
    bytes[..32].copy_from_slice(&creator_id.to_bytes());
    bytes[32..64].copy_from_slice(&launch_salt);
    bytes[64..].copy_from_slice(&namespace.to_bytes());
    Impl::hash_bytes(&bytes)
        .as_bytes()
        .try_into()
        .expect("sha256 is 32 bytes")
}

struct CreateFactoryPoolAccounts {
    config: AccountWithMetadata,
    factory: AccountWithMetadata,
    token_definition: AccountWithMetadata,
    mint_holding: AccountWithMetadata,
    metadata: AccountWithMetadata,
    creator_escrow: AccountWithMetadata,
    creator: AccountWithMetadata,
    creator_holding: AccountWithMetadata,
    collateral_definition: AccountWithMetadata,
    factory_token_ata: AccountWithMetadata,
    factory_collateral_ata: AccountWithMetadata,
    pool: AccountWithMetadata,
    pool_token_ata: AccountWithMetadata,
    pool_collateral_ata: AccountWithMetadata,
    clock: AccountWithMetadata,
}

struct CreateFactoryPoolParams {
    trade_mode: TradeMode,
    graduation_hook_program_id: Option<ProgramId>,
    graduation_hook_configuration: Vec<u8>,
    namespace: AccountId,
    launch_salt: [u8; 32],
    name: String,
    uri: String,
    sale_reserve: u128,
    dex_seed_reserve: u128,
    creator_allocation: u128,
    virtual_token_reserve: u128,
    virtual_collateral_reserve: u128,
    end_timestamp: Option<u64>,
    factory_program_id: ProgramId,
    curve_program_id: ProgramId,
}

#[must_use]
fn create_factory_pool(
    accounts: CreateFactoryPoolAccounts,
    params: CreateFactoryPoolParams,
) -> (Vec<AccountPostState>, Vec<ChainedCall>) {
    let CreateFactoryPoolAccounts {
        config,
        factory,
        token_definition,
        mint_holding,
        metadata,
        creator_escrow,
        creator,
        creator_holding,
        collateral_definition,
        factory_token_ata,
        factory_collateral_ata,
        pool,
        pool_token_ata,
        pool_collateral_ata,
        clock,
    } = accounts;
    let CreateFactoryPoolParams {
        trade_mode,
        graduation_hook_program_id,
        graduation_hook_configuration,
        namespace,
        launch_salt,
        name,
        uri,
        sale_reserve,
        dex_seed_reserve,
        creator_allocation,
        virtual_token_reserve,
        virtual_collateral_reserve,
        end_timestamp,
        factory_program_id,
        curve_program_id,
    } = params;
    assert!(
        graduation_hook_program_id.is_some() || graduation_hook_configuration.is_empty(),
        "Hook configuration requires a program"
    );
    curve_core::validated_config(&config, namespace, curve_program_id);
    assert!(!name.is_empty(), "token name must not be empty");
    assert!(!uri.is_empty(), "token metadata URI must not be empty");
    let supply = total_supply(sale_reserve, dex_seed_reserve, creator_allocation)
        .expect("launch allocations are valid");
    validate_curve_parameters(
        sale_reserve,
        virtual_token_reserve,
        virtual_collateral_reserve,
    )
    .expect("factory curve parameters are valid");
    assert_eq!(
        factory.account_id,
        compute_factory_pda(namespace, factory_program_id, launch_salt),
        "Factory account ID does not match PDA"
    );
    assert_eq!(
        factory.account,
        Account::default(),
        "Launch salt is already in use"
    );
    assert_eq!(
        token_definition.account_id,
        compute_definition_pda(namespace, factory_program_id, launch_salt),
        "Token definition ID does not match PDA"
    );
    assert_eq!(
        mint_holding.account_id,
        compute_mint_pda(namespace, factory_program_id, launch_salt),
        "Mint holding ID does not match PDA"
    );
    assert_eq!(
        metadata.account_id,
        compute_metadata_pda(namespace, factory_program_id, launch_salt),
        "Metadata ID does not match PDA"
    );
    assert_eq!(
        creator_escrow.account_id,
        compute_escrow_pda(namespace, factory_program_id, launch_salt),
        "Creator escrow ID does not match PDA"
    );
    assert!(creator.is_authorized, "Creator authorization is missing");
    curve_core::identity(&creator);
    let now = trusted_time(&clock);
    assert!(
        end_timestamp.is_none_or(|timestamp| timestamp > now),
        "End timestamp must be in the future"
    );
    associated_token_account_core::verify_ata_and_get_seed(
        &creator_holding,
        &creator,
        token_definition.account_id,
        ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
    );
    assert_eq!(
        pool.account_id,
        compute_pool_pda(
            namespace,
            curve_program_id,
            token_definition.account_id,
            collateral_definition.account_id,
            factory.account_id
        ),
        "Pool ID does not match factory-owned PDA"
    );

    let state = FactoryState {
        trade_mode,
        graduation_hook: graduation_hook_program_id.map(|program_id| GraduationHook {
            program_id,
            configuration: graduation_hook_configuration,
        }),
        creation_stage: CreationStage::Minted,
        settlement_stage: SettlementStage::Unstarted,
        end_timestamp,
        namespace,
        launch_salt,
        token_definition_id: token_definition.account_id,
        collateral_definition_id: collateral_definition.account_id,
        sale_reserve,
        dex_seed_reserve,
        creator_allocation,
        total_supply: supply,
        virtual_token_reserve,
        virtual_collateral_reserve,
        curve_program_id,
        creator_commitment: compute_creator_commitment(
            namespace,
            curve_core::identity(&creator),
            launch_salt,
        ),
        creator_escrow_id: creator_escrow.account_id,
        pool_id: pool.account_id,
        creator_allocation_claimed: false,
    };
    let calls = vec![
        ChainedCall::new(
            curve_core::TOKEN_PROGRAM_ID,
            vec![
                AccountWithMetadata {
                    is_authorized: true,
                    ..token_definition.clone()
                },
                AccountWithMetadata {
                    is_authorized: true,
                    ..mint_holding.clone()
                },
                AccountWithMetadata {
                    is_authorized: true,
                    ..metadata.clone()
                },
            ],
            &token_core::Instruction::NewDefinitionWithMetadata {
                new_definition: NewTokenDefinition::Fungible {
                    name,
                    total_supply: supply,
                },
                metadata: Box::new(NewTokenMetadata {
                    standard: MetadataStandard::Simple,
                    uri,
                    creators: String::new(),
                }),
            },
        )
        .with_pda_seeds(vec![
            compute_definition_seed(namespace, launch_salt),
            compute_mint_seed(namespace, launch_salt),
            compute_metadata_seed(namespace, launch_salt),
        ]),
    ];
    let mut post = factory.account;
    post.data = Data::from(&state);
    (
        vec![
            AccountPostState::new_claimed_if_default(
                post,
                Claim::Pda(compute_factory_seed(namespace, launch_salt)),
            ),
            AccountPostState::new(token_definition.account),
            AccountPostState::new(mint_holding.account),
            AccountPostState::new(metadata.account),
            AccountPostState::new(creator_escrow.account),
            AccountPostState::new(creator.account),
            AccountPostState::new(creator_holding.account),
            AccountPostState::new(collateral_definition.account),
            AccountPostState::new(factory_token_ata.account),
            AccountPostState::new(factory_collateral_ata.account),
            AccountPostState::new(pool.account),
            AccountPostState::new(pool_token_ata.account),
            AccountPostState::new(pool_collateral_ata.account),
            AccountPostState::new(clock.account),
            AccountPostState::new(config.account),
        ],
        calls,
    )
}

/// Advances one bounded creation stage. Amounts and program IDs come only from persisted state.
pub fn continue_creation(
    pre: Vec<AccountWithMetadata>,
    program: ProgramId,
) -> (Vec<AccountPostState>, Vec<ChainedCall>) {
    let [
        factory,
        definition,
        mint,
        _metadata,
        escrow,
        creator,
        _creator_holding,
        collateral,
        factory_token,
        factory_collateral,
        pool,
        reserve0,
        reserve1,
        clock,
        config,
    ]: [_; 15] = pre
        .clone()
        .try_into()
        .expect("ContinueCreation requires fifteen accounts");
    let mut state = FactoryState::try_from(&factory.account.data).expect("valid factory state");
    validate_creator(&factory, &creator, &state, program);
    assert_eq!(
        definition.account_id, state.token_definition_id,
        "Wrong launch definition"
    );
    assert_eq!(
        collateral.account_id, state.collateral_definition_id,
        "Wrong collateral definition"
    );
    assert_eq!(
        mint.account_id,
        compute_mint_pda(state.namespace, program, state.launch_salt),
        "Wrong mint holding"
    );
    assert_eq!(
        escrow.account_id, state.creator_escrow_id,
        "Wrong creator escrow"
    );
    assert_eq!(pool.account_id, state.pool_id, "Wrong pool");
    let stage = state.creation_stage;
    state.creation_stage = match stage {
        CreationStage::Minted => CreationStage::Allocated,
        CreationStage::Allocated => CreationStage::PoolPrepared,
        CreationStage::PoolPrepared => CreationStage::Active,
        CreationStage::Active => panic!("Creation is already complete"),
    };
    let mut factory_post = factory.account.clone();
    factory_post.data = Data::from(&state);
    let factory_snapshot = AccountWithMetadata {
        account: factory_post.clone(),
        ..factory.clone()
    };
    let owner = AccountWithMetadata {
        is_authorized: true,
        ..factory_snapshot.clone()
    };
    let calls = match stage {
        CreationStage::Minted => {
            associated_token_account_core::verify_ata_and_get_seed(
                &factory_token,
                &factory_snapshot,
                definition.account_id,
                ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
            );
            let mut calls = vec![ChainedCall::new(
                ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                vec![factory_snapshot, definition.clone(), factory_token.clone()],
                &associated_token_account_core::Instruction::Create {
                    ata_program_id: ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                },
            )];
            let amount = state
                .sale_reserve
                .checked_add(state.dex_seed_reserve)
                .expect("checked supply");
            let mut sender = mint;
            sender.is_authorized = true;
            calls.push(
                ChainedCall::new(
                    curve_core::TOKEN_PROGRAM_ID,
                    vec![
                        sender.clone(),
                        curve_core::after_ata_creation(&factory_token, &definition),
                    ],
                    &token_core::Instruction::Transfer {
                        amount_to_transfer: amount,
                    },
                )
                .with_pda_seeds(vec![compute_mint_seed(state.namespace, state.launch_salt)]),
            );
            if state.creator_allocation != 0 {
                let mut holding =
                    TokenHolding::try_from(&sender.account.data).expect("mint holding");
                let TokenHolding::Fungible { balance, .. } = &mut holding else {
                    panic!("fungible mint required")
                };
                *balance = balance.checked_sub(amount).expect("mint supply");
                sender.account.data = Data::from(&holding);
                calls.push(
                    ChainedCall::new(
                        curve_core::TOKEN_PROGRAM_ID,
                        vec![
                            sender,
                            AccountWithMetadata {
                                is_authorized: true,
                                ..escrow
                            },
                        ],
                        &token_core::Instruction::Transfer {
                            amount_to_transfer: state.creator_allocation,
                        },
                    )
                    .with_pda_seeds(vec![
                        compute_mint_seed(state.namespace, state.launch_salt),
                        compute_escrow_seed(state.namespace, state.launch_salt),
                    ]),
                );
            }
            calls
        }
        CreationStage::Allocated | CreationStage::PoolPrepared => {
            let instruction = if stage == CreationStage::Allocated {
                CurveInstruction::CreatePool {
                    allowed_input_token: match state.trade_mode {
                        TradeMode::BuyOnly => Some(1),
                        TradeMode::BuyAndSell => None,
                    },
                    defer_funding: true,
                    namespace: state.namespace,
                    token0_amount: state.sale_reserve,
                    token1_amount: 0,
                    virtual_reserve0: state.virtual_token_reserve,
                    virtual_reserve1: state.virtual_collateral_reserve,
                    close_timestamp: state.end_timestamp,
                    close_on_depletion: Some(0),
                    owner: factory.account_id,
                    owner_program_id: Some(program),
                    owner_program_seed: Some(
                        *compute_factory_seed(state.namespace, state.launch_salt).as_bytes(),
                    ),
                    curve_program_id: state.curve_program_id,
                }
            } else {
                CurveInstruction::ActivatePool
            };
            vec![
                ChainedCall::new(
                    state.curve_program_id,
                    vec![
                        pool,
                        owner,
                        definition,
                        collateral,
                        factory_token,
                        factory_collateral,
                        reserve0,
                        reserve1,
                        clock,
                        config,
                    ],
                    &instruction,
                )
                .with_pda_seeds(vec![compute_factory_seed(
                    state.namespace,
                    state.launch_salt,
                )]),
            ]
        }
        CreationStage::Active => unreachable!(),
    };
    let mut posts: Vec<_> = pre
        .into_iter()
        .map(|p| AccountPostState::new(p.account))
        .collect();
    posts[0] = AccountPostState::new(factory_post);
    (posts, calls)
}

fn validate_creator(
    factory: &AccountWithMetadata,
    creator: &AccountWithMetadata,
    state: &FactoryState,
    program: ProgramId,
) {
    assert_eq!(
        factory.account.program_owner, program,
        "Wrong factory program owner"
    );
    assert_eq!(
        factory.account_id,
        compute_factory_pda(state.namespace, program, state.launch_salt),
        "Factory account ID does not match PDA"
    );
    assert_eq!(
        compute_creator_commitment(
            state.namespace,
            curve_core::identity(creator),
            state.launch_salt
        ),
        state.creator_commitment,
        "Creator commitment does not match launch"
    );
}

struct ClaimCreatorAllocationAccounts {
    factory: AccountWithMetadata,
    pool: AccountWithMetadata,
    escrow: AccountWithMetadata,
    creator: AccountWithMetadata,
    token_definition: AccountWithMetadata,
    creator_holding: AccountWithMetadata,
    clock: AccountWithMetadata,
}

struct ClaimCreatorAllocationParams {
    factory_program_id: ProgramId,
}

/// Marks a delayed allocation as released once the factory-owned pool is closed. Token
/// transfer plumbing remains in the token adapter; this state transition makes repeats impossible.
#[must_use]
fn claim_creator_allocation(
    accounts: ClaimCreatorAllocationAccounts,
    params: ClaimCreatorAllocationParams,
) -> (Vec<AccountPostState>, Vec<ChainedCall>) {
    let ClaimCreatorAllocationAccounts {
        factory,
        pool,
        escrow,
        creator,
        token_definition,
        creator_holding,
        clock,
    } = accounts;
    let ClaimCreatorAllocationParams { factory_program_id } = params;
    let mut state =
        FactoryState::try_from(&factory.account.data).expect("Factory account holds invalid data");
    assert_eq!(
        state.creation_stage,
        CreationStage::Active,
        "Creation is not complete"
    );
    assert_eq!(
        factory.account_id,
        compute_factory_pda(state.namespace, factory_program_id, state.launch_salt),
        "Factory account ID does not match PDA"
    );
    assert!(creator.is_authorized, "Creator authorization is missing");
    curve_core::identity(&creator);
    assert_eq!(
        compute_creator_commitment(
            state.namespace,
            curve_core::identity(&creator),
            state.launch_salt
        ),
        state.creator_commitment,
        "Creator commitment does not match launch"
    );
    assert_eq!(
        pool.account_id, state.pool_id,
        "Pool does not belong to factory launch"
    );
    assert_eq!(
        escrow.account_id, state.creator_escrow_id,
        "Creator escrow does not belong to factory launch"
    );
    assert_eq!(
        token_definition.account_id, state.token_definition_id,
        "Token definition does not belong to factory launch"
    );
    associated_token_account_core::verify_ata_and_get_seed(
        &creator_holding,
        &creator,
        state.token_definition_id,
        ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
    );
    assert!(
        !state.creator_allocation_claimed,
        "Creator allocation is already claimed"
    );
    let pool_state =
        PoolAccount::try_from(&pool.account.data).expect("Pool account holds invalid data");
    assert_eq!(
        pool_state.namespace, state.namespace,
        "Pool belongs to another namespace"
    );
    assert!(
        pool_state.pool.effective_lifecycle(trusted_time(&clock)) != PoolLifecycle::Open,
        "Pool must be closed before creator allocation claim"
    );
    state.creator_allocation_claimed = true;
    let mut post = factory.account;
    post.data = Data::from(&state);
    let calls = if state.creator_allocation == 0 {
        vec![]
    } else {
        let holding = TokenHolding::try_from(&escrow.account.data)
            .expect("Creator escrow must hold launch tokens");
        let amount = match holding {
            TokenHolding::Fungible { balance, .. } => {
                assert!(
                    balance >= state.creator_allocation,
                    "Escrow allocation missing"
                );
                state.creator_allocation
            }
            _ => panic!("Creator escrow must hold fungible launch tokens"),
        };
        vec![
            ChainedCall::new(
                ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                vec![
                    creator.clone(),
                    token_definition.clone(),
                    creator_holding.clone(),
                ],
                &associated_token_account_core::Instruction::Create {
                    ata_program_id: ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                },
            ),
            ChainedCall::new(
                escrow.account.program_owner,
                vec![
                    AccountWithMetadata {
                        is_authorized: true,
                        ..escrow.clone()
                    },
                    curve_core::after_ata_creation(&creator_holding, &token_definition),
                ],
                &token_core::Instruction::Transfer {
                    amount_to_transfer: amount,
                },
            )
            .with_pda_seeds(vec![compute_escrow_seed(
                state.namespace,
                state.launch_salt,
            )]),
        ]
    };
    (
        vec![
            AccountPostState::new(post),
            AccountPostState::new(pool.account),
            AccountPostState::new(escrow.account.clone()),
            AccountPostState::new(creator.account),
            AccountPostState::new(token_definition.account),
            AccountPostState::new(creator_holding.account.clone()),
            AccountPostState::new(clock.account),
        ],
        calls,
    )
}

// The trailing treasury ATA can alias an existing recipient. Curve withdrawal
// accepts its omission when it is the owner token1 recipient.
fn unique_treasury_accounts(mut accounts: Vec<AccountWithMetadata>) -> Vec<AccountWithMetadata> {
    let last = accounts.last().expect("withdrawal accounts").account_id;
    if accounts[..accounts.len() - 1]
        .iter()
        .any(|a| a.account_id == last)
    {
        accounts.pop();
    }
    accounts
}

struct WithdrawFactoryProceedsAccounts {
    factory: AccountWithMetadata,
    pool: AccountWithMetadata,
    creator: AccountWithMetadata,
    token_definition: AccountWithMetadata,
    collateral_definition: AccountWithMetadata,
    factory_token_ata: AccountWithMetadata,
    factory_collateral_ata: AccountWithMetadata,
    pool_token_ata: AccountWithMetadata,
    pool_collateral_ata: AccountWithMetadata,
    creator_token_ata: AccountWithMetadata,
    creator_collateral_ata: AccountWithMetadata,
    clock: AccountWithMetadata,
    config: AccountWithMetadata,
    treasury_ata: AccountWithMetadata,
}

struct WithdrawFactoryProceedsParams {
    factory_program_id: ProgramId,
}

/// Withdraws the closed pool, burns unsold token0, then returns R and all collateral to creator.
#[must_use]
fn withdraw_factory_proceeds(
    accounts: WithdrawFactoryProceedsAccounts,
    params: WithdrawFactoryProceedsParams,
) -> (Vec<AccountPostState>, Vec<ChainedCall>) {
    settle_factory_proceeds(accounts, params, false)
}

fn settle_factory_proceeds(
    accounts: WithdrawFactoryProceedsAccounts,
    params: WithdrawFactoryProceedsParams,
    hook: bool,
) -> (Vec<AccountPostState>, Vec<ChainedCall>) {
    let WithdrawFactoryProceedsAccounts {
        factory,
        pool,
        creator,
        token_definition,
        collateral_definition,
        factory_token_ata,
        factory_collateral_ata,
        pool_token_ata,
        pool_collateral_ata,
        creator_token_ata,
        creator_collateral_ata,
        clock,
        config,
        treasury_ata,
    } = accounts;
    let WithdrawFactoryProceedsParams { factory_program_id } = params;
    let pre = vec![
        factory.clone(),
        pool.clone(),
        creator.clone(),
        token_definition.clone(),
        collateral_definition.clone(),
        factory_token_ata.clone(),
        factory_collateral_ata.clone(),
        pool_token_ata.clone(),
        pool_collateral_ata.clone(),
        creator_token_ata.clone(),
        creator_collateral_ata.clone(),
        clock.clone(),
        config.clone(),
        treasury_ata.clone(),
    ];
    let mut state = FactoryState::try_from(&factory.account.data).expect("valid factory state");
    if hook {
        assert_eq!(
            factory.account.program_owner, factory_program_id,
            "Wrong factory program owner"
        );
        assert_eq!(
            factory.account_id,
            compute_factory_pda(state.namespace, factory_program_id, state.launch_salt),
            "Factory account ID does not match PDA"
        );
        let settings = state
            .graduation_hook
            .as_ref()
            .expect("Graduation hook is not configured");
        if matches!(
            state.settlement_stage,
            SettlementStage::Burned { .. }
                | SettlementStage::HookReady { .. }
                | SettlementStage::NoPayout
        ) {
            assert_eq!(
                creator.account.program_owner, settings.program_id,
                "Adapter must initialize its custody PDA before ATA preparation"
            );
        }
        assert_eq!(
            creator.account_id,
            compute_graduation_pda(settings.program_id, factory.account_id),
            "Wrong adapter custody PDA"
        );
    } else {
        assert!(creator.is_authorized, "Creator authorization is missing");
        validate_creator(&factory, &creator, &state, factory_program_id);
    }
    assert_eq!(
        state.creation_stage,
        CreationStage::Active,
        "Creation is not complete"
    );
    assert_eq!(
        pool.account_id, state.pool_id,
        "Pool does not belong to factory launch"
    );
    assert_eq!(
        token_definition.account_id, state.token_definition_id,
        "Wrong launch definition"
    );
    assert_eq!(
        collateral_definition.account_id, state.collateral_definition_id,
        "Wrong collateral definition"
    );
    let pool_state = PoolAccount::try_from(&pool.account.data).expect("valid pool");
    assert!(pool_state.funded, "Pool funding is pending");
    assert_eq!(
        pool_state.namespace, state.namespace,
        "Pool belongs to another namespace"
    );
    assert_eq!(
        pool.account.program_owner, state.curve_program_id,
        "Wrong curve owner"
    );
    assert!(
        pool_state.pool.effective_lifecycle(trusted_time(&clock)) != PoolLifecycle::Open,
        "Pool must be closed before proceeds withdrawal"
    );
    for (account, owner, definition) in [
        (&factory_token_ata, &factory, state.token_definition_id),
        (
            &factory_collateral_ata,
            &factory,
            state.collateral_definition_id,
        ),
        (&creator_token_ata, &creator, state.token_definition_id),
        (
            &creator_collateral_ata,
            &creator,
            state.collateral_definition_id,
        ),
    ] {
        associated_token_account_core::verify_ata_and_get_seed(
            account,
            owner,
            definition,
            ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
        );
    }
    let stage = state.settlement_stage;
    assert!(
        !hook
            || !matches!(
                stage,
                SettlementStage::TokenPaid { .. } | SettlementStage::Complete
            ),
        "Manual payout has started"
    );
    state.settlement_stage = match stage {
        SettlementStage::Unstarted => SettlementStage::Ready,
        SettlementStage::Ready => SettlementStage::Withdrawn {
            unsold: pool_state.pool.real_reserve0,
            collateral_due: pool_state.pool.real_reserve1,
        },
        SettlementStage::Withdrawn { collateral_due, .. } => {
            SettlementStage::Burned { collateral_due }
        }
        SettlementStage::Burned { collateral_due } if hook => {
            SettlementStage::HookReady { collateral_due }
        }
        SettlementStage::Burned { collateral_due }
        | SettlementStage::HookReady { collateral_due }
            if !hook && state.dex_seed_reserve == 0 =>
        {
            if collateral_due == 0 {
                SettlementStage::NoPayout
            } else {
                SettlementStage::Complete
            }
        }
        SettlementStage::NoPayout if hook => SettlementStage::HookReady { collateral_due: 0 },
        SettlementStage::NoPayout => panic!("Proceeds settlement has no payout"),
        SettlementStage::Burned { collateral_due } => SettlementStage::TokenPaid { collateral_due },
        SettlementStage::HookReady { .. } if hook => SettlementStage::Graduated,
        SettlementStage::HookReady { collateral_due } => {
            SettlementStage::TokenPaid { collateral_due }
        }
        SettlementStage::Graduated => panic!("Proceeds have graduated"),
        SettlementStage::TokenPaid { .. } => SettlementStage::Complete,
        SettlementStage::Complete => panic!("Proceeds settlement is already complete"),
    };
    let mut post = factory.account.clone();
    post.data = Data::from(&state);
    let snapshot = AccountWithMetadata {
        account: post.clone(),
        ..factory
    };
    let owner = AccountWithMetadata {
        is_authorized: true,
        ..snapshot.clone()
    };
    let seeds = vec![compute_factory_seed(state.namespace, state.launch_salt)];
    let calls = match stage {
        SettlementStage::Unstarted => vec![ChainedCall::new(
            ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
            vec![snapshot, collateral_definition, factory_collateral_ata],
            &associated_token_account_core::Instruction::Create {
                ata_program_id: ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
            },
        )],
        SettlementStage::Ready => vec![
            ChainedCall::new(
                state.curve_program_id,
                unique_treasury_accounts(vec![
                    pool,
                    owner,
                    factory_token_ata,
                    factory_collateral_ata,
                    pool_token_ata,
                    pool_collateral_ata,
                    clock,
                    config,
                    treasury_ata,
                ]),
                &CurveInstruction::WithdrawReserves,
            )
            .with_pda_seeds(seeds),
        ],
        SettlementStage::Withdrawn { unsold, .. } => {
            if unsold == 0 {
                vec![]
            } else {
                vec![
                    ChainedCall::new(
                        ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                        vec![owner, factory_token_ata, token_definition],
                        &associated_token_account_core::Instruction::Burn {
                            ata_program_id: ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                            amount: unsold,
                        },
                    )
                    .with_pda_seeds(seeds),
                ]
            }
        }
        SettlementStage::Burned { .. } | SettlementStage::NoPayout if hook => vec![
            ChainedCall::new(
                ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                vec![creator.clone(), token_definition, creator_token_ata],
                &associated_token_account_core::Instruction::Create {
                    ata_program_id: ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                },
            ),
            ChainedCall::new(
                ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                vec![creator, collateral_definition, creator_collateral_ata],
                &associated_token_account_core::Instruction::Create {
                    ata_program_id: ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                },
            ),
        ],
        SettlementStage::Burned { collateral_due }
        | SettlementStage::HookReady { collateral_due }
            if !hook && state.dex_seed_reserve == 0 =>
        {
            payout_calls(
                owner,
                creator,
                factory_collateral_ata,
                creator_collateral_ata,
                collateral_definition,
                collateral_due,
                seeds,
            )
        }
        SettlementStage::Burned { .. } | SettlementStage::HookReady { .. } if !hook => {
            payout_calls(
                owner,
                creator,
                factory_token_ata,
                creator_token_ata,
                token_definition,
                state.dex_seed_reserve,
                seeds,
            )
        }
        SettlementStage::TokenPaid { collateral_due } => payout_calls(
            owner,
            creator,
            factory_collateral_ata,
            creator_collateral_ata,
            collateral_definition,
            collateral_due,
            seeds,
        ),
        SettlementStage::HookReady { collateral_due } => {
            hook::final_calls(&pre, snapshot, &state, factory_program_id, collateral_due)
        }
        SettlementStage::Complete
        | SettlementStage::Graduated
        | SettlementStage::NoPayout
        | SettlementStage::Burned { .. } => unreachable!(),
    };
    let mut posts: Vec<_> = pre
        .into_iter()
        .map(|p| AccountPostState::new(p.account))
        .collect();
    posts[0] = AccountPostState::new(post);
    (posts, calls)
}

fn payout_calls(
    owner: AccountWithMetadata,
    creator: AccountWithMetadata,
    source: AccountWithMetadata,
    recipient: AccountWithMetadata,
    definition: AccountWithMetadata,
    amount: u128,
    seeds: Vec<PdaSeed>,
) -> Vec<ChainedCall> {
    if amount == 0 {
        return vec![];
    }
    let initialized = curve_core::after_ata_creation(&recipient, &definition);
    vec![
        ChainedCall::new(
            ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
            vec![creator, definition, recipient],
            &associated_token_account_core::Instruction::Create {
                ata_program_id: ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
            },
        ),
        ChainedCall::new(
            ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
            vec![owner, source, initialized],
            &associated_token_account_core::Instruction::Transfer {
                ata_program_id: ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                amount,
            },
        )
        .with_pda_seeds(seeds),
    ]
}

fn settlement_accounts(
    pre_states: Vec<AccountWithMetadata>,
    program: ProgramId,
) -> WithdrawFactoryProceedsAccounts {
    let mut pre_states = pre_states;
    if pre_states.len() == 13 {
        let state = FactoryState::try_from(&pre_states[0].account.data).expect("valid factory");
        let config =
            curve_core::validated_config(&pre_states[12], state.namespace, state.curve_program_id);
        let treasury_id = associated_token_account_core::get_associated_token_account_id(
            &ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
            &associated_token_account_core::compute_ata_seed(
                config.treasury,
                state.collateral_definition_id,
            ),
        );
        let existing = pre_states
            .iter()
            .find(|a| a.account_id == treasury_id)
            .expect("omitted treasury ATA must already be supplied")
            .clone();
        pre_states.push(existing);
    }

    assert_eq!(
        pre_states.len(),
        14,
        "Settlement requires fourteen accounts"
    );
    hook::validate_settlement(&pre_states, program);
    let [
        factory,
        pool,
        creator,
        token_definition,
        collateral_definition,
        factory_token_ata,
        factory_collateral_ata,
        pool_token_ata,
        pool_collateral_ata,
        creator_token_ata,
        creator_collateral_ata,
        clock,
        config,
        treasury_ata,
    ] = pre_states
        .try_into()
        .expect("WithdrawFactoryProceeds requires exactly fourteen accounts");
    WithdrawFactoryProceedsAccounts {
        factory,
        pool,
        creator,
        token_definition,
        collateral_definition,
        factory_token_ata,
        factory_collateral_ata,
        pool_token_ata,
        pool_collateral_ata,
        creator_token_ata,
        creator_collateral_ata,
        clock,
        config,
        treasury_ata,
    }
}

fn trusted_time(clock: &AccountWithMetadata) -> u64 {
    assert_eq!(
        clock.account_id,
        clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
        "Clock account is not the trusted LEZ clock"
    );
    clock_core::ClockAccountData::from_bytes(clock.account.data.as_ref()).timestamp
}

#[must_use]
pub fn process_instruction(
    pre_states: Vec<AccountWithMetadata>,
    instruction: Instruction,
    factory_program_id: ProgramId,
) -> (Vec<AccountPostState>, Vec<ChainedCall>) {
    let (private, instruction) = match instruction {
        Instruction::Private { instruction } => (true, *instruction),
        instruction => (false, instruction),
    };
    let original = pre_states.clone();
    let saved_curve_program = original
        .first()
        .and_then(|account| FactoryState::try_from(&account.account.data).ok())
        .map(|state| state.curve_program_id);
    let (mut posts, mut calls) = match instruction {
        Instruction::Private { .. } => panic!("nested execution mode wrapper"),
        Instruction::CreateFactoryPool {
            trade_mode,
            graduation_hook_program_id,
            graduation_hook_configuration,
            namespace,
            launch_salt,
            name,
            uri,
            sale_reserve,
            dex_seed_reserve,
            creator_allocation,
            virtual_token_reserve,
            virtual_collateral_reserve,
            end_timestamp,
            curve_program_id,
        } => {
            let [
                factory,
                definition,
                mint,
                metadata,
                escrow,
                creator,
                creator_holding,
                collateral_definition,
                factory_token_ata,
                factory_collateral_ata,
                pool,
                pool_token_ata,
                pool_collateral_ata,
                clock,
                config,
            ] = pre_states
                .try_into()
                .expect("CreateFactoryPool requires exactly fifteen accounts");
            create_factory_pool(
                CreateFactoryPoolAccounts {
                    config,
                    factory,
                    token_definition: definition,
                    mint_holding: mint,
                    metadata,
                    creator_escrow: escrow,
                    creator,
                    creator_holding,
                    collateral_definition,
                    factory_token_ata,
                    factory_collateral_ata,
                    pool,
                    pool_token_ata,
                    pool_collateral_ata,
                    clock,
                },
                CreateFactoryPoolParams {
                    trade_mode: TradeMode::try_from(trade_mode).expect("valid trade mode"),
                    graduation_hook_program_id,
                    graduation_hook_configuration,
                    namespace,
                    launch_salt,
                    name,
                    uri,
                    sale_reserve,
                    dex_seed_reserve,
                    creator_allocation,
                    virtual_token_reserve,
                    virtual_collateral_reserve,
                    end_timestamp,
                    factory_program_id,
                    curve_program_id,
                },
            )
        }
        Instruction::ContinueCreation => continue_creation(pre_states, factory_program_id),
        Instruction::ClaimCreatorAllocation => {
            let [
                factory,
                pool,
                escrow,
                creator,
                token_definition,
                creator_holding,
                clock,
            ] = pre_states
                .try_into()
                .expect("ClaimCreatorAllocation requires exactly seven accounts");
            claim_creator_allocation(
                ClaimCreatorAllocationAccounts {
                    factory,
                    pool,
                    escrow,
                    creator,
                    token_definition,
                    creator_holding,
                    clock,
                },
                ClaimCreatorAllocationParams { factory_program_id },
            )
        }
        Instruction::ExecuteGraduationHook { remaining_accounts } => {
            assert!(!private, "Graduation hook requires public execution");
            let remaining = usize::try_from(remaining_accounts).expect("remaining account count");
            let fixed = pre_states
                .len()
                .checked_sub(remaining)
                .expect("Invalid remaining account count");
            assert!(
                fixed == 13 || fixed == 14,
                "Hook requires thirteen or fourteen fixed accounts"
            );
            let mut pre_states = pre_states;
            let remaining = pre_states.split_off(fixed);
            hook::validate_interface(&pre_states, &remaining, factory_program_id);
            let (mut posts, mut calls) = settle_factory_proceeds(
                settlement_accounts(pre_states, factory_program_id),
                WithdrawFactoryProceedsParams { factory_program_id },
                true,
            );
            if FactoryState::try_from(&posts[0].account().data)
                .expect("valid factory")
                .settlement_stage
                == SettlementStage::Graduated
            {
                calls
                    .last_mut()
                    .expect("hook callback")
                    .pre_states
                    .extend(remaining.clone());
            }
            posts.truncate(fixed);
            posts.extend(
                remaining
                    .into_iter()
                    .map(|account| AccountPostState::new(account.account)),
            );
            (posts, calls)
        }
        Instruction::WithdrawFactoryProceeds => withdraw_factory_proceeds(
            settlement_accounts(pre_states, factory_program_id),
            WithdrawFactoryProceedsParams { factory_program_id },
        ),
    };
    posts.truncate(original.len());
    if private {
        for call in &mut calls {
            if Some(call.program_id) == saved_curve_program
                && call.program_id != ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID
                && call.program_id != curve_core::TOKEN_PROGRAM_ID
            {
                let instruction: CurveInstruction =
                    risc0_zkvm::serde::from_slice(&call.instruction_data)
                        .expect("curve instruction");
                call.instruction_data = risc0_zkvm::serde::to_vec(&CurveInstruction::Private {
                    instruction: Box::new(instruction),
                })
                .expect("serialize private curve call");
            }
        }
    } else {
        curve_core::use_public_authorizations(&original, &mut calls, factory_program_id);
    }
    (posts, calls)
}

#[cfg(test)]
mod hook_tests;

#[cfg(test)]
mod tests {
    fn namespace_config() -> AccountWithMetadata {
        AccountWithMetadata {
            account: Account {
                program_owner: CURVE_PROGRAM_ID,
                data: Data::from(&curve_core::Config {
                    admin: AccountId::new([0xAD; 32]),
                    protocol_fee_bps: 0,
                    treasury: AccountId::new([2; 32]),
                }),
                ..Account::default()
            },
            account_id: curve_core::compute_config_pda(
                AccountId::new([0xAD; 32]),
                CURVE_PROGRAM_ID,
            ),
            is_authorized: false,
        }
    }
    use super::*;
    use pool::{Pool, PoolCreateParams};

    const FACTORY_PROGRAM_ID: ProgramId = [7; 8];
    const CURVE_PROGRAM_ID: ProgramId = [6; 8];
    const TOKEN_PROGRAM_ID: ProgramId = [5; 8];

    fn ata_id(owner: AccountId, definition: AccountId) -> AccountId {
        associated_token_account_core::get_associated_token_account_id(
            &ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
            &associated_token_account_core::compute_ata_seed(owner, definition),
        )
    }

    fn creator(id: u8, is_authorized: bool) -> AccountWithMetadata {
        let account_id = AccountId::new([id; 32]);
        AccountWithMetadata {
            account: if is_authorized {
                Account {
                    program_owner: curve_core::TOKEN_PROGRAM_ID,
                    data: Data::from(&TokenHolding::NftMaster {
                        definition_id: account_id,
                        print_balance: 1,
                    }),
                    ..Account::default()
                }
            } else {
                Account::default()
            },
            account_id,
            is_authorized,
        }
    }

    fn trusted_clock(timestamp: u64) -> AccountWithMetadata {
        AccountWithMetadata {
            account: Account {
                data: Data::try_from(
                    clock_core::ClockAccountData {
                        block_id: 7,
                        timestamp,
                    }
                    .to_bytes(),
                )
                .expect("clock data fits"),
                ..Account::default()
            },
            account_id: clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
            is_authorized: false,
        }
    }

    fn token_definition(account_id: AccountId) -> AccountWithMetadata {
        AccountWithMetadata {
            account: Account {
                program_owner: TOKEN_PROGRAM_ID,
                data: Data::from(&token_core::TokenDefinition::Fungible {
                    name: "Fixture".into(),
                    total_supply: 1000,
                    metadata_id: None,
                }),
                ..Account::default()
            },
            account_id,
            is_authorized: false,
        }
    }

    fn delayed_factory(creator: &AccountWithMetadata) -> (AccountWithMetadata, FactoryState) {
        let launch_salt = [1; 32];
        let state = FactoryState {
            trade_mode: TradeMode::BuyAndSell,
            graduation_hook: None,
            settlement_stage: SettlementStage::Unstarted,
            creation_stage: CreationStage::Active,
            end_timestamp: None,
            namespace: AccountId::new([0xAD; 32]),
            launch_salt,
            token_definition_id: AccountId::new([2; 32]),
            collateral_definition_id: AccountId::new([3; 32]),
            sale_reserve: 800,
            dex_seed_reserve: 100,
            creator_allocation: 100,
            total_supply: 1000,
            virtual_token_reserve: 2000,
            virtual_collateral_reserve: 100,
            curve_program_id: CURVE_PROGRAM_ID,
            creator_commitment: compute_creator_commitment(
                AccountId::new([0xAD; 32]),
                creator.account_id,
                launch_salt,
            ),
            creator_escrow_id: AccountId::new([4; 32]),
            pool_id: AccountId::new([5; 32]),
            creator_allocation_claimed: false,
        };
        (
            AccountWithMetadata {
                account: Account {
                    data: Data::from(&state),
                    ..Account::default()
                },
                account_id: compute_factory_pda(
                    AccountId::new([0xAD; 32]),
                    FACTORY_PROGRAM_ID,
                    launch_salt,
                ),
                is_authorized: false,
            },
            state,
        )
    }
    fn expired_pool(state: &FactoryState, factory_id: AccountId) -> AccountWithMetadata {
        let pool = Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 0,
            virtual_reserve0: 1_000,
            virtual_reserve1: 100,
            close_timestamp: Some(2),
            close_on_depletion: Some(pool::TokenSide::Token0),
        })
        .expect("valid pool");
        AccountWithMetadata {
            account: Account {
                program_owner: CURVE_PROGRAM_ID,
                data: Data::from(&PoolAccount {
                    allowed_input_token: None,
                    funded: true,
                    namespace: state.namespace,
                    token0_definition_id: state.token_definition_id,
                    token1_definition_id: state.collateral_definition_id,
                    owner: factory_id,
                    owner_program: None,
                    pool,
                }),
                ..Account::default()
            },
            account_id: state.pool_id,
            is_authorized: false,
        }
    }

    fn settlement_accounts(
        factory: &AccountWithMetadata,
        pool: &AccountWithMetadata,
        creator_account: &AccountWithMetadata,
        state: &FactoryState,
        timestamp: u64,
    ) -> WithdrawFactoryProceedsAccounts {
        WithdrawFactoryProceedsAccounts {
            factory: factory.clone(),
            pool: pool.clone(),
            creator: creator_account.clone(),
            token_definition: token_definition(state.token_definition_id),
            collateral_definition: token_definition(state.collateral_definition_id),
            factory_token_ata: AccountWithMetadata {
                account_id: ata_id(factory.account_id, state.token_definition_id),
                ..creator(1, false)
            },
            factory_collateral_ata: AccountWithMetadata {
                account_id: ata_id(factory.account_id, state.collateral_definition_id),
                ..creator(2, false)
            },
            pool_token_ata: AccountWithMetadata {
                account_id: ata_id(state.pool_id, state.token_definition_id),
                ..creator(3, false)
            },
            pool_collateral_ata: AccountWithMetadata {
                account_id: ata_id(state.pool_id, state.collateral_definition_id),
                ..creator(4, false)
            },
            creator_token_ata: AccountWithMetadata {
                account_id: ata_id(creator_account.account_id, state.token_definition_id),
                ..creator(5, false)
            },
            creator_collateral_ata: AccountWithMetadata {
                account_id: ata_id(creator_account.account_id, state.collateral_definition_id),
                ..creator(6, false)
            },
            clock: trusted_clock(timestamp),
            config: namespace_config(),
            treasury_ata: creator(8, false),
        }
    }

    #[test]
    fn creation_continuation_translates_saved_mode_into_neutral_pool_restriction() {
        for (mode, expected) in [(TradeMode::BuyOnly, Some(1)), (TradeMode::BuyAndSell, None)] {
            let creator = creator(9, true);
            let (mut factory, mut state) = delayed_factory(&creator);
            state.creation_stage = CreationStage::Allocated;
            state.trade_mode = mode;
            factory.account.program_owner = FACTORY_PROGRAM_ID;
            factory.account.data = Data::from(&state);
            let placeholder = |id| AccountWithMetadata {
                account_id: id,
                account: Account::default(),
                is_authorized: false,
            };
            let mut accounts = vec![placeholder(AccountId::new([20; 32])); 15];
            accounts[0] = factory;
            accounts[1] = placeholder(state.token_definition_id);
            accounts[2] = placeholder(compute_mint_pda(
                state.namespace,
                FACTORY_PROGRAM_ID,
                state.launch_salt,
            ));
            accounts[4] = placeholder(state.creator_escrow_id);
            accounts[5] = creator;
            accounts[7] = placeholder(state.collateral_definition_id);
            accounts[10] = placeholder(state.pool_id);
            let (posts, calls) =
                process_instruction(accounts, Instruction::ContinueCreation, FACTORY_PROGRAM_ID);
            assert_eq!(
                FactoryState::try_from(&posts[0].account().data)
                    .unwrap()
                    .trade_mode,
                mode
            );
            let instruction: CurveInstruction =
                risc0_zkvm::serde::from_slice(&calls[0].instruction_data).unwrap();
            let CurveInstruction::CreatePool {
                allowed_input_token,
                ..
            } = instruction
            else {
                panic!("pool creation required")
            };
            assert_eq!(allowed_input_token, expected);
        }
    }

    #[test]
    fn creator_rights_follow_the_nft_to_a_new_public_holder() {
        let original = creator(9, true);
        let (factory, state) = delayed_factory(&original);
        let mut next = original.clone();
        next.account_id = AccountId::new([77; 32]);
        next.account.data = Data::from(&TokenHolding::NftMaster {
            definition_id: original.account_id,
            print_balance: 1,
        });
        let escrow = AccountWithMetadata {
            account: Account {
                program_owner: TOKEN_PROGRAM_ID,
                data: Data::from(&TokenHolding::Fungible {
                    definition_id: state.token_definition_id,
                    balance: state.creator_allocation,
                }),
                ..Account::default()
            },
            account_id: state.creator_escrow_id,
            is_authorized: false,
        };
        let creator_holding = AccountWithMetadata {
            account: Account::default(),
            account_id: ata_id(next.account_id, state.token_definition_id),
            is_authorized: false,
        };

        let (posts, calls) = claim_creator_allocation(
            ClaimCreatorAllocationAccounts {
                factory: factory.clone(),
                pool: expired_pool(&state, factory.account_id),
                escrow,
                creator: next,
                token_definition: token_definition(state.token_definition_id),
                creator_holding,
                clock: trusted_clock(2),
            },
            ClaimCreatorAllocationParams {
                factory_program_id: FACTORY_PROGRAM_ID,
            },
        );

        assert_eq!(calls.len(), 2);
        let claimed =
            FactoryState::try_from(&posts[0].account().data).expect("factory state parses");
        assert!(claimed.creator_allocation_claimed);
    }

    #[test]
    #[should_panic(expected = "Pool must be closed before proceeds withdrawal")]
    fn settlement_rejects_unsold_sale_tokens_before_expiry() {
        let expected_creator = creator(9, true);
        let (mut factory, state) = delayed_factory(&expected_creator);
        factory.account.program_owner = FACTORY_PROGRAM_ID;
        let pool = expired_pool(&state, factory.account_id);

        let _ = withdraw_factory_proceeds(
            settlement_accounts(&factory, &pool, &expected_creator, &state, 1),
            WithdrawFactoryProceedsParams {
                factory_program_id: FACTORY_PROGRAM_ID,
            },
        );
    }

    #[test]
    fn namespace_scopes_all_factory_accounts_even_with_the_same_salt() {
        let a = AccountId::new([51; 32]);
        let b = AccountId::new([52; 32]);
        for derive in [
            compute_factory_pda,
            compute_definition_pda,
            compute_mint_pda,
            compute_metadata_pda,
            compute_escrow_pda,
        ] {
            assert_ne!(
                derive(a, FACTORY_PROGRAM_ID, [1; 32]),
                derive(b, FACTORY_PROGRAM_ID, [1; 32])
            );
        }
        assert_ne!(
            compute_creator_commitment(a, AccountId::new([3; 32]), [1; 32]),
            compute_creator_commitment(b, AccountId::new([3; 32]), [1; 32])
        );
    }

    #[test]
    fn instruction_tags_match_the_idl_order() {
        let tag = |instruction| {
            risc0_zkvm::serde::to_vec(&instruction)
                .expect("instruction serializes")
                .first()
                .copied()
        };
        assert_eq!(
            tag(Instruction::CreateFactoryPool {
                trade_mode: 1,
                graduation_hook_program_id: None,
                graduation_hook_configuration: vec![],
                namespace: AccountId::new([0xAD; 32]),
                launch_salt: [1; 32],
                name: "Launch".into(),
                uri: "https://example.test/launch.json".into(),
                sale_reserve: 800,
                dex_seed_reserve: 150,
                creator_allocation: 50,
                virtual_token_reserve: 2_000,
                virtual_collateral_reserve: 100,
                end_timestamp: None,
                curve_program_id: CURVE_PROGRAM_ID,
            }),
            Some(0)
        );
        assert_eq!(tag(Instruction::ContinueCreation), Some(1));
        assert_eq!(tag(Instruction::ClaimCreatorAllocation), Some(2));
        assert_eq!(tag(Instruction::WithdrawFactoryProceeds), Some(3));
        assert_eq!(
            tag(Instruction::Private {
                instruction: Box::new(Instruction::ContinueCreation),
            }),
            Some(5)
        );
    }

    #[test]
    fn fixed_supply_is_the_exact_three_way_split() {
        assert_eq!(total_supply(800, 150, 50), Ok(1_000));
    }
    #[test]
    fn launch_requires_tradeable_supply() {
        assert_eq!(total_supply(0, 1, 1), Err(CreateError::SaleReserveZero));
    }

    #[test]
    fn launch_requires_virtual_token_reserve_above_tradeable_supply() {
        assert_eq!(
            validate_curve_parameters(800, 800, 100),
            Err(CreateError::VirtualTokenReserveNotAboveSaleReserve)
        );
        assert_eq!(
            validate_curve_parameters(800, 799, 100),
            Err(CreateError::VirtualTokenReserveNotAboveSaleReserve)
        );
        assert_eq!(validate_curve_parameters(800, 801, 100), Ok(()));
    }

    #[test]
    fn launch_rejects_virtual_reserves_outside_the_curve_arithmetic_bound() {
        assert_eq!(
            validate_curve_parameters(800, 801, 0),
            Err(CreateError::VirtualReserveZero)
        );
        assert_eq!(
            validate_curve_parameters(800, pool::VIRTUAL_RESERVE_BOUND, 100),
            Err(CreateError::VirtualReserveAboveBound)
        );
        assert_eq!(
            validate_curve_parameters(800, 801, pool::VIRTUAL_RESERVE_BOUND),
            Err(CreateError::VirtualReserveAboveBound)
        );
    }
    #[test]
    fn overflow_cannot_create_an_unbacked_supply() {
        assert_eq!(
            total_supply(u128::MAX, 1, 0),
            Err(CreateError::SupplyOverflow)
        );
    }
    #[test]
    fn launch_salt_scopes_every_factory_address() {
        let id = [1; 8];
        assert_ne!(
            compute_factory_pda(AccountId::new([0xAD; 32]), id, [1; 32]),
            compute_factory_pda(AccountId::new([0xAD; 32]), id, [2; 32])
        );
        assert_ne!(
            compute_definition_pda(AccountId::new([0xAD; 32]), id, [1; 32]),
            compute_factory_pda(AccountId::new([0xAD; 32]), id, [1; 32])
        );
    }

    #[test]
    fn creator_commitment_is_scoped_to_creator_and_launch() {
        let creator_account = creator(9, true);
        assert_ne!(
            compute_creator_commitment(
                AccountId::new([0xAD; 32]),
                creator_account.account_id,
                [1; 32]
            ),
            compute_creator_commitment(
                AccountId::new([0xAD; 32]),
                creator_account.account_id,
                [2; 32]
            )
        );
        assert_ne!(
            compute_creator_commitment(
                AccountId::new([0xAD; 32]),
                creator_account.account_id,
                [1; 32]
            ),
            compute_creator_commitment(
                AccountId::new([0xAD; 32]),
                creator(8, true).account_id,
                [1; 32]
            )
        );
    }

    #[test]
    #[should_panic(expected = "Creator authorization is missing")]
    fn unlock_requires_the_creator_private_witness() {
        let expected_creator = creator(9, true);
        let (factory, state) = delayed_factory(&expected_creator);
        let _ = claim_creator_allocation(
            ClaimCreatorAllocationAccounts {
                factory,
                pool: AccountWithMetadata {
                    account_id: state.pool_id,
                    ..creator(1, false)
                },
                escrow: AccountWithMetadata {
                    account_id: state.creator_escrow_id,
                    ..creator(2, false)
                },
                creator: creator(9, false),
                token_definition: token_definition(state.token_definition_id),
                creator_holding: creator(3, false),
                clock: trusted_clock(1),
            },
            ClaimCreatorAllocationParams {
                factory_program_id: FACTORY_PROGRAM_ID,
            },
        );
    }

    #[test]
    #[should_panic(expected = "Creator commitment does not match launch")]
    fn copied_commitment_cannot_unlock_to_another_creator() {
        let expected_creator = creator(9, true);
        let (factory, state) = delayed_factory(&expected_creator);
        let _ = claim_creator_allocation(
            ClaimCreatorAllocationAccounts {
                factory,
                pool: AccountWithMetadata {
                    account_id: state.pool_id,
                    ..creator(1, false)
                },
                escrow: AccountWithMetadata {
                    account_id: state.creator_escrow_id,
                    ..creator(2, false)
                },
                creator: creator(8, true),
                token_definition: token_definition(state.token_definition_id),
                creator_holding: creator(3, false),
                clock: trusted_clock(1),
            },
            ClaimCreatorAllocationParams {
                factory_program_id: FACTORY_PROGRAM_ID,
            },
        );
    }

    #[test]
    #[should_panic(expected = "ATA account ID does not match expected derivation")]
    fn unlock_rejects_a_recipient_not_owned_by_the_creator() {
        let expected_creator = creator(9, true);
        let (factory, state) = delayed_factory(&expected_creator);
        let _ = claim_creator_allocation(
            ClaimCreatorAllocationAccounts {
                factory,
                pool: AccountWithMetadata {
                    account_id: state.pool_id,
                    ..creator(1, false)
                },
                escrow: AccountWithMetadata {
                    account_id: state.creator_escrow_id,
                    ..creator(2, false)
                },
                creator: expected_creator,
                token_definition: token_definition(state.token_definition_id),
                creator_holding: creator(3, false),
                clock: trusted_clock(1),
            },
            ClaimCreatorAllocationParams {
                factory_program_id: FACTORY_PROGRAM_ID,
            },
        );
    }

    #[test]
    fn zero_delayed_allocation_unlocks_without_an_escrow_holding() {
        let expected_creator = creator(9, true);
        let (mut factory, mut state) = delayed_factory(&expected_creator);
        state.creator_allocation = 0;
        state.total_supply = state.sale_reserve + state.dex_seed_reserve;
        factory.account.data = Data::from(&state);

        let mut closed_pool = Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 0,
            virtual_reserve0: 1_000,
            virtual_reserve1: 100,
            close_timestamp: None,
            close_on_depletion: None,
        })
        .expect("valid pool");
        assert_eq!(closed_pool.close_pool(0), Ok(()));
        let pool = AccountWithMetadata {
            account: Account {
                data: Data::from(&PoolAccount {
                    allowed_input_token: None,
                    funded: true,
                    namespace: AccountId::new([0xAD; 32]),
                    token0_definition_id: state.token_definition_id,
                    token1_definition_id: state.collateral_definition_id,
                    owner: factory.account_id,
                    owner_program: None,
                    pool: closed_pool,
                }),
                ..Account::default()
            },
            account_id: state.pool_id,
            is_authorized: false,
        };
        let creator_holding = AccountWithMetadata {
            account: Account::default(),
            account_id: ata_id(expected_creator.account_id, state.token_definition_id),
            is_authorized: false,
        };
        let escrow = AccountWithMetadata {
            account: Account::default(),
            account_id: state.creator_escrow_id,
            is_authorized: false,
        };

        let (post_states, calls) = claim_creator_allocation(
            ClaimCreatorAllocationAccounts {
                factory,
                pool,
                escrow,
                creator: expected_creator,
                token_definition: token_definition(state.token_definition_id),
                creator_holding,
                clock: trusted_clock(1),
            },
            ClaimCreatorAllocationParams {
                factory_program_id: FACTORY_PROGRAM_ID,
            },
        );

        assert!(calls.is_empty());
        let unlocked =
            FactoryState::try_from(&post_states[0].account().data).expect("factory state parses");
        assert!(unlocked.creator_allocation_claimed);
    }

    #[test]
    fn delayed_unlock_initializes_a_fresh_creator_ata_before_transfer() {
        let expected_creator = creator(9, true);
        let (factory, state) = delayed_factory(&expected_creator);
        let mut closed_pool = Pool::create(PoolCreateParams {
            token0_amount: 800,
            token1_amount: 0,
            virtual_reserve0: 1_000,
            virtual_reserve1: 100,
            close_timestamp: None,
            close_on_depletion: None,
        })
        .expect("valid pool");
        assert_eq!(closed_pool.close_pool(0), Ok(()));
        let pool = AccountWithMetadata {
            account: Account {
                data: Data::from(&PoolAccount {
                    allowed_input_token: None,
                    funded: true,
                    namespace: AccountId::new([0xAD; 32]),
                    token0_definition_id: state.token_definition_id,
                    token1_definition_id: state.collateral_definition_id,
                    owner: factory.account_id,
                    owner_program: None,
                    pool: closed_pool,
                }),
                ..Account::default()
            },
            account_id: state.pool_id,
            is_authorized: false,
        };
        let escrow = AccountWithMetadata {
            account: Account {
                program_owner: TOKEN_PROGRAM_ID,
                data: Data::from(&TokenHolding::Fungible {
                    definition_id: state.token_definition_id,
                    balance: state.creator_allocation,
                }),
                ..Account::default()
            },
            account_id: state.creator_escrow_id,
            is_authorized: false,
        };
        let creator_holding = AccountWithMetadata {
            account: Account::default(),
            account_id: ata_id(expected_creator.account_id, state.token_definition_id),
            is_authorized: false,
        };

        let (_post_states, calls) = claim_creator_allocation(
            ClaimCreatorAllocationAccounts {
                factory,
                pool,
                escrow,
                creator: expected_creator,
                token_definition: token_definition(state.token_definition_id),
                creator_holding,
                clock: trusted_clock(1),
            },
            ClaimCreatorAllocationParams {
                factory_program_id: FACTORY_PROGRAM_ID,
            },
        );

        assert_eq!(
            calls.len(),
            2,
            "a fresh creator ATA must be initialized before the escrow transfer"
        );
        assert_eq!(calls[0].program_id, ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID);
    }

    #[test]
    fn creation_dispatch_preserves_the_ordered_account_mapping() {
        let launch_salt = [3; 32];
        let factory_id =
            compute_factory_pda(AccountId::new([0xAD; 32]), FACTORY_PROGRAM_ID, launch_salt);
        let token_definition_id =
            compute_definition_pda(AccountId::new([0xAD; 32]), FACTORY_PROGRAM_ID, launch_salt);
        let collateral_definition_id = AccountId::new([4; 32]);
        let expected_creator = creator(9, true);
        let pool_id = compute_pool_pda(
            AccountId::new([0xAD; 32]),
            CURVE_PROGRAM_ID,
            token_definition_id,
            collateral_definition_id,
            factory_id,
        );
        let factory = AccountWithMetadata {
            account: Account::default(),
            account_id: factory_id,
            is_authorized: false,
        };
        let definition = AccountWithMetadata {
            account: Account {
                program_owner: TOKEN_PROGRAM_ID,
                ..Account::default()
            },
            account_id: token_definition_id,
            is_authorized: false,
        };
        let mint = AccountWithMetadata {
            account_id: compute_mint_pda(
                AccountId::new([0xAD; 32]),
                FACTORY_PROGRAM_ID,
                launch_salt,
            ),
            ..creator(1, false)
        };
        let metadata = AccountWithMetadata {
            account_id: compute_metadata_pda(
                AccountId::new([0xAD; 32]),
                FACTORY_PROGRAM_ID,
                launch_salt,
            ),
            ..creator(2, false)
        };
        let escrow = AccountWithMetadata {
            account_id: compute_escrow_pda(
                AccountId::new([0xAD; 32]),
                FACTORY_PROGRAM_ID,
                launch_salt,
            ),
            ..creator(3, false)
        };
        let creator_holding = AccountWithMetadata {
            account_id: ata_id(expected_creator.account_id, token_definition_id),
            ..creator(4, false)
        };
        let collateral_definition = AccountWithMetadata {
            account_id: collateral_definition_id,
            ..creator(5, false)
        };
        let factory_token_ata = AccountWithMetadata {
            account_id: ata_id(factory_id, token_definition_id),
            ..creator(6, false)
        };
        let factory_collateral_ata = AccountWithMetadata {
            account_id: ata_id(factory_id, collateral_definition_id),
            ..creator(7, false)
        };
        let pool = AccountWithMetadata {
            account_id: pool_id,
            ..creator(8, false)
        };
        let pool_token_ata = AccountWithMetadata {
            account_id: ata_id(pool_id, token_definition_id),
            ..creator(10, false)
        };
        let pool_collateral_ata = AccountWithMetadata {
            account_id: ata_id(pool_id, collateral_definition_id),
            ..creator(11, false)
        };

        let (post_states, calls) = create_factory_pool(
            CreateFactoryPoolAccounts {
                config: namespace_config(),
                factory: factory.clone(),
                token_definition: definition.clone(),
                mint_holding: mint.clone(),
                metadata: metadata.clone(),
                creator_escrow: escrow.clone(),
                creator: expected_creator.clone(),
                creator_holding: creator_holding.clone(),
                collateral_definition: collateral_definition.clone(),
                factory_token_ata: factory_token_ata.clone(),
                factory_collateral_ata: factory_collateral_ata.clone(),
                pool: pool.clone(),
                pool_token_ata: pool_token_ata.clone(),
                pool_collateral_ata: pool_collateral_ata.clone(),
                clock: trusted_clock(1),
            },
            CreateFactoryPoolParams {
                trade_mode: TradeMode::BuyOnly,
                graduation_hook_program_id: None,
                graduation_hook_configuration: vec![],
                namespace: AccountId::new([0xAD; 32]),
                launch_salt,
                name: "Launch".into(),
                uri: "https://example.test/launch.json".into(),
                sale_reserve: 800,
                dex_seed_reserve: 150,
                creator_allocation: 50,
                virtual_token_reserve: 2_000,
                virtual_collateral_reserve: 100,
                end_timestamp: None,
                factory_program_id: FACTORY_PROGRAM_ID,
                curve_program_id: CURVE_PROGRAM_ID,
            },
        );
        let expected_posts = post_states.clone();
        let expected_calls = calls.clone();
        let dispatch_pre = vec![
            factory,
            definition,
            mint,
            metadata,
            escrow,
            expected_creator,
            creator_holding,
            collateral_definition,
            factory_token_ata,
            factory_collateral_ata,
            pool,
            pool_token_ata,
            pool_collateral_ata,
            trusted_clock(1),
            namespace_config(),
        ];
        let (dispatch_posts, dispatch_calls) = process_instruction(
            dispatch_pre,
            Instruction::CreateFactoryPool {
                trade_mode: 0,
                graduation_hook_program_id: None,
                graduation_hook_configuration: vec![],
                namespace: AccountId::new([0xAD; 32]),
                launch_salt,
                name: "Launch".into(),
                uri: "https://example.test/launch.json".into(),
                sale_reserve: 800,
                dex_seed_reserve: 150,
                creator_allocation: 50,
                virtual_token_reserve: 2_000,
                virtual_collateral_reserve: 100,
                end_timestamp: None,
                curve_program_id: CURVE_PROGRAM_ID,
            },
            FACTORY_PROGRAM_ID,
        );
        assert_eq!(dispatch_posts.len(), expected_posts.len());
        for (dispatch_post, expected_post) in dispatch_posts.iter().zip(expected_posts.iter()) {
            assert_eq!(dispatch_post.account(), expected_post.account());
        }
        assert_eq!(dispatch_calls, expected_calls);

        assert_eq!(post_states.len(), 15);
        let state =
            FactoryState::try_from(&post_states[0].account().data).expect("factory state parses");
        assert_eq!(state.trade_mode, TradeMode::BuyOnly);
        assert_eq!(state.total_supply, 1_000);
        assert_eq!(state.sale_reserve, 800);
        assert_eq!(state.dex_seed_reserve, 150);
        assert_eq!(state.creator_allocation, 50);
        assert!(!state.creator_allocation_claimed);

        assert_eq!(state.creation_stage, CreationStage::Minted);
        let [definition_call]: [_; 1] = calls.try_into().expect("one mint call before allocation");
        let definition_instruction: token_core::Instruction =
            risc0_zkvm::serde::from_slice(&definition_call.instruction_data)
                .expect("token definition instruction parses");
        match definition_instruction {
            token_core::Instruction::NewDefinitionWithMetadata {
                new_definition: NewTokenDefinition::Fungible { name, total_supply },
                metadata,
            } => {
                assert_eq!(name, "Launch");
                assert_eq!(total_supply, 1_000);
                assert_eq!(metadata.uri, "https://example.test/launch.json");
            }
            _ => panic!("factory must create a fungible definition"),
        }
    }

    #[test]
    fn expiry_allows_the_current_creator_holder_to_start_settlement() {
        let original = creator(9, true);
        let (mut factory, state) = delayed_factory(&original);
        factory.account.program_owner = FACTORY_PROGRAM_ID;
        let mut current_creator = original.clone();
        current_creator.account_id = AccountId::new([77; 32]);
        current_creator.account.data = Data::from(&TokenHolding::NftMaster {
            definition_id: original.account_id,
            print_balance: 1,
        });
        let (posts, calls) = withdraw_factory_proceeds(
            WithdrawFactoryProceedsAccounts {
                pool: expired_pool(&state, factory.account_id),
                factory: factory.clone(),
                creator: current_creator.clone(),
                token_definition: token_definition(state.token_definition_id),
                collateral_definition: token_definition(state.collateral_definition_id),
                factory_token_ata: AccountWithMetadata {
                    account_id: ata_id(factory.account_id, state.token_definition_id),
                    ..creator(1, false)
                },
                factory_collateral_ata: AccountWithMetadata {
                    account_id: ata_id(factory.account_id, state.collateral_definition_id),
                    ..creator(2, false)
                },
                pool_token_ata: AccountWithMetadata {
                    account_id: ata_id(state.pool_id, state.token_definition_id),
                    ..creator(3, false)
                },
                pool_collateral_ata: AccountWithMetadata {
                    account_id: ata_id(state.pool_id, state.collateral_definition_id),
                    ..creator(4, false)
                },
                creator_token_ata: AccountWithMetadata {
                    account_id: ata_id(current_creator.account_id, state.token_definition_id),
                    ..creator(5, false)
                },
                creator_collateral_ata: AccountWithMetadata {
                    account_id: ata_id(current_creator.account_id, state.collateral_definition_id),
                    ..creator(6, false)
                },
                clock: trusted_clock(2),
                config: namespace_config(),
                treasury_ata: creator(8, false),
            },
            WithdrawFactoryProceedsParams {
                factory_program_id: FACTORY_PROGRAM_ID,
            },
        );

        assert_eq!(calls.len(), 1);
        assert_eq!(
            FactoryState::try_from(&posts[0].account().data)
                .expect("factory state parses")
                .settlement_stage,
            SettlementStage::Ready
        );
    }

    #[test]
    fn withdrawal_prepares_collateral_custody_before_moving_reserves() {
        let expected_creator = creator(9, true);
        let (mut factory, state) = delayed_factory(&expected_creator);
        factory.account.program_owner = FACTORY_PROGRAM_ID;
        let mut closed_pool = Pool::create(PoolCreateParams {
            token0_amount: 70,
            token1_amount: 30,
            virtual_reserve0: 1_000,
            virtual_reserve1: 100,
            close_timestamp: None,
            close_on_depletion: Some(pool::TokenSide::Token0),
        })
        .expect("valid pool");
        assert_eq!(closed_pool.close_pool(1), Ok(()));
        let pool = AccountWithMetadata {
            account: Account {
                program_owner: CURVE_PROGRAM_ID,
                data: Data::from(&PoolAccount {
                    allowed_input_token: None,
                    funded: true,
                    namespace: AccountId::new([0xAD; 32]),
                    token0_definition_id: state.token_definition_id,
                    token1_definition_id: state.collateral_definition_id,
                    owner: factory.account_id,
                    owner_program: None,
                    pool: closed_pool,
                }),
                ..Account::default()
            },
            account_id: state.pool_id,
            is_authorized: false,
        };
        let factory_token = AccountWithMetadata {
            account_id: ata_id(factory.account_id, state.token_definition_id),
            ..creator(1, false)
        };
        let factory_collateral = AccountWithMetadata {
            account_id: ata_id(factory.account_id, state.collateral_definition_id),
            ..creator(2, false)
        };
        let pool_token = AccountWithMetadata {
            account_id: ata_id(state.pool_id, state.token_definition_id),
            ..creator(3, false)
        };
        let pool_collateral = AccountWithMetadata {
            account_id: ata_id(state.pool_id, state.collateral_definition_id),
            ..creator(4, false)
        };
        let creator_token = AccountWithMetadata {
            account_id: ata_id(expected_creator.account_id, state.token_definition_id),
            ..creator(5, false)
        };
        let creator_collateral = AccountWithMetadata {
            account_id: ata_id(expected_creator.account_id, state.collateral_definition_id),
            ..creator(6, false)
        };
        let (_posts, calls) = withdraw_factory_proceeds(
            WithdrawFactoryProceedsAccounts {
                factory,
                pool,
                creator: expected_creator,
                token_definition: token_definition(state.token_definition_id),
                collateral_definition: token_definition(state.collateral_definition_id),
                factory_token_ata: factory_token,
                factory_collateral_ata: factory_collateral,
                pool_token_ata: pool_token,
                pool_collateral_ata: pool_collateral,
                creator_token_ata: creator_token,
                creator_collateral_ata: creator_collateral,
                clock: trusted_clock(1),
                config: creator(7, false),
                treasury_ata: creator(8, false),
            },
            WithdrawFactoryProceedsParams {
                factory_program_id: FACTORY_PROGRAM_ID,
            },
        );
        assert_eq!(
            calls.len(),
            1,
            "collateral ATA creation is a separate stage"
        );
        let instruction: associated_token_account_core::Instruction =
            risc0_zkvm::serde::from_slice(&calls[0].instruction_data).unwrap();
        assert!(matches!(
            instruction,
            associated_token_account_core::Instruction::Create { .. }
        ));
        assert_eq!(
            FactoryState::try_from(&_posts[0].account().data)
                .unwrap()
                .settlement_stage,
            SettlementStage::Ready
        );
    }

    #[test]
    fn expiry_settlement_burns_unsold_d_once_and_keeps_r_and_creator_allocation() {
        let expected_creator = creator(9, true);
        let (mut factory, state) = delayed_factory(&expected_creator);
        factory.account.program_owner = FACTORY_PROGRAM_ID;
        let pool = expired_pool(&state, factory.account_id);
        let params = WithdrawFactoryProceedsParams {
            factory_program_id: FACTORY_PROGRAM_ID,
        };

        let (posts, calls) = withdraw_factory_proceeds(
            settlement_accounts(&factory, &pool, &expected_creator, &state, 2),
            params,
        );
        assert_eq!(calls.len(), 1);
        factory.account = posts[0].account().clone();

        let (posts, calls) = withdraw_factory_proceeds(
            settlement_accounts(&factory, &pool, &expected_creator, &state, 2),
            WithdrawFactoryProceedsParams {
                factory_program_id: FACTORY_PROGRAM_ID,
            },
        );
        assert_eq!(calls.len(), 1);
        let withdrawn =
            FactoryState::try_from(&posts[0].account().data).expect("factory state parses");
        assert_eq!(
            withdrawn.settlement_stage,
            SettlementStage::Withdrawn {
                unsold: state.sale_reserve,
                collateral_due: 0,
            }
        );
        factory.account = posts[0].account().clone();

        let (posts, calls) = withdraw_factory_proceeds(
            settlement_accounts(&factory, &pool, &expected_creator, &state, 2),
            WithdrawFactoryProceedsParams {
                factory_program_id: FACTORY_PROGRAM_ID,
            },
        );
        let [burn]: [_; 1] = calls.try_into().expect("one unsold-D burn call");
        let burn_instruction: associated_token_account_core::Instruction =
            risc0_zkvm::serde::from_slice(&burn.instruction_data).expect("burn instruction parses");
        assert!(matches!(
            burn_instruction,
            associated_token_account_core::Instruction::Burn { amount: 800, .. }
        ));
        factory.account = posts[0].account().clone();

        let (posts, calls) = withdraw_factory_proceeds(
            settlement_accounts(&factory, &pool, &expected_creator, &state, 2),
            WithdrawFactoryProceedsParams {
                factory_program_id: FACTORY_PROGRAM_ID,
            },
        );
        let [_create, reserve_transfer]: [_; 2] = calls.try_into().expect("R payout calls");
        let reserve_instruction: associated_token_account_core::Instruction =
            risc0_zkvm::serde::from_slice(&reserve_transfer.instruction_data)
                .expect("reserve transfer instruction parses");
        assert!(matches!(
            reserve_instruction,
            associated_token_account_core::Instruction::Transfer { amount: 100, .. }
        ));
        factory.account = posts[0].account().clone();

        let (posts, calls) = withdraw_factory_proceeds(
            settlement_accounts(&factory, &pool, &expected_creator, &state, 2),
            WithdrawFactoryProceedsParams {
                factory_program_id: FACTORY_PROGRAM_ID,
            },
        );
        assert!(calls.is_empty());
        let complete =
            FactoryState::try_from(&posts[0].account().data).expect("factory state parses");
        assert_eq!(complete.settlement_stage, SettlementStage::Complete);
        assert_eq!(complete.creator_allocation, state.creator_allocation);
        assert!(!complete.creator_allocation_claimed);
        factory.account = posts[0].account().clone();

        let result = std::panic::catch_unwind(|| {
            withdraw_factory_proceeds(
                settlement_accounts(&factory, &pool, &expected_creator, &state, 2),
                WithdrawFactoryProceedsParams {
                    factory_program_id: FACTORY_PROGRAM_ID,
                },
            )
        });
        assert!(result.is_err());
    }

    #[test]
    fn factory_state_round_trips_without_private_authorization_material() {
        let state = FactoryState {
            trade_mode: TradeMode::BuyAndSell,
            graduation_hook: None,
            settlement_stage: SettlementStage::Unstarted,
            creation_stage: CreationStage::Active,
            end_timestamp: None,
            namespace: AccountId::new([0xAD; 32]),
            launch_salt: [1; 32],
            token_definition_id: AccountId::new([2; 32]),
            collateral_definition_id: AccountId::new([3; 32]),
            sale_reserve: 800,
            dex_seed_reserve: 100,
            creator_allocation: 100,
            total_supply: 1000,
            virtual_token_reserve: 2000,
            virtual_collateral_reserve: 100,
            curve_program_id: CURVE_PROGRAM_ID,
            creator_commitment: [4; 32],
            creator_escrow_id: AccountId::new([5; 32]),
            pool_id: AccountId::new([6; 32]),
            creator_allocation_claimed: false,
        };
        assert_eq!(
            FactoryState::try_from(&Data::from(&state)).expect("state parses"),
            state
        );
    }
}
