use std::{error::Error, path::PathBuf};

use lee::AccountId;

/// Errors from client validation, quotes, wallet operations and transaction submission.
#[derive(Debug, thiserror::Error, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum ClientError {
    #[error("distribution adapter: {0:?}")]
    Distribution(distribution_adapter::Error),
    #[error("private purchases are not supported by canonical public token holdings")]
    UnsupportedPrivateRoute,
    #[error("private authority actions are not supported by canonical public token holdings")]
    UnsupportedPrivateAuthority,
    #[error("serializing actor message")]
    EncodeMessage(#[source] std::io::Error),
    #[error("reading deployed program header")]
    ReadProgramHeader(#[source] Box<dyn Error + Send + Sync>),
    #[error("invalid deployed program header")]
    DecodeProgramHeader,
    #[error("deployed program image differs from the supplied binary")]
    InvocationImageMismatch,
    #[error("destination actor is not declared")]
    InvocationDestinationMissing,
    #[error("not a valid 32-byte base58 account id: {input}")]
    #[strum(serialize = "invalid_account_id")]
    InvalidAccountId {
        input: String,
        #[source]
        source: lee_core::account::AccountIdError,
    },
    #[error("account location has an unknown prefix: {input}")]
    #[strum(serialize = "unknown_account_location_prefix")]
    UnknownAccountLocationPrefix { input: String },
    #[error("a private account ID cannot use a public location: {input}")]
    #[strum(serialize = "private_account_id_requires_private_location")]
    PrivateAccountIdRequiresPrivateLocation { input: String },
    #[error("reading guest binary {}", path.display())]
    #[strum(serialize = "read_program")]
    ReadProgram {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{} is not a valid guest program", path.display())]
    #[strum(serialize = "invalid_program")]
    InvalidProgram {
        path: PathBuf,
        #[source]
        source: Box<lee::error::LeeError>,
    },
    #[error("private buy requires a non-zero gas reserve")]
    ZeroPrivateBuyGasReserve,
    #[error("private buy requires non-zero token output and collateral cap")]
    ZeroPrivateBuyAmount,
    #[error("private buy curve guest does not match the sale")]
    PrivateBuyCurveProgramMismatch,
    #[error("persisting the transient public account key")]
    #[strum(serialize = "store_transient_key")]
    StoreTransientKey(#[source] Box<dyn Error + Send + Sync>),
    #[error("wallet does not control private collateral source")]
    PrivateCollateralSourceUnavailable,
    #[error("wallet does not control private token destination")]
    PrivateTokenDestinationUnavailable,
    #[error("serializing private buy instruction")]
    #[strum(serialize = "encode_private_buy")]
    EncodePrivateBuy(#[source] Box<lee::error::LeeError>),
    #[error("submitting atomic private purchase")]
    #[strum(serialize = "submit_private_buy")]
    SubmitPrivateBuy(#[source] Box<wallet::ExecutionFailureKind>),
    #[error("input token is restricted by the pool")]
    InputTokenRestricted,
    #[error("namespace is frozen")]
    NamespaceFrozen,
    #[error("pool funding is pending")]
    PoolFundingPending,
    #[error("pool swaps are paused")]
    PoolPaused,
    #[error("cannot quote purchase: {0}")]
    #[strum(serialize = "quote_buy")]
    QuoteBuy(#[source] pool::SwapError),
    #[error("cannot quote sale: {0}")]
    #[strum(serialize = "quote_sell")]
    QuoteSell(#[source] pool::SwapError),
    #[error("cannot quote collateral purchase: {0}")]
    #[strum(serialize = "quote_buy_with_collateral")]
    QuoteBuyWithCollateral(#[source] pool::SwapError),
    #[error("cannot quote LBP trade: {0}")]
    QuoteLbpTrade(#[source] lbp_core::Error),
    #[error("a capped LBP buy quote needs the observed block ID")]
    BlockIdRequired,
    #[error("purchase exceeds the sale token allowance for this block")]
    CurveBlockAllocationExceeded,
    #[error("reading the canonical clock")]
    #[strum(serialize = "read_clock")]
    ReadClock(#[source] Box<dyn Error + Send + Sync>),
    #[error("decoding the canonical clock")]
    #[strum(serialize = "decode_clock")]
    DecodeClock(#[source] std::io::Error),
    #[error("invalid factory curve parameters: {0}")]
    #[strum(serialize = "factory_parameters")]
    FactoryParameters(#[from] factory_core::CreateError),
    #[error("invalid factory sale policy: {0}")]
    FactoryPolicy(#[from] factory_core::Error),
    #[error("reading the live curve configuration")]
    #[strum(serialize = "read_curve_config")]
    ReadCurveConfig(#[source] Box<dyn Error + Send + Sync>),
    #[error("decoding the live curve configuration")]
    #[strum(serialize = "decode_curve_config")]
    DecodeCurveConfig(#[source] curve_core::Error),
    #[error("reading the factory launch state")]
    #[strum(serialize = "read_factory_state")]
    ReadFactoryState(#[source] Box<dyn Error + Send + Sync>),
    #[error("decoding the factory launch state")]
    #[strum(serialize = "decode_factory_state")]
    DecodeFactoryState(#[source] factory_core::Error),
    #[error("reading the factory curve pool")]
    #[strum(serialize = "read_factory_pool")]
    ReadFactoryPool(#[source] Box<dyn Error + Send + Sync>),
    #[error("decoding the factory curve pool")]
    #[strum(serialize = "decode_factory_pool")]
    DecodeFactoryPool(#[source] curve_core::Error),
    #[error("program binary does not match the invocation program ID")]
    InvocationProgramMismatch,
    #[error("wallet has no public signing key for {0}")]
    PublicSigningKeyUnavailable(AccountId),
    #[error("querying signer account nonces")]
    #[strum(serialize = "read_signer_nonces")]
    ReadSignerNonces(#[source] Box<dyn Error + Send + Sync>),
    #[error("serializing launchpad instruction")]
    #[strum(serialize = "encode_instruction")]
    EncodeInstruction(#[source] Box<lee::error::LeeError>),
    #[error("preparing public launchpad transaction")]
    PublicWritePreparation(#[source] Box<wallet::ExecutionFailureKind>),
    #[error("public transaction was rejected by simulation before submission")]
    PublicWriteRejected(#[source] Box<wallet::ExecutionFailureKind>),
    #[error("public transaction outcome is not known")]
    PublicWritePending {
        hash: Option<common::HashType>,
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("public write stopped after earlier transactions executed")]
    WriteInterrupted {
        executed_hashes: Vec<common::HashType>,
        #[source]
        source: Box<ClientError>,
    },
    #[error("private authority action must use its authority holder as the only signer")]
    InvalidAuthoritySigners,
    #[error("action does not include the authority holder")]
    AuthorityHolderMissing,
    #[error("action accounts must be distinct")]
    DuplicateActionAccounts,
    #[error("wallet does not control private authority source")]
    PrivateAuthoritySourceUnavailable,
    #[error("persisting authority holder key")]
    #[strum(serialize = "store_authority_key")]
    StoreAuthorityKey(#[source] Box<dyn Error + Send + Sync>),
    #[error("action program does not match supplied guest")]
    AuthorityProgramMismatch,
    #[error("private authority source cannot also be an action account")]
    AuthoritySourceIsActionAccount,
    #[error("submitting atomic private authority action")]
    #[strum(serialize = "submit_private_authority")]
    SubmitPrivateAuthority(#[source] Box<wallet::ExecutionFailureKind>),
    #[error("private authority holding is unavailable")]
    PrivateAuthorityHoldingUnavailable,
    #[error("reading public authority holding")]
    #[strum(serialize = "read_authority_holding")]
    ReadAuthorityHolding(#[source] Box<dyn Error + Send + Sync>),
    #[error("authority holding must belong to the token program")]
    AuthorityHoldingProgramMismatch,
    #[error("decoding authority holding: {0:?}")]
    #[strum(serialize = "decode_authority_holding")]
    DecodeAuthorityHolding(lez_compat_core::Error),
    #[error("authority requires an NFT master with one remaining unit")]
    InvalidAuthorityHolding,
    #[error("authority source and destination must differ")]
    SameAuthoritySourceAndDestination,
    #[error("wallet does not control private holding")]
    PrivateHoldingUnavailable,
    #[error("transferring private authority")]
    #[strum(serialize = "transfer_private_authority")]
    TransferPrivateAuthority(#[source] Box<wallet::ExecutionFailureKind>),
    #[error("public transaction {} reverted", hex::encode(hash))]
    PublicWriteReverted { hash: common::HashType },
    #[error("expected a private transaction")]
    ExpectedPrivateTransaction,
    #[error("private result count mismatch")]
    PrivateResultCountMismatch,
    #[error("private output index exceeds u32")]
    #[strum(serialize = "private_output_index")]
    PrivateOutputIndex(#[from] std::num::TryFromIntError),
    #[error("decrypting confirmed private account")]
    DecryptPrivateAccount,
    #[error("updating confirmed private account {account}")]
    #[strum(serialize = "update_private_account")]
    UpdatePrivateAccount {
        account: AccountId,
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("persisting confirmed private authority state")]
    #[strum(serialize = "store_private_state")]
    StorePrivateState(#[source] Box<dyn Error + Send + Sync>),
    #[error("reading treasury collateral ATA")]
    #[strum(serialize = "read_treasury_account")]
    ReadTreasuryAccount(#[source] Box<dyn Error + Send + Sync>),
    #[error("initializing treasury collateral ATA")]
    #[strum(serialize = "initialize_treasury_account")]
    InitializeTreasuryAccount(#[source] Box<ClientError>),
    #[error("reading collection pool")]
    #[strum(serialize = "read_collection_pool")]
    ReadCollectionPool(#[source] Box<dyn Error + Send + Sync>),
    #[error("pool belongs to another program")]
    PoolProgramMismatch,
    #[error("decoding collection pool")]
    #[strum(serialize = "decode_collection_pool")]
    DecodeCollectionPool(#[source] curve_core::Error),
    #[error("pool does not match selected namespace or collateral")]
    PoolSelectionMismatch,
    #[error("factory account belongs to another program")]
    FactoryProgramMismatch,
    #[error("decoding resumable factory state")]
    #[strum(serialize = "decode_resumable_factory_state")]
    DecodeResumableFactoryState(#[source] factory_core::Error),
    #[error("loaded factory state or pool does not match the selected sale")]
    FactorySaleMismatch,
    #[error("saved sale uses a different pricing engine")]
    SaleEngineMismatch,
    #[error("private authority router is required")]
    PrivateAuthorityRouterRequired,
    #[error("creation already completed")]
    CreationAlreadyComplete,
    #[error("creation did not complete; retry with the same launch salt")]
    CreationIncomplete,
    #[error("factory launch does not exist")]
    FactoryLaunchMissing,
    #[error("launch does not match the selected collateral or curve")]
    LaunchSelectionMismatch,
    #[error("graduation hook is not configured")]
    GraduationNotConfigured,
    #[error("too many graduation adapter accounts")]
    TooManyGraduationAccounts,
    #[error("settlement did not complete; retry with the same launch salt")]
    SettlementIncomplete,
    #[error("existing launch parameters differ; resume with the original parameters")]
    LaunchParametersMismatch,
}

impl ClientError {
    pub(crate) fn after_execution(self, executed_hashes: Vec<common::HashType>) -> Self {
        if executed_hashes.is_empty() {
            self
        } else {
            Self::WriteInterrupted {
                executed_hashes,
                source: Box::new(self),
            }
        }
    }
}

/// The result of a client operation.
pub type Result<T> = std::result::Result<T, ClientError>;

impl From<lee::error::LeeError> for ClientError {
    fn from(source: lee::error::LeeError) -> Self {
        Self::EncodeInstruction(Box::new(source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_do_not_depend_on_source_messages() {
        for (error, expected) in [
            (
                ClientError::ZeroPrivateBuyGasReserve,
                "zero_private_buy_gas_reserve",
            ),
            (ClientError::PoolFundingPending, "pool_funding_pending"),
            (
                ClientError::LaunchParametersMismatch,
                "launch_parameters_mismatch",
            ),
            (ClientError::QuoteBuy(pool::SwapError::Closed), "quote_buy"),
            (
                ClientError::QuoteLbpTrade(lbp_core::Error::Closed),
                "quote_lbp_trade",
            ),
            (
                ClientError::QuoteBuy(pool::SwapError::SlippageExceeded),
                "quote_buy",
            ),
            (
                ClientError::QuoteSell(pool::SwapError::Closed),
                "quote_sell",
            ),
            (
                ClientError::QuoteBuyWithCollateral(pool::SwapError::Closed),
                "quote_buy_with_collateral",
            ),
            (
                factory_core::CreateError::SaleReserveZero.into(),
                "factory_parameters",
            ),
            (factory_core::Error::InvalidTerms.into(), "factory_policy"),
            (
                ClientError::ReadFactoryPool(Box::new(std::io::Error::other("slippage"))),
                "read_factory_pool",
            ),
            (
                ClientError::SubmitPrivateBuy(Box::new(
                    wallet::ExecutionFailureKind::KeyNotFoundError,
                )),
                "submit_private_buy",
            ),
        ] {
            let label: &'static str = (&error).into();
            assert_eq!(label, expected);
        }
    }

    #[test]
    fn domain_errors_keep_their_sources() {
        let error: ClientError =
            factory_core::CreateError::VirtualTokenReserveNotAboveSaleReserve.into();
        assert!(matches!(
            error
                .source()
                .unwrap()
                .downcast_ref::<factory_core::CreateError>(),
            Some(factory_core::CreateError::VirtualTokenReserveNotAboveSaleReserve)
        ));
        let error: ClientError = factory_core::Error::InvalidTerms.into();
        assert!(matches!(
            error
                .source()
                .unwrap()
                .downcast_ref::<factory_core::Error>(),
            Some(factory_core::Error::InvalidTerms)
        ));
        let error = ClientError::QuoteLbpTrade(lbp_core::Error::Closed);
        assert!(matches!(
            error.source().unwrap().downcast_ref::<lbp_core::Error>(),
            Some(lbp_core::Error::Closed)
        ));
        let error = ClientError::QuoteSell(pool::SwapError::InsufficientRealOutputReserve);
        assert!(matches!(
            error.source().unwrap().downcast_ref::<pool::SwapError>(),
            Some(pool::SwapError::InsufficientRealOutputReserve)
        ));
    }

    #[test]
    fn upstream_errors_keep_concrete_sources() {
        let error = ClientError::ReadFactoryPool(Box::new(std::io::Error::from(
            std::io::ErrorKind::PermissionDenied,
        )));
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .kind(),
            std::io::ErrorKind::PermissionDenied
        );
        let error = ClientError::SubmitPrivateAuthority(Box::new(
            wallet::ExecutionFailureKind::KeyNotFoundError,
        ));
        assert!(matches!(
            error
                .source()
                .unwrap()
                .downcast_ref::<Box<wallet::ExecutionFailureKind>>()
                .map(Box::as_ref),
            Some(wallet::ExecutionFailureKind::KeyNotFoundError)
        ));
        let error = ClientError::EncodeMessage(std::io::Error::other("invalid actor message"));
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .kind(),
            std::io::ErrorKind::Other
        );
    }

    #[test]
    fn treasury_initialization_keeps_the_submission_source_chain() {
        let error =
            ClientError::InitializeTreasuryAccount(Box::new(ClientError::ReadSignerNonces(
                Box::new(std::io::Error::from(std::io::ErrorKind::TimedOut)),
            )));
        let submission = error.source().unwrap();
        assert!(matches!(
            submission
                .downcast_ref::<Box<ClientError>>()
                .map(Box::as_ref),
            Some(ClientError::ReadSignerNonces(_))
        ));
        assert_eq!(
            submission
                .source()
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .kind(),
            std::io::ErrorKind::TimedOut
        );
    }
}
