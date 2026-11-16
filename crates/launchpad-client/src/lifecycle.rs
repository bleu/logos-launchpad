//! Resumable factory workflows. Every stage is confirmed before the next is built.
use super::*;
use factory_core::CreationStage;

#[derive(Debug, Default)]
pub struct LifecycleReceipt {
    pub transaction_hashes: Vec<HashType>,
    pub authority_holders: Vec<AccountId>,
}

/// A factory workflow bound to one namespace and creator role holder.
pub struct FactorySession<'a> {
    wallet: &'a mut LaunchpadWallet,
    factory: &'a ProgramHandle,
    curve: &'a ProgramHandle,
    namespace: AccountId,
    authority: AccountLocation,
}

impl<'a> FactorySession<'a> {
    pub fn new(
        wallet: &'a mut LaunchpadWallet,
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
        Ok(Some(state))
    }

    async fn submit(
        &mut self,
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

    /// Resume from on-chain state; completed creation is an idempotent no-op.
    pub async fn create_sale(
        &mut self,
        sale: FactorySale,
        request: CreateSaleRequest,
    ) -> Result<LifecycleReceipt> {
        self.validate_sale(sale)?;
        if self.authority.is_private() {
            return Err(ClientError::UnsupportedPrivateAuthority);
        }
        let mut receipt = LifecycleReceipt::default();
        for _ in 0..4 {
            let state = self.state(sale).await?;
            if next_creation_invocation(
                sale,
                self.authority.account_id(),
                request.clone(),
                state.as_ref(),
            )?
            .is_none()
            {
                return Ok(receipt);
            }
            self.submit(
                |holder| {
                    next_creation_invocation(sale, holder, request.clone(), state.as_ref())?
                        .ok_or(ClientError::CreationAlreadyComplete)
                },
                &mut receipt,
            )
            .await?;
        }
        if !(self
            .state(sale)
            .await?
            .is_some_and(|state| state.creation_stage == CreationStage::Active))
        {
            return Err(ClientError::CreationIncomplete);
        }
        Ok(receipt)
    }

    /// Resume the exact withdrawal, burn and payout sequence without paying a stage twice.
    pub async fn withdraw(&mut self, sale: FactorySale, now: u64) -> Result<LifecycleReceipt> {
        self.validate_sale(sale)?;
        if self.authority.is_private() {
            return Err(ClientError::UnsupportedPrivateAuthority);
        }
        let mut receipt = LifecycleReceipt::default();
        for _ in 0..5 {
            let state = self
                .state(sale)
                .await?
                .ok_or(ClientError::FactoryLaunchMissing)?;
            if state.settlement_stage.is_manual_settlement_complete() {
                return Ok(receipt);
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
                &mut receipt,
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
        Ok(receipt)
    }
}

/// Chooses the next wire call from persisted state; never resubmits the mint stage.
pub fn next_creation_invocation(
    sale: FactorySale,
    holder: AccountId,
    request: CreateSaleRequest,
    state: Option<&FactoryState>,
) -> Result<Option<PublicInvocation<FactoryInstruction>>> {
    if let Some(state) = state {
        validate_factory_state(sale, state)?;
        if !(state.buy_policy == request.buy_policy
            && state.trade_mode == request.trade_mode
            && state.graduation_hook == request.graduation_hook
            && state.sale_reserve == request.sale_reserve
            && state.dex_seed_reserve == request.dex_seed_reserve
            && state.creator_allocation == request.creator_allocation
            && state.virtual_token_reserve == request.virtual_token_reserve
            && state.virtual_collateral_reserve == request.virtual_collateral_reserve
            && state.end_timestamp == request.end_timestamp
            && state.name == request.name
            && state.uri == request.uri)
        {
            return Err(ClientError::LaunchParametersMismatch);
        }
        if state.creation_stage == CreationStage::Active {
            return Ok(None);
        }
    }
    let now = request.now;
    let mut call = build_create_sale_invocation(sale, holder, request)?;
    if state.is_some() {
        call.instruction = FactoryInstruction::ContinueCreation {
            creator: holder,
            now,
        };
    }
    Ok(Some(call))
}
