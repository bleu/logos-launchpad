//! The SDK. Hides wallet handling, message and witness construction, program ids, and
//! account derivation behind pool and factory lifecycle operations.
//!
//! The CLI parses arguments and calls this crate, and nothing else. The private deshield
//! to swap to re-shield flow also belongs here, because RFP-015 is explicit that the
//! program cannot enforce the re-shield and the SDK must.
//!
//! This crate contains the account and program loading shared by client lifecycle operations.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod error;
pub use error::{ClientError, Result};

pub use factory_core::CreationStage;

mod types;
pub use types::{
    AccountLocation, ExactInputTradeTerms, ExactOutputTradeTerms, FactorySale, PoolFeeTarget,
    PrivateAccountId,
};

mod lifecycle;
pub use lifecycle::{FactorySession, LifecycleReceipt, next_creation_invocation};

#[cfg(test)]
mod tests;

use std::{collections::HashMap, path::Path};

use associated_token_account_core::{compute_ata_seed, get_associated_token_account_id};
use common::{HashType, transaction::LeeTransaction};
use curve_core::{Config, Instruction as CurveInstruction, compute_config_pda, compute_pool_pda};
use factory_core::{
    ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID, FactoryState, Instruction as FactoryInstruction,
    compute_definition_pda, compute_escrow_pda, compute_factory_pda, compute_metadata_pda,
    compute_mint_pda,
};
use lee::{
    AccountId, PublicTransaction,
    privacy_preserving_transaction::circuit::ProgramWithDependencies,
    program::Program,
    public_transaction::{Message, WitnessSet},
};
use lee_core::program::ProgramId;
use sequencer_service_rpc::RpcClient as _;
use serde::Serialize;
use wallet::{AccountIdentity, WalletCore};

/// Parses a public or private account location. A bare account ID is public.
pub fn parse_account_id(raw: &str) -> Result<AccountId> {
    raw.parse::<AccountLocation>()
        .map(AccountLocation::account_id)
}

pub fn load_program(path: &Path) -> Result<Program> {
    let bytecode = std::fs::read(path).map_err(|source| ClientError::ReadProgram {
        path: path.to_owned(),
        source,
    })?;
    Program::new(bytecode.into()).map_err(|source| ClientError::InvalidProgram {
        path: path.to_owned(),
        source: Box::new(source),
    })
}

/// Inputs for the factory's one-time launch and pool creation operation.
#[derive(Debug, Clone)]
pub struct CreateSaleRequest {
    pub name: String,
    pub uri: String,
    pub sale_reserve: u128,
    pub dex_seed_reserve: u128,
    pub creator_allocation: u128,
    pub virtual_token_reserve: u128,
    pub virtual_collateral_reserve: u128,
    pub end_timestamp: Option<u64>,
}

/// Inputs for one RFP-015 private purchase composition.
#[derive(Debug, Clone, Copy)]
pub struct PrivateBuyRequest {
    pub amount_out: u128,
    pub max_collateral_in: u128,
    pub from_private: PrivateAccountId,
    pub to_private: PrivateAccountId,
    pub gas_reserve: u128,
}

/// Validates values the private router must not accept as no-op funding.
pub fn validate_private_buy_request(request: PrivateBuyRequest) -> Result<()> {
    if request.gas_reserve == 0 {
        return Err(ClientError::ZeroPrivateBuyGasReserve);
    }
    if request.amount_out == 0 || request.max_collateral_in == 0 {
        return Err(ClientError::ZeroPrivateBuyAmount);
    }

    Ok(())
}

/// Receipt for the single privacy-preserving transaction submitted by [`submit_private_buy`].
#[derive(Debug, Clone, Copy)]
pub struct PrivateBuyReceipt {
    pub transaction_hash: HashType,
    pub transient_public_account: AccountId,
    pub private_destination: AccountId,
}

/// Programs needed by the atomic private-buy route.
pub struct PrivateBuyRoute<'a> {
    pub private_buy_program: &'a Program,
    pub curve_program: &'a Program,
}

/// Submits one private transaction that funds a fresh public account with native gas and
/// collateral, buys exact launch-token output, then re-shields it to `to_private`.
///
/// The router guest is supplied separately because its image ID is deployment-specific. Its
/// dependencies are pinned LEZ token/ATA/native programs plus the supplied curve guest.
pub async fn submit_private_buy(
    wallet: &mut WalletCore,
    sale: FactorySale,
    route: PrivateBuyRoute<'_>,
    request: PrivateBuyRequest,
) -> Result<PrivateBuyReceipt> {
    validate_private_buy_request(request)?;
    if route.curve_program.id() != sale.curve_program_id {
        return Err(ClientError::PrivateBuyCurveProgramMismatch);
    }
    let (transient_public_account, _) = wallet.create_new_account_public(None);
    wallet
        .store_config_changes()
        .await
        .map_err(|source| ClientError::StoreTransientKey(source.into_boxed_dyn_error()))?;

    let addresses = factory_pool_addresses(sale);
    let ata_program = programs::ata();
    let token_program = programs::token();
    let native_transfer_program = programs::authenticated_transfer();
    let dependencies = HashMap::from([
        (route.curve_program.id(), route.curve_program.clone()),
        (ata_program.id(), ata_program.clone()),
        (token_program.id(), token_program.clone()),
        (
            native_transfer_program.id(),
            native_transfer_program.clone(),
        ),
    ]);
    let source = wallet
        .resolve_private_account(request.from_private.account_id())
        .ok_or(ClientError::PrivateCollateralSourceUnavailable)?;
    let destination = wallet
        .resolve_private_account(request.to_private.account_id())
        .ok_or(ClientError::PrivateTokenDestinationUnavailable)?;
    let accounts = vec![
        source,
        AccountIdentity::Public(transient_public_account),
        AccountIdentity::PublicNoSign(sale.collateral_definition),
        AccountIdentity::PublicNoSign(addresses.token_definition),
        AccountIdentity::PublicNoSign(associated_token_account(
            transient_public_account,
            sale.collateral_definition,
        )),
        AccountIdentity::PublicNoSign(associated_token_account(
            transient_public_account,
            addresses.token_definition,
        )),
        AccountIdentity::PublicNoSign(addresses.pool),
        AccountIdentity::PublicNoSign(compute_config_pda(sale.namespace, sale.curve_program_id)),
        AccountIdentity::PublicNoSign(associated_token_account(
            addresses.pool,
            sale.collateral_definition,
        )),
        AccountIdentity::PublicNoSign(associated_token_account(
            addresses.pool,
            addresses.token_definition,
        )),
        AccountIdentity::PublicNoSign(clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID),
        destination,
    ];
    let instruction = private_flow_core::PrivateBuyInstruction::Execute {
        curve_program_id: sale.curve_program_id,
        token_program_id: token_program.id(),
        ata_program_id: ata_program.id(),
        native_transfer_program_id: native_transfer_program.id(),
        amount_out: request.amount_out,
        max_collateral_in: request.max_collateral_in,
        gas_reserve: request.gas_reserve,
        collateral_definition_id: sale.collateral_definition,
    };
    let instruction_data = Program::serialize_instruction(instruction)
        .map_err(|source| ClientError::EncodePrivateBuy(Box::new(source)))?;
    let (transaction_hash, _) = wallet
        .send_privacy_preserving_tx(
            accounts,
            instruction_data,
            &ProgramWithDependencies::new(route.private_buy_program.clone(), dependencies),
        )
        .await
        .map_err(|source| ClientError::SubmitPrivateBuy(Box::new(source)))?;
    Ok(PrivateBuyReceipt {
        transaction_hash,
        transient_public_account,
        private_destination: request.to_private.account_id(),
    })
}

/// Builds the curve configuration initialization/update call.
#[must_use]
pub fn build_update_config_invocation(
    namespace: AccountId,
    curve_program_id: ProgramId,
    authority: AccountId,
    admin: AccountId,
    protocol_fee_bps: u16,
    treasury: AccountId,
) -> PublicInvocation<CurveInstruction> {
    PublicInvocation {
        program_id: curve_program_id,
        account_ids: vec![compute_config_pda(namespace, curve_program_id), authority],
        signer_accounts: vec![authority],
        instruction: CurveInstruction::UpdateConfig {
            namespace,
            admin,
            protocol_fee_bps: protocol_fee_bps.into(),
            treasury,
        },
    }
}

/// Quotes an exact-output purchase against a snapshot of the current pool and fee config.
/// The quote is informational only; callers enforce its cap through `BuyRequest` on-chain.
pub fn quote_buy(
    pool_account: &curve_core::PoolAccount,
    config: &Config,
    amount_out: u128,
    now: u64,
) -> Result<pool::SwapOutcome> {
    if !pool_account.funded {
        return Err(ClientError::PoolFundingPending);
    }
    let mut pool = pool_account.pool.clone();
    pool.swap_exact_output(pool::ExactOutputSwap {
        token_in: pool::TokenSide::Token1,
        amount_out,
        max_amount_in: u128::MAX,
        pool_fee_bps: 0,
        protocol_fee_bps: config.protocol_fee_bps,
        now,
    })
    .map_err(ClientError::QuoteBuy)
}

/// Quotes an exact-input sale against a snapshot of the current pool and fee config.
/// The quote is informational only; callers enforce its floor through `SellRequest` on-chain.
pub fn quote_sell(
    pool_account: &curve_core::PoolAccount,
    config: &Config,
    amount_in: u128,
    now: u64,
) -> Result<pool::SwapOutcome> {
    if !pool_account.funded {
        return Err(ClientError::PoolFundingPending);
    }
    let mut pool = pool_account.pool.clone();
    pool.swap_exact_input(pool::ExactInputSwap {
        token_in: pool::TokenSide::Token0,
        amount_in,
        min_amount_out: 0,
        pool_fee_bps: 0,
        protocol_fee_bps: config.protocol_fee_bps,
        now,
    })
    .map_err(ClientError::QuoteSell)
}

/// Quotes an exact-input purchase spending collateral against a pool snapshot.
pub fn quote_buy_with_collateral(
    pool_account: &curve_core::PoolAccount,
    config: &Config,
    collateral_in: u128,
    now: u64,
) -> Result<pool::SwapOutcome> {
    if !pool_account.funded {
        return Err(ClientError::PoolFundingPending);
    }
    let mut pool = pool_account.pool.clone();
    pool.swap_exact_input(pool::ExactInputSwap {
        token_in: pool::TokenSide::Token1,
        amount_in: collateral_in,
        min_amount_out: 0,
        pool_fee_bps: 0,
        protocol_fee_bps: config.protocol_fee_bps,
        now,
    })
    .map_err(ClientError::QuoteBuyWithCollateral)
}

/// A public program call with all account addresses and required signatures resolved.
#[derive(Debug, Clone)]
pub struct PublicInvocation<I> {
    pub program_id: ProgramId,
    pub account_ids: Vec<AccountId>,
    pub signer_accounts: Vec<AccountId>,
    pub instruction: I,
}

/// Builds the complete factory call for a launch. All PDAs and ATAs are derived locally;
/// only the creator account is caller-provided.
pub fn build_create_sale_invocation(
    sale: FactorySale,
    creator: AccountId,
    request: CreateSaleRequest,
) -> Result<PublicInvocation<FactoryInstruction>> {
    factory_core::validate_curve_parameters(
        request.sale_reserve,
        request.virtual_token_reserve,
        request.virtual_collateral_reserve,
    )?;
    let addresses = factory_pool_addresses(sale);
    let account_ids = vec![
        addresses.factory,
        addresses.token_definition,
        compute_mint_pda(sale.namespace, sale.factory_program_id, sale.launch_salt),
        compute_metadata_pda(sale.namespace, sale.factory_program_id, sale.launch_salt),
        compute_escrow_pda(sale.namespace, sale.factory_program_id, sale.launch_salt),
        creator,
        associated_token_account(creator, addresses.token_definition),
        sale.collateral_definition,
        associated_token_account(addresses.factory, addresses.token_definition),
        associated_token_account(addresses.factory, sale.collateral_definition),
        addresses.pool,
        associated_token_account(addresses.pool, addresses.token_definition),
        associated_token_account(addresses.pool, sale.collateral_definition),
        clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
        compute_config_pda(sale.namespace, sale.curve_program_id),
    ];
    Ok(PublicInvocation {
        program_id: sale.factory_program_id,
        account_ids,
        signer_accounts: vec![creator],
        instruction: FactoryInstruction::CreateFactoryPool {
            namespace: sale.namespace,
            launch_salt: sale.launch_salt,
            name: request.name,
            uri: request.uri,
            sale_reserve: request.sale_reserve,
            dex_seed_reserve: request.dex_seed_reserve,
            creator_allocation: request.creator_allocation,
            virtual_token_reserve: request.virtual_token_reserve,
            virtual_collateral_reserve: request.virtual_collateral_reserve,
            end_timestamp: request.end_timestamp,
            curve_program_id: sale.curve_program_id,
        },
    })
}

/// Builds the creator-authorized factory withdrawal that runs the factory settlement policy.
#[must_use]
pub fn build_withdraw_factory_proceeds_invocation(
    sale: FactorySale,
    creator: AccountId,
    treasury: AccountId,
) -> PublicInvocation<FactoryInstruction> {
    let addresses = factory_pool_addresses(sale);
    let mut invocation = PublicInvocation {
        program_id: sale.factory_program_id,
        account_ids: vec![
            addresses.factory,
            addresses.pool,
            creator,
            addresses.token_definition,
            sale.collateral_definition,
            associated_token_account(addresses.factory, addresses.token_definition),
            associated_token_account(addresses.factory, sale.collateral_definition),
            associated_token_account(addresses.pool, addresses.token_definition),
            associated_token_account(addresses.pool, sale.collateral_definition),
            associated_token_account(creator, addresses.token_definition),
            associated_token_account(creator, sale.collateral_definition),
            clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
            compute_config_pda(sale.namespace, sale.curve_program_id),
            associated_token_account(treasury, sale.collateral_definition),
        ],
        signer_accounts: vec![creator],
        instruction: FactoryInstruction::WithdrawFactoryProceeds,
    };
    let last = invocation.account_ids.len() - 1;
    if invocation.account_ids[..last].contains(&invocation.account_ids[last]) {
        invocation.account_ids.pop();
    }
    invocation
}

/// Builds the creator-authorized release of an `OnClose` allocation from factory escrow.
#[must_use]
pub fn build_claim_creator_allocation_invocation(
    sale: FactorySale,
    creator: AccountId,
) -> PublicInvocation<FactoryInstruction> {
    let addresses = factory_pool_addresses(sale);
    PublicInvocation {
        program_id: sale.factory_program_id,
        account_ids: vec![
            addresses.factory,
            addresses.pool,
            compute_escrow_pda(sale.namespace, sale.factory_program_id, sale.launch_salt),
            creator,
            addresses.token_definition,
            associated_token_account(creator, addresses.token_definition),
            clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
        ],
        signer_accounts: vec![creator],
        instruction: FactoryInstruction::ClaimCreatorAllocation,
    }
}

/// Builds a factory-launch purchase as a neutral curve exact-output swap.
#[must_use]
pub fn build_buy_invocation(
    sale: FactorySale,
    participant: AccountId,
    terms: ExactOutputTradeTerms,
) -> PublicInvocation<CurveInstruction> {
    let addresses = factory_pool_addresses(sale);
    PublicInvocation {
        program_id: sale.curve_program_id,
        account_ids: vec![
            addresses.pool,
            compute_config_pda(sale.namespace, sale.curve_program_id),
            participant,
            associated_token_account(participant, sale.collateral_definition),
            associated_token_account(addresses.pool, sale.collateral_definition),
            associated_token_account(addresses.pool, addresses.token_definition),
            associated_token_account(participant, addresses.token_definition),
            clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
        ],
        signer_accounts: vec![participant],
        instruction: CurveInstruction::SwapExactOutput {
            amount_out: terms.amount_out,
            max_amount_in: terms.max_amount_in,
            token_in: sale.collateral_definition,
        },
    }
}

/// Builds a factory-launch purchase as a neutral curve exact-input swap.
///
/// This is the RFP's primary buy form: the participant supplies collateral and
/// protects the resulting launch-token amount with a floor.
#[must_use]
pub fn build_buy_with_collateral_invocation(
    sale: FactorySale,
    participant: AccountId,
    terms: ExactInputTradeTerms,
) -> PublicInvocation<CurveInstruction> {
    let addresses = factory_pool_addresses(sale);
    PublicInvocation {
        program_id: sale.curve_program_id,
        account_ids: vec![
            addresses.pool,
            compute_config_pda(sale.namespace, sale.curve_program_id),
            participant,
            associated_token_account(participant, sale.collateral_definition),
            associated_token_account(addresses.pool, sale.collateral_definition),
            associated_token_account(addresses.pool, addresses.token_definition),
            associated_token_account(participant, addresses.token_definition),
            clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
        ],
        signer_accounts: vec![participant],
        instruction: CurveInstruction::SwapExactInput {
            amount_in: terms.amount_in,
            min_amount_out: terms.min_amount_out,
            token_in: sale.collateral_definition,
        },
    }
}

/// Builds a factory-launch sale as a neutral curve exact-input swap.
#[must_use]
pub fn build_sell_invocation(
    sale: FactorySale,
    participant: AccountId,
    terms: ExactInputTradeTerms,
) -> PublicInvocation<CurveInstruction> {
    let addresses = factory_pool_addresses(sale);
    PublicInvocation {
        program_id: sale.curve_program_id,
        account_ids: vec![
            addresses.pool,
            compute_config_pda(sale.namespace, sale.curve_program_id),
            participant,
            associated_token_account(participant, addresses.token_definition),
            associated_token_account(addresses.pool, addresses.token_definition),
            associated_token_account(addresses.pool, sale.collateral_definition),
            associated_token_account(participant, sale.collateral_definition),
            clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
        ],
        signer_accounts: vec![participant],
        instruction: CurveInstruction::SwapExactInput {
            amount_in: terms.amount_in,
            min_amount_out: terms.min_amount_out,
            token_in: addresses.token_definition,
        },
    }
}

/// Reads the live curve configuration so callers direct protocol fees to its configured treasury.
pub async fn load_curve_config(
    namespace: AccountId,
    wallet: &WalletCore,
    curve_program_id: ProgramId,
) -> Result<Config> {
    let config_id = compute_config_pda(namespace, curve_program_id);
    let account = wallet
        .get_account_public(config_id)
        .await
        .map_err(|source| ClientError::ReadCurveConfig(source.into_boxed_dyn_error()))?;
    Config::try_from(&account.data).map_err(ClientError::DecodeCurveConfig)
}

/// Reads the immutable launch policy and current factory lifecycle flags for a sale.
pub async fn load_factory_state(wallet: &WalletCore, sale: FactorySale) -> Result<FactoryState> {
    let factory_id = factory_pool_addresses(sale).factory;
    let account = wallet
        .get_account_public(factory_id)
        .await
        .map_err(|source| ClientError::ReadFactoryState(source.into_boxed_dyn_error()))?;
    if account.program_owner != sale.factory_program_id {
        return Err(ClientError::FactoryProgramMismatch);
    }
    let state = FactoryState::try_from(&account.data).map_err(ClientError::DecodeFactoryState)?;
    validate_factory_state(sale, &state)?;
    Ok(state)
}

/// Reads the current neutral curve reserves for a factory sale.
pub async fn load_factory_pool(
    wallet: &WalletCore,
    sale: FactorySale,
) -> Result<curve_core::PoolAccount> {
    let addresses = factory_pool_addresses(sale);
    let account = wallet
        .get_account_public(addresses.pool)
        .await
        .map_err(|source| ClientError::ReadFactoryPool(source.into_boxed_dyn_error()))?;
    if account.program_owner != sale.curve_program_id {
        return Err(ClientError::PoolProgramMismatch);
    }
    let pool =
        curve_core::PoolAccount::try_from(&account.data).map_err(ClientError::DecodeFactoryPool)?;
    let expected_owner_program = (
        sale.factory_program_id,
        *factory_core::compute_factory_seed(sale.namespace, sale.launch_salt).as_bytes(),
    );
    if !(pool.namespace == sale.namespace
        && pool.token0_definition_id == addresses.token_definition
        && pool.token1_definition_id == sale.collateral_definition
        && pool.owner == addresses.factory
        && pool.owner_program == Some(expected_owner_program))
    {
        return Err(ClientError::FactorySaleMismatch);
    }
    Ok(pool)
}

/// Signs and submits a public invocation through the configured project wallet.
///
/// The caller supplies only application-level accounts; this boundary obtains each nonce,
/// resolves the corresponding wallet key, and constructs the LEZ public transaction.
pub async fn submit_public_invocation<I: Serialize>(
    wallet: &WalletCore,
    program: &Program,
    invocation: PublicInvocation<I>,
) -> Result<HashType> {
    if program.id() != invocation.program_id {
        return Err(ClientError::InvocationProgramMismatch);
    }
    let signing_keys = invocation
        .signer_accounts
        .iter()
        .map(|account_id| {
            wallet
                .get_account_public_signing_key(*account_id)
                .ok_or(ClientError::PublicSigningKeyUnavailable(*account_id))
        })
        .collect::<Result<Vec<_>>>()?;
    let nonces = wallet
        .get_accounts_nonces(invocation.signer_accounts)
        .await
        .map_err(|source| ClientError::ReadSignerNonces(source.into_boxed_dyn_error()))?;
    let message = Message::try_new(
        invocation.program_id,
        invocation.account_ids,
        nonces,
        invocation.instruction,
    )?;
    let witnesses = WitnessSet::for_message(&message, &signing_keys);
    let hash = wallet
        .sequencer_client
        .send_transaction(LeeTransaction::Public(PublicTransaction::new(
            message, witnesses,
        )))
        .await
        .map_err(ClientError::SubmitPublicTransaction)?;
    wait_for_confirmation(wallet, hash).await?;
    Ok(hash)
}

#[derive(Debug, Clone, Copy)]
struct FactoryPoolAddresses {
    factory: AccountId,
    token_definition: AccountId,
    creator_escrow: AccountId,
    pool: AccountId,
}

fn factory_pool_addresses(sale: FactorySale) -> FactoryPoolAddresses {
    let factory = compute_factory_pda(sale.namespace, sale.factory_program_id, sale.launch_salt);
    let token_definition =
        compute_definition_pda(sale.namespace, sale.factory_program_id, sale.launch_salt);
    let creator_escrow =
        compute_escrow_pda(sale.namespace, sale.factory_program_id, sale.launch_salt);
    let pool = compute_pool_pda(
        sale.namespace,
        sale.curve_program_id,
        token_definition,
        sale.collateral_definition,
        factory,
    );
    FactoryPoolAddresses {
        factory,
        token_definition,
        creator_escrow,
        pool,
    }
}

pub(crate) fn validate_factory_state(sale: FactorySale, state: &FactoryState) -> Result<()> {
    let addresses = factory_pool_addresses(sale);
    if state.namespace != sale.namespace
        || state.launch_salt != sale.launch_salt
        || state.curve_program_id != sale.curve_program_id
        || state.collateral_definition_id != sale.collateral_definition
        || state.token_definition_id != addresses.token_definition
        || state.creator_escrow_id != addresses.creator_escrow
        || state.pool_id != addresses.pool
    {
        return Err(ClientError::FactorySaleMismatch);
    }
    Ok(())
}

fn associated_token_account(owner: AccountId, token_definition: AccountId) -> AccountId {
    get_associated_token_account_id(
        &ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
        &compute_ata_seed(owner, token_definition),
    )
}

/// Permanently renounces namespace administration without changing the trading settings.
pub fn build_renounce_admin_invocation(
    namespace: AccountId,
    curve_program_id: ProgramId,
    admin: AccountId,
) -> PublicInvocation<CurveInstruction> {
    PublicInvocation {
        program_id: curve_program_id,
        account_ids: vec![compute_config_pda(namespace, curve_program_id), admin],
        signer_accounts: vec![admin],
        instruction: CurveInstruction::RenounceAdmin { namespace },
    }
}
/// Issues a single, unprintable NFT master as a stable authority identity.
pub fn build_create_authority_invocation(
    definition: AccountId,
    holder: AccountId,
    metadata: AccountId,
    name: String,
    uri: String,
) -> PublicInvocation<token_core::Instruction> {
    PublicInvocation {
        program_id: curve_core::TOKEN_PROGRAM_ID,
        account_ids: vec![definition, holder, metadata],
        signer_accounts: vec![definition, holder, metadata],
        instruction: token_core::Instruction::NewDefinitionWithMetadata {
            new_definition: token_core::NewTokenDefinition::NonFungible {
                name,
                printable_supply: 1,
            },
            metadata: Box::new(token_core::NewTokenMetadata {
                standard: token_core::MetadataStandard::Simple,
                uri,
                creators: String::new(),
            }),
        },
    }
}

/// Moves the entire authority. Both accounts sign so a fresh recipient can be claimed.
pub fn build_transfer_authority_invocation(
    holder: AccountId,
    recipient: AccountId,
) -> PublicInvocation<token_core::Instruction> {
    PublicInvocation {
        program_id: curve_core::TOKEN_PROGRAM_ID,
        account_ids: vec![holder, recipient],
        signer_accounts: vec![holder, recipient],
        instruction: token_core::Instruction::Transfer {
            amount_to_transfer: 1,
        },
    }
}

/// Selects the guest's metadata convention for the pinned privacy execution runtime.
pub trait PrivateInstruction: Serialize {
    fn for_private_execution(&self) -> Self;
}
impl PrivateInstruction for CurveInstruction {
    fn for_private_execution(&self) -> Self {
        match self {
            Self::Private { .. } => self.clone(),
            _ => Self::Private {
                instruction: Box::new(self.clone()),
            },
        }
    }
}
impl PrivateInstruction for FactoryInstruction {
    fn for_private_execution(&self) -> Self {
        match self {
            Self::Private { .. } => self.clone(),
            _ => Self::Private {
                instruction: Box::new(self.clone()),
            },
        }
    }
}

/// Encodes an app action with exactly one NFT-holder signer for the private router.
pub fn build_private_authority_instruction<I: PrivateInstruction>(
    action: &PublicInvocation<I>,
    holder: AccountId,
) -> Result<private_flow_core::PrivateAuthorityInstruction> {
    if action.signer_accounts != vec![holder] {
        return Err(ClientError::InvalidAuthoritySigners);
    }
    let authority_index = action
        .account_ids
        .iter()
        .position(|id| *id == holder)
        .ok_or(ClientError::AuthorityHolderMissing)?;
    if action
        .account_ids
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len()
        != action.account_ids.len()
    {
        return Err(ClientError::DuplicateActionAccounts);
    }
    Ok(private_flow_core::PrivateAuthorityInstruction {
        program_id: action.program_id,
        instruction_data: Program::serialize_instruction(
            action.instruction.for_private_execution(),
        )?,
        authority_index,
    })
}

/// Executes an app role operation with a fresh public NFT holder, returning the NFT to source.
/// `build` constructs the action and its payout ATAs for that fresh holder.
pub async fn submit_private_authority<I: PrivateInstruction>(
    wallet: &mut WalletCore,
    router: &Program,
    action_program: &Program,
    extra_dependencies: Vec<Program>,
    source: AccountId,
    build: impl FnOnce(AccountId) -> Result<PublicInvocation<I>>,
) -> Result<PrivateBuyReceipt> {
    let source_identity = wallet
        .resolve_private_account(source)
        .ok_or(ClientError::PrivateAuthoritySourceUnavailable)?;
    let (holder, _) = wallet.create_new_account_public(None);
    wallet
        .store_config_changes()
        .await
        .map_err(|source| ClientError::StoreAuthorityKey(source.into_boxed_dyn_error()))?;
    let action = build(holder)?;
    if action.program_id != action_program.id() {
        return Err(ClientError::AuthorityProgramMismatch);
    }
    if action.account_ids.contains(&source) {
        return Err(ClientError::AuthoritySourceIsActionAccount);
    }
    let instruction = build_private_authority_instruction(&action, holder)?;
    let mut identities = vec![source_identity];
    identities.extend(action.account_ids.iter().map(|id| {
        if *id == holder {
            AccountIdentity::Public(*id)
        } else {
            AccountIdentity::PublicNoSign(*id)
        }
    }));
    let mut dependencies: HashMap<_, _> = extra_dependencies
        .into_iter()
        .map(|program| (program.id(), program))
        .collect();
    for program in [action_program.clone(), programs::token(), programs::ata()] {
        dependencies.insert(program.id(), program);
    }
    let (transaction_hash, secrets) = wallet
        .send_privacy_preserving_tx(
            identities,
            Program::serialize_instruction(instruction)?,
            &ProgramWithDependencies::new(router.clone(), dependencies),
        )
        .await
        .map_err(|source| ClientError::SubmitPrivateAuthority(Box::new(source)))?;
    confirm_private_accounts(wallet, transaction_hash, &[source], &secrets).await?;
    Ok(PrivateBuyReceipt {
        transaction_hash,
        transient_public_account: holder,
        private_destination: source,
    })
}

/// Reads the stable NFT identity for a public or private holder location.
pub async fn load_authority_identity(
    wallet: &WalletCore,
    location: AccountLocation,
) -> Result<AccountId> {
    let holder = location.account_id();
    let account = match location {
        AccountLocation::Private(_) => wallet
            .get_account_private(holder)
            .ok_or(ClientError::PrivateAuthorityHoldingUnavailable)?,
        AccountLocation::Public(_) => wallet
            .get_account_public(holder)
            .await
            .map_err(|source| ClientError::ReadAuthorityHolding(source.into_boxed_dyn_error()))?,
    };
    if account.program_owner != curve_core::TOKEN_PROGRAM_ID {
        return Err(ClientError::AuthorityHoldingProgramMismatch);
    }
    match token_core::TokenHolding::try_from(&account.data)
        .map_err(ClientError::DecodeAuthorityHolding)?
    {
        token_core::TokenHolding::NftMaster {
            definition_id,
            print_balance: 1,
        } => Ok(definition_id),
        _ => Err(ClientError::InvalidAuthorityHolding),
    }
}

/// Transfers authority across public/private state using the pinned token guest.
pub async fn transfer_authority(
    wallet: &mut WalletCore,
    source: AccountLocation,
    destination: AccountLocation,
) -> Result<HashType> {
    let from = source.account_id();
    let to = destination.account_id();
    if from == to {
        return Err(ClientError::SameAuthoritySourceAndDestination);
    }
    load_authority_identity(wallet, source).await?;
    let identity = |location: AccountLocation| -> Result<AccountIdentity> {
        match location {
            AccountLocation::Private(account_id) => wallet
                .resolve_private_account(account_id)
                .ok_or(ClientError::PrivateHoldingUnavailable),
            AccountLocation::Public(account_id) => Ok(AccountIdentity::Public(account_id)),
        }
    };
    if source.is_private() || destination.is_private() {
        let accounts = vec![identity(source)?, identity(destination)?];
        let (hash, secrets) = wallet
            .send_privacy_preserving_tx(
                accounts,
                Program::serialize_instruction(token_core::Instruction::Transfer {
                    amount_to_transfer: 1,
                })?,
                &programs::token().into(),
            )
            .await
            .map_err(|source| ClientError::TransferPrivateAuthority(Box::new(source)))?;
        let private_ids: Vec<_> = [source, destination]
            .into_iter()
            .filter_map(|location| match location {
                AccountLocation::Private(account_id) => Some(account_id),
                AccountLocation::Public(_) => None,
            })
            .collect();
        confirm_private_accounts(wallet, hash, &private_ids, &secrets).await?;
        Ok(hash)
    } else {
        submit_public_invocation(
            wallet,
            &programs::token(),
            build_transfer_authority_invocation(from, to),
        )
        .await
    }
}

/// A stage is complete only after inclusion, not after mempool submission.
async fn wait_for_confirmation(wallet: &WalletCore, hash: HashType) -> Result<LeeTransaction> {
    tokio::time::timeout(
        std::time::Duration::from_secs(60),
        wallet.poll_native_token_transfer(hash),
    )
    .await
    .map_err(|source| ClientError::ConfirmationTimeout { hash, source })?
    .map_err(|source| ClientError::ConfirmTransaction(source.into_boxed_dyn_error()))
}

async fn confirm_private_accounts(
    wallet: &mut WalletCore,
    hash: HashType,
    ids: &[AccountId],
    secrets: &[lee_core::SharedSecretKey],
) -> Result<()> {
    let LeeTransaction::PrivacyPreserving(tx) = wait_for_confirmation(wallet, hash).await? else {
        return Err(ClientError::ExpectedPrivateTransaction);
    };
    if !(ids.len() == secrets.len() && ids.len() == tx.message.encrypted_private_post_states.len())
    {
        return Err(ClientError::PrivateResultCountMismatch);
    }
    for (index, (id, secret)) in ids.iter().zip(secrets).enumerate() {
        let (kind, account) = lee_core::EncryptionScheme::decrypt(
            &tx.message.encrypted_private_post_states[index].ciphertext,
            secret,
            &tx.message.new_commitments[index],
            u32::try_from(index)?,
        )
        .ok_or(ClientError::DecryptPrivateAccount)?;
        wallet
            .storage_mut()
            .key_chain_mut()
            .insert_private_account(*id, kind, account)
            .map_err(|source| ClientError::UpdatePrivateAccount {
                account: *id,
                source: source.into_boxed_dyn_error(),
            })?;
    }
    wallet
        .store_persistent_data()
        .map_err(|source| ClientError::StorePrivateState(source.into_boxed_dyn_error()))
}

/// Builds permissionless collection for a pool in the selected namespace.
#[must_use]
pub fn build_collect_fees_invocation(
    namespace: AccountId,
    curve_program_id: ProgramId,
    pool: AccountId,
    collateral: AccountId,
    treasury: AccountId,
) -> PublicInvocation<CurveInstruction> {
    PublicInvocation {
        program_id: curve_program_id,
        account_ids: vec![
            pool,
            compute_config_pda(namespace, curve_program_id),
            associated_token_account(pool, collateral),
            associated_token_account(treasury, collateral),
        ],
        signer_accounts: vec![],
        instruction: CurveInstruction::CollectFees,
    }
}

/// Initializes the live treasury ATA before collection. No treasury signature is needed.
pub async fn prepare_fee_collection(
    wallet: &WalletCore,
    namespace: AccountId,
    curve: ProgramId,
    collateral: AccountId,
) -> Result<AccountId> {
    let config = load_curve_config(namespace, wallet, curve).await?;
    let ata = associated_token_account(config.treasury, collateral);
    if wallet
        .get_account_public(ata)
        .await
        .map_err(|source| ClientError::ReadTreasuryAccount(source.into_boxed_dyn_error()))?
        == lee_core::account::Account::default()
    {
        let program = programs::ata();
        submit_public_invocation(
            wallet,
            &program,
            PublicInvocation {
                program_id: program.id(),
                account_ids: vec![config.treasury, collateral, ata],
                signer_accounts: vec![],
                instruction: associated_token_account_core::Instruction::Create {
                    ata_program_id: program.id(),
                },
            },
        )
        .await
        .map_err(|source| ClientError::InitializeTreasuryAccount(Box::new(source)))?;
    }
    Ok(config.treasury)
}

/// Collects the supplied pools sequentially, rejecting any foreign namespace or mint.
/// Each collection is independently atomic and safe to retry; this is not an atomic batch.
pub async fn collect_pool_fees(
    wallet: &WalletCore,
    namespace: AccountId,
    curve: &Program,
    targets: &[PoolFeeTarget],
) -> Result<Vec<HashType>> {
    let mut hashes = Vec::new();
    for target in targets {
        let account = wallet
            .get_account_public(target.pool)
            .await
            .map_err(|source| ClientError::ReadCollectionPool(source.into_boxed_dyn_error()))?;
        if account.program_owner != curve.id() {
            return Err(ClientError::PoolProgramMismatch);
        }
        let state = curve_core::PoolAccount::try_from(&account.data)
            .map_err(ClientError::DecodeCollectionPool)?;
        if !(state.namespace == namespace
            && state.token1_definition_id == target.collateral_definition)
        {
            return Err(ClientError::PoolSelectionMismatch);
        }
        let treasury =
            prepare_fee_collection(wallet, namespace, curve.id(), target.collateral_definition)
                .await?;
        hashes.push(
            submit_public_invocation(
                wallet,
                curve,
                build_collect_fees_invocation(
                    namespace,
                    curve.id(),
                    target.pool,
                    target.collateral_definition,
                    treasury,
                ),
            )
            .await?,
        );
    }
    Ok(hashes)
}
