use borsh::BorshSerialize;
use common::HashType;
use curve_core::Message as CurveInstruction;
use factory_core::Message as FactoryInstruction;
use lee::{AccountId, program::Program};
use lee_core::account::Actor;
use lez_compat_core::token;
use program_loader_core::LoaderActorState as _;
use wallet::{WalletCore, WalletSequencerClient};

use crate::invocation::{associated_token_account, clock_actor, invocation, token_actor};
use crate::{
    AccountLocation, ClientError, FactorySale, PrivateAccountId, ProgramHandle, PublicInvocation,
    PublicWriteWallet, Result, write_transport,
};

/// The upstream wallet for legacy authority and private-flow helpers.
pub type LaunchpadWallet = wallet::WalletCore<sequencer_service_rpc::SequencerClient>;

#[derive(Debug, Clone, Copy)]
pub struct PrivateBuyRequest {
    pub amount_out: u128,
    pub max_collateral_in: u128,
    pub from_private: PrivateAccountId,
    pub to_private: PrivateAccountId,
    pub gas_reserve: u128,
}

pub fn validate_private_buy_request(request: PrivateBuyRequest) -> Result<()> {
    if request.gas_reserve == 0 {
        return Err(ClientError::ZeroPrivateBuyGasReserve);
    }
    if request.amount_out == 0 || request.max_collateral_in == 0 {
        return Err(ClientError::ZeroPrivateBuyAmount);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct PrivateBuyReceipt {
    pub transaction_hash: HashType,
    pub transient_public_account: AccountId,
    pub private_destination: AccountId,
}
pub struct PrivateBuyRoute<'a> {
    pub private_buy_program: &'a Program,
    pub curve_program: &'a Program,
}

/// Canonical public token holdings do not implement the full private route.
pub async fn submit_private_buy(
    _wallet: &mut LaunchpadWallet,
    _sale: FactorySale,
    _route: PrivateBuyRoute<'_>,
    request: PrivateBuyRequest,
) -> Result<PrivateBuyReceipt> {
    validate_private_buy_request(request)?;
    Err(ClientError::UnsupportedPrivateRoute)
}

/// Legacy NFT helper. This does not create a launch role.
pub fn build_create_authority_invocation(
    definition: AccountId,
    owner: AccountId,
    name: String,
    uri: String,
) -> PublicInvocation<token::Message> {
    invocation(
        token_actor(definition),
        vec![token_actor(associated_token_account(owner, definition))],
        vec![definition],
        token::Message::Create {
            name,
            kind: token::TokenKind::NftMaster,
            supply: 1,
            recipient: owner,
            metadata: Some(token::Metadata {
                standard: token::MetadataStandard::Simple,
                uri,
                creators: String::new(),
            }),
        },
    )
    .with_holding(owner, definition)
}
/// Legacy NFT helper. This does not transfer a launch role.
pub fn build_transfer_authority_invocation(
    owner: AccountId,
    recipient: AccountId,
    definition: AccountId,
) -> PublicInvocation<token::Message> {
    invocation(
        token_actor(owner),
        vec![
            token_actor(associated_token_account(owner, definition)),
            token_actor(associated_token_account(recipient, definition)),
        ],
        vec![owner],
        token::Message::Transfer {
            definition,
            recipient,
            amount: 1,
        },
    )
    .with_holding(owner, definition)
    .with_holding(recipient, definition)
}

pub trait PrivateInstruction: BorshSerialize + Clone {}
impl PrivateInstruction for CurveInstruction {}
impl PrivateInstruction for FactoryInstruction {}

pub fn build_private_authority_instruction<I: PrivateInstruction>(
    _action: &PublicInvocation<I>,
    _holder: AccountId,
) -> Result<()> {
    Err(ClientError::UnsupportedPrivateAuthority)
}
pub async fn submit_private_authority<I: PrivateInstruction>(
    _wallet: &mut LaunchpadWallet,
    _router: &ProgramHandle,
    _action_program: &ProgramHandle,
    _extra_dependencies: Vec<ProgramHandle>,
    _source: AccountId,
    _build: impl FnOnce(AccountId) -> Result<PublicInvocation<I>>,
) -> Result<PrivateBuyReceipt> {
    Err(ClientError::UnsupportedPrivateAuthority)
}

pub(super) async fn read_actor(
    wallet: &WalletCore<impl WalletSequencerClient>,
    actor: Actor,
) -> std::result::Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let account = wallet
        .get_account_view(actor)
        .await
        .map_err(|e| e.into_boxed_dyn_error())?
        .ok_or_else(|| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "actor state is missing",
            )) as Box<dyn std::error::Error + Send + Sync>
        })?;
    Ok(account
        .data
        .actor_state(actor.program_account_id)
        .as_ref()
        .to_vec())
}

/// Submits through the Logos wallet. Returns the hash only after a successful execution receipt.
pub async fn submit_public_invocation<I: BorshSerialize>(
    wallet: &PublicWriteWallet<impl WalletSequencerClient>,
    program: &ProgramHandle,
    call: PublicInvocation<I>,
) -> Result<HashType> {
    if call.destination.program_account_id != program.account_id {
        return Err(ClientError::InvocationProgramMismatch);
    }
    let header_actor = Actor::new(
        program.account_id,
        lee_core::program::PROGRAM_LOADER_ACCOUNT_ID,
    );
    let header_bytes = read_actor(wallet, header_actor)
        .await
        .map_err(ClientError::ReadProgramHeader)?;
    let header = lee_core::program::ProgramHeader::from_loader_actor_state(&header_bytes)
        .ok_or(ClientError::DecodeProgramHeader)?;
    if header.image_id != program.image_id() {
        return Err(ClientError::InvocationImageMismatch);
    }
    let message = call.message_bytes()?;
    let root = call
        .actors
        .iter()
        .position(|a| *a == call.destination)
        .ok_or(ClientError::InvocationDestinationMissing)?;
    let actors = call.account_mentions();
    // TODO(LOG-66): Save the signed transaction in the wallet before broadcast and recover its receipt after restart.
    let hash = wallet
        .send_pub_tx_paid_by(actors, root, message, call.fee_payer)
        .await
        .map_err(public_send_error)?;
    wait_for_execution(wallet, hash).await?;
    Ok(hash)
}

fn public_send_error(source: wallet::ExecutionFailureKind) -> ClientError {
    match source {
        wallet::ExecutionFailureKind::SimulatedRevert(_) => {
            ClientError::PublicWriteRejected(Box::new(source))
        }
        source if write_transport::is_send_failure(&source) => ClientError::PublicWritePending {
            hash: None,
            source: Box::new(source),
        },
        source => ClientError::PublicWritePreparation(Box::new(source)),
    }
}

async fn wait_for_execution(
    wallet: &WalletCore<impl WalletSequencerClient>,
    hash: HashType,
) -> Result<()> {
    use std::time::Duration;

    let receipt = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if let Some(receipt) = wallet.client().get_transaction_receipt(hash).await? {
                break Ok::<_, Box<dyn std::error::Error + Send + Sync>>(receipt);
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
    .await
    .map_err(|source| ClientError::PublicWritePending {
        hash: Some(hash),
        source: Box::new(source),
    })?
    .map_err(|source| ClientError::PublicWritePending {
        hash: Some(hash),
        source,
    })?;
    if !receipt.success {
        return Err(ClientError::PublicWriteReverted { hash });
    }
    Ok(())
}

/// Reads a legacy NFT holding, not a launch role authority.
pub async fn load_authority_identity(
    wallet: &LaunchpadWallet,
    location: AccountLocation,
    definition: AccountId,
) -> Result<AccountId> {
    read_authority_holding(wallet, location, definition).await
}

async fn read_authority_holding(
    wallet: &WalletCore<impl WalletSequencerClient>,
    location: AccountLocation,
    definition: AccountId,
) -> Result<AccountId> {
    if location.is_private() {
        return Err(ClientError::UnsupportedPrivateAuthority);
    }
    let owner = location.account_id();
    let data = read_actor(
        wallet,
        token_actor(associated_token_account(owner, definition)),
    )
    .await
    .map_err(ClientError::ReadAuthorityHolding)?;
    match token::TokenState::decode(&data).map_err(ClientError::DecodeAuthorityHolding)? {
        token::TokenState::Holding {
            definition: actual,
            owner: holder,
            kind: token::TokenKind::NftMaster,
            balance: 1,
        } if actual == definition && holder == owner => Ok(definition),
        _ => Err(ClientError::InvalidAuthorityHolding),
    }
}
/// Transfers a legacy NFT, not a launch role authority.
pub async fn transfer_authority(
    wallet: &mut PublicWriteWallet<impl WalletSequencerClient>,
    token_program: &ProgramHandle,
    source: AccountLocation,
    destination: AccountLocation,
    definition: AccountId,
) -> Result<HashType> {
    if source.is_private() || destination.is_private() {
        return Err(ClientError::UnsupportedPrivateAuthority);
    }
    if source == destination {
        return Err(ClientError::SameAuthoritySourceAndDestination);
    }
    read_authority_holding(wallet, source, definition).await?;
    submit_public_invocation(
        wallet,
        token_program,
        build_transfer_authority_invocation(
            source.account_id(),
            destination.account_id(),
            definition,
        ),
    )
    .await
}

/// Reads the block ID that a transaction in the next block observes from the canonical clock.
pub async fn load_clock_block_id(wallet: &WalletCore<impl WalletSequencerClient>) -> Result<u64> {
    let data = read_actor(wallet, clock_actor())
        .await
        .map_err(ClientError::ReadClock)?;
    borsh::from_slice::<clock_core::ClockAccountData>(&data)
        .map(|clock| clock.block_id)
        .map_err(ClientError::DecodeClock)
}
