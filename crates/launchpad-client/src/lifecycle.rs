//! Public factory creation and resumable settlement.
use common::HashType;
use factory_core::{
    CreationStage, CreatorAllocationRelease, FactoryState, FixedPriceInputs, LbpCreationStage,
    Message as FactoryInstruction, SaleStart, WeightedInputs, compute_factory_pda,
};
use lee::AccountId;
use wallet::WalletSequencerClient;

use crate::curve::{validate_factory_pool, validate_factory_state};
use crate::lbp::{fixed_price_sale, lbp_addresses, weighted_sale};
use crate::{
    AccountLocation, ClientError, CreateSaleRequest, EndingPolicy, FactorySale, ProgramHandle,
    PublicInvocation, PublicWriteWallet, Result, SaleInfo, SaleSelection,
    build_create_fixed_price_invocation, build_create_sale_invocation,
    build_create_weighted_invocation, build_withdraw_factory_proceeds_invocation,
    load_factory_pool, load_sale_info, prepare_fee_collection, submit_public_invocation,
};

#[derive(Debug, Default)]
pub struct LifecycleReceipt {
    pub transaction_hashes: Vec<HashType>,
    pub authority_holders: Vec<AccountId>,
}

#[derive(Debug, Clone)]
pub enum LbpCreateRequest {
    FixedPrice(FixedPriceInputs),
    Weighted(WeightedInputs),
}

impl LbpCreateRequest {
    fn validate(&self, factory_program_id: AccountId) -> Result<()> {
        let invalid = || ClientError::FactoryPolicy(factory_core::Error::InvalidTerms);
        let (ending, start, end, sale, buy_policy) = match self {
            Self::FixedPrice(inputs) => {
                if inputs.price.collateral_units == 0
                    || inputs.price.token_units == 0
                    || inputs.ending_policy == EndingPolicy::Manual
                {
                    return Err(invalid());
                }
                let (SaleStart::Now { timestamp } | SaleStart::At { timestamp }) = inputs.start;
                (
                    inputs.ending_policy,
                    timestamp,
                    inputs.end_timestamp,
                    fixed_price_sale(factory_program_id, inputs),
                    &inputs.buy_policy,
                )
            }
            Self::Weighted(inputs) => {
                if inputs.collateral_seed == 0
                    || !lbp_core::valid_weight_endpoints(
                        inputs.token_weight_start,
                        inputs.token_weight_end,
                    )
                    || !matches!(
                        inputs.ending_policy,
                        EndingPolicy::Scheduled | EndingPolicy::Either
                    )
                {
                    return Err(invalid());
                }
                (
                    inputs.ending_policy,
                    inputs.start_timestamp,
                    Some(inputs.end_timestamp),
                    weighted_sale(factory_program_id, inputs),
                    &inputs.buy_policy,
                )
            }
        };
        ending.validate(Some(start), end).map_err(|_| invalid())?;
        for seconds in [Some(start), end].into_iter().flatten() {
            curve_core::timestamp_millis(seconds).map_err(|_| invalid())?;
        }
        buy_policy
            .validate_binding(sale.namespace.value(), lbp_addresses(sale).pool.value())
            .map_err(|_| invalid())
    }
}

/// Create and verify a funded sale. A matching sale is a no-op for its current creator.
pub async fn create_lbp_sale(
    wallet: &PublicWriteWallet<impl WalletSequencerClient>,
    factory: &ProgramHandle,
    request: LbpCreateRequest,
) -> Result<LifecycleReceipt> {
    request.validate(factory.account_id)?;
    let (selection, creator, call) = match &request {
        LbpCreateRequest::FixedPrice(inputs) => (
            SaleSelection::FixedPrice(fixed_price_sale(factory.account_id, inputs)),
            inputs.creator,
            build_create_fixed_price_invocation(factory.account_id, inputs.clone()),
        ),
        LbpCreateRequest::Weighted(inputs) => (
            SaleSelection::Weighted(weighted_sale(factory.account_id, inputs)),
            inputs.creator,
            build_create_weighted_invocation(factory.account_id, inputs.clone()),
        ),
    };
    match load_sale_info(wallet.client(), selection).await {
        Ok(info) => {
            let role = verify_lbp_creation(info, &request)?;
            validate_creator(wallet, role, creator).await?;
            return Ok(LifecycleReceipt::default());
        }
        Err(ClientError::FactoryLaunchMissing) => {}
        Err(error) => return Err(error),
    }

    let hash = submit_public_invocation(wallet, factory, call).await?;
    load_sale_info(wallet.client(), selection)
        .await
        .and_then(|info| verify_lbp_creation(info, &request))
        .map_err(|source| ClientError::CreationVerificationFailed {
            hash,
            source: Box::new(source),
        })?;
    Ok(LifecycleReceipt {
        transaction_hashes: vec![hash],
        authority_holders: vec![creator],
    })
}

fn verify_lbp_creation(info: SaleInfo, request: &LbpCreateRequest) -> Result<AccountId> {
    macro_rules! common_terms_match {
        ($sale:expr, $inputs:expr) => {{
            let terms = &$sale.terms;
            terms.name == $inputs.name
                && terms.uri == $inputs.uri
                && terms.sale_inventory == $inputs.sale_inventory
                && terms.graduation_reserve == $inputs.graduation_reserve
                && terms.creator_allocation == $inputs.creator_allocation
                && terms.buy_policy == $inputs.buy_policy
                && terms.pause_policy == $inputs.pause_policy
                && terms.ending_policy == $inputs.ending_policy
                && terms.trade_mode == $inputs.trade_mode
                && terms.sell_fee_bps == $inputs.sell_fee_bps
                && terms.per_block_token_limit == $inputs.per_block_token_limit
                && terms.graduation_hook == $inputs.graduation_hook
                && terms.graduation_policy == $inputs.graduation_policy
                && $sale.allocation_release == $inputs.allocation_release
        }};
    }
    let (sale, terms_match) = match (request, info) {
        (LbpCreateRequest::FixedPrice(inputs), SaleInfo::FixedPrice(info)) => {
            let (SaleStart::Now { timestamp } | SaleStart::At { timestamp }) = inputs.start;
            let terms_match = common_terms_match!(info.sale, inputs)
                && info.sale.start_timestamp == timestamp
                && info.sale.terms.end_timestamp == inputs.end_timestamp
                && info.price == inputs.price;
            (info.sale, terms_match)
        }
        (LbpCreateRequest::Weighted(inputs), SaleInfo::Weighted(info)) => {
            let terms_match = common_terms_match!(info.sale, inputs)
                && info.sale.start_timestamp == inputs.start_timestamp
                && info.sale.terms.end_timestamp == Some(inputs.end_timestamp)
                && info.collateral_seed == inputs.collateral_seed
                && info.token_weight_start == inputs.token_weight_start
                && info.token_weight_end == inputs.token_weight_end;
            (info.sale, terms_match)
        }
        _ => return Err(ClientError::SaleEngineMismatch),
    };
    if sale.setup_stage != LbpCreationStage::Complete
        || !sale.pool.is_some_and(|pool| pool.funded)
        || (sale.allocation_release == CreatorAllocationRelease::AfterSetup
            && !sale.creator_allocation_released)
    {
        return Err(ClientError::CreationIncomplete);
    }
    if !terms_match {
        return Err(ClientError::LaunchParametersMismatch);
    }
    Ok(sale.terms.creator_role)
}

async fn validate_creator(
    wallet: &PublicWriteWallet<impl WalletSequencerClient>,
    role: AccountId,
    creator: AccountId,
) -> Result<()> {
    let account = wallet
        .get_account_public(role)
        .await
        .map_err(|source| ClientError::ReadCreatorRole(source.into_boxed_dyn_error()))?
        .ok_or(ClientError::CreatorRoleMissing)?;
    let state = account
        .data
        .actor_state(curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID);
    if state.is_empty() {
        return Err(ClientError::CreatorRoleMissing);
    }
    let holder = lez_compat_core::authority::current_holder(state.as_ref())
        .map_err(ClientError::DecodeCreatorRole)?;
    if holder != creator {
        return Err(ClientError::CreatorAuthorityMismatch);
    }
    wallet
        .get_account_public_signing_key(creator)
        .ok_or(ClientError::PublicSigningKeyUnavailable(creator))?;
    Ok(())
}

/// A factory workflow bound to one namespace and creator role holder.
pub struct FactorySession<'a, C: WalletSequencerClient> {
    wallet: &'a PublicWriteWallet<C>,
    factory: &'a ProgramHandle,
    curve: &'a ProgramHandle,
    namespace: AccountId,
    authority: AccountLocation,
}

impl<'a, C: WalletSequencerClient> FactorySession<'a, C> {
    pub fn new(
        wallet: &'a PublicWriteWallet<C>,
        factory: &'a ProgramHandle,
        curve: &'a ProgramHandle,
        _router: Option<&'a ProgramHandle>,
        namespace: AccountId,
        authority: AccountLocation,
    ) -> Self {
        Self {
            wallet,
            factory,
            curve,
            namespace,
            authority,
        }
    }

    fn validate_sale(&self, sale: FactorySale) -> Result<()> {
        if sale.namespace != self.namespace
            || sale.factory_program_id != self.factory.account_id
            || sale.curve_program_id != self.curve.account_id
        {
            return Err(ClientError::FactorySaleMismatch);
        }
        Ok(())
    }

    async fn state(&self, sale: FactorySale) -> Result<Option<FactoryState>> {
        self.validate_sale(sale)?;
        let account = self
            .wallet
            .get_account_public(compute_factory_pda(
                sale.namespace,
                sale.factory_program_id,
                sale.launch_salt,
            ))
            .await
            .map_err(|source| ClientError::ReadFactoryState(source.into_boxed_dyn_error()))?;
        let Some(account) = account else {
            return Ok(None);
        };
        let bytes = account.data.actor_state(sale.factory_program_id);
        if bytes.as_ref().is_empty() {
            return Ok(None);
        }
        let state = FactoryState::decode(bytes.as_ref())
            .map_err(ClientError::DecodeResumableFactoryState)?;
        validate_factory_state(sale, &state)?;
        if state.creation_stage == CreationStage::Active {
            let pool = load_factory_pool(self.wallet, sale).await?;
            validate_factory_pool(sale, &state, &pool)?;
            if !pool.funded {
                return Err(ClientError::CreationIncomplete);
            }
        }
        Ok(Some(state))
    }

    async fn submit(
        &self,
        build: impl FnOnce(AccountId) -> Result<PublicInvocation<FactoryInstruction>>,
        receipt: &mut LifecycleReceipt,
    ) -> Result<()> {
        match self.authority {
            AccountLocation::Private(_) => return Err(ClientError::UnsupportedPrivateAuthority),
            AccountLocation::Public(source) => {
                receipt.transaction_hashes.push(
                    submit_public_invocation(self.wallet, self.factory, build(source)?).await?,
                );
                receipt.authority_holders.push(source);
            }
        }
        Ok(())
    }

    /// Create and verify a funded sale. A matching sale is a no-op for its current creator.
    pub async fn create_sale(
        &self,
        sale: FactorySale,
        request: CreateSaleRequest,
    ) -> Result<LifecycleReceipt> {
        let AccountLocation::Public(creator) = self.authority else {
            return Err(ClientError::UnsupportedPrivateAuthority);
        };
        if let Some(state) = self.state(sale).await? {
            validate_completed_creation(sale, &request, &state)?;
            validate_creator(self.wallet, state.creator_role_id, creator).await?;
            return Ok(LifecycleReceipt::default());
        }

        let call = build_create_sale_invocation(sale, creator, request.clone())?;
        let hash = submit_public_invocation(self.wallet, self.factory, call).await?;
        self.verify_creation(sale, &request)
            .await
            .map_err(|source| ClientError::CreationVerificationFailed {
                hash,
                source: Box::new(source),
            })?;
        Ok(LifecycleReceipt {
            transaction_hashes: vec![hash],
            authority_holders: vec![creator],
        })
    }

    async fn verify_creation(&self, sale: FactorySale, request: &CreateSaleRequest) -> Result<()> {
        let state = self
            .state(sale)
            .await?
            .ok_or(ClientError::CreationIncomplete)?;
        validate_completed_creation(sale, request, &state)
    }

    /// Resume the exact withdrawal and payout sequence without paying a stage twice.
    pub async fn withdraw(&self, sale: FactorySale, now: u64) -> Result<LifecycleReceipt> {
        let mut receipt = LifecycleReceipt::default();
        let result = self.withdraw_steps(sale, now, &mut receipt).await;
        finish(result, receipt)
    }

    async fn withdraw_steps(
        &self,
        sale: FactorySale,
        now: u64,
        receipt: &mut LifecycleReceipt,
    ) -> Result<()> {
        self.validate_sale(sale)?;
        if self.authority.is_private() {
            return Err(ClientError::UnsupportedPrivateAuthority);
        }
        for _ in 0..5 {
            let state = self
                .state(sale)
                .await?
                .ok_or(ClientError::FactoryLaunchMissing)?;
            if state.graduation_hook.is_some() {
                return Err(ClientError::FactoryPolicy(
                    factory_core::Error::InvalidTerms,
                ));
            }
            if state.settlement_stage.is_manual_settlement_complete() {
                return Ok(());
            }
            let treasury = prepare_fee_collection(
                self.wallet,
                sale.namespace,
                sale.curve_program_id,
                sale.collateral_definition,
            )
            .await?;
            self.submit(
                |holder| {
                    Ok(build_withdraw_factory_proceeds_invocation(
                        sale, holder, treasury, now,
                    ))
                },
                receipt,
            )
            .await?;
        }
        if !(self
            .state(sale)
            .await?
            .is_some_and(|state| state.settlement_stage.is_manual_settlement_complete()))
        {
            return Err(ClientError::SettlementIncomplete);
        }
        Ok(())
    }
}

fn finish(result: Result<()>, receipt: LifecycleReceipt) -> Result<LifecycleReceipt> {
    match result {
        Ok(()) => Ok(receipt),
        Err(error) => Err(error.after_execution(receipt.transaction_hashes)),
    }
}

/// Build one creation call for a fresh sale. A matching active sale needs no call.
pub fn next_creation_invocation(
    sale: FactorySale,
    holder: AccountId,
    request: CreateSaleRequest,
    state: Option<&FactoryState>,
) -> Result<Option<PublicInvocation<FactoryInstruction>>> {
    if let Some(state) = state {
        validate_completed_creation(sale, &request, state)?;
        return Ok(None);
    }
    build_create_sale_invocation(sale, holder, request).map(Some)
}

fn validate_completed_creation(
    sale: FactorySale,
    request: &CreateSaleRequest,
    state: &FactoryState,
) -> Result<()> {
    validate_factory_state(sale, state)?;
    if state.creation_stage != CreationStage::Active {
        return Err(ClientError::CreationIncomplete);
    }
    if !(state.buy_policy == request.buy_policy
        && state.pause_policy == request.pause_policy
        && state.ending_policy == request.ending_policy
        && state.trade_mode == request.trade_mode
        && state.sell_fee_bps == request.sell_fee_bps
        && state.per_block_token_limit == request.per_block_token_limit
        && state.graduation_hook == request.graduation_hook
        && state.sale_reserve == request.sale_reserve
        && state.graduation_reserve == request.graduation_reserve
        && state.graduation_policy == request.graduation_policy
        && state.creator_allocation == request.creator_allocation
        && state.virtual_token_reserve == request.virtual_token_reserve
        && state.virtual_collateral_reserve == request.virtual_collateral_reserve
        && state.end_timestamp == request.end_timestamp
        && state.name == request.name
        && state.uri == request.uri)
    {
        return Err(ClientError::LaunchParametersMismatch);
    }
    Ok(())
}
