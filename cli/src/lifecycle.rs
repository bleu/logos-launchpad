use std::path::Path;

use eyre::Result;
use launchpad_client::{
    AccountLocation, CreateSaleRequest, FactorySale, PoolFeeTarget, PublicWriteWallet,
    build_claim_creator_allocation_invocation, load_factory_state, submit_public_invocation,
};
use wallet::WalletSequencerClient;

use super::{
    args::{CollectFeesArgs, CreateSaleArgs, FactoryLifecycleArgs, GraduationArgs},
    current_time,
    distribution::read_distribution_configuration,
    factory_sale, load_deployment,
    write_result::WriteSuccess,
};

#[cfg(test)]
mod distribution_tests;

impl CreateSaleArgs {
    fn sale(&self, namespace: lee::AccountId) -> FactorySale {
        FactorySale {
            namespace,
            factory_program_id: self.factory_program_account,
            curve_program_id: self.curve_program_account,
            launch_salt: self.launch.launch_salt,
            collateral_definition: self.collateral_definition.account_id(),
        }
    }

    fn prepared_sale_request(&self, sale: FactorySale) -> Result<CreateSaleRequest> {
        let mut request = self.sale_request();
        request.buy_policy = launchpad_client::curve_buy_policy(sale, self.allowlist_root);
        request.total_supply()?;
        if let Some(file) = &self.distribution_configuration {
            let configuration = read_distribution_configuration(file)?;
            request.graduation_hook = Some(launchpad_client::distribution::hook(
                &configuration,
                self.graduation_hook_program_id.ok_or_else(|| {
                    eyre::eyre!("distribution requires --graduation-hook-program-id")
                })?,
                sale.factory_program_id,
                sale.namespace,
                sale.launch_salt,
            )?);
        }
        Ok(request)
    }
}

pub(super) async fn create_sale<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: CreateSaleArgs,
    router: Option<&Path>,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let sale = args.sale(namespace);
    let request = args.prepared_sale_request(sale)?;
    let factory_program =
        load_deployment(&args.factory_program_path, args.factory_program_account)?;
    let curve_program = load_deployment(&args.curve_program_path, args.curve_program_account)?;
    let wallet = open_wallet()?;
    if router.is_some() {
        return Err(launchpad_client::ClientError::UnsupportedPrivateAuthority.into());
    }
    let receipt = launchpad_client::FactorySession::new(
        &wallet,
        &factory_program,
        &curve_program,
        None,
        namespace,
        args.creator,
    )
    .create_sale(sale, request)
    .await?;
    Ok(WriteSuccess::creation(receipt, sale.launch_salt))
}

pub(super) async fn withdraw_factory_proceeds<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: FactoryLifecycleArgs,
    router: Option<&Path>,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let factory_program =
        load_deployment(&args.factory_program_path, args.factory_program_account)?;
    let curve_program = load_deployment(&args.curve_program_path, args.curve_program_account)?;
    let sale = factory_sale(
        namespace,
        &factory_program,
        &curve_program,
        args.launch.launch_salt,
        args.collateral_definition.account_id(),
    );
    let wallet = open_wallet()?;
    if router.is_some() {
        return Err(launchpad_client::ClientError::UnsupportedPrivateAuthority.into());
    }
    let receipt = launchpad_client::FactorySession::new(
        &wallet,
        &factory_program,
        &curve_program,
        None,
        namespace,
        args.creator,
    )
    .withdraw(sale, current_time()?)
    .await?;
    Ok(WriteSuccess::lifecycle(receipt, args.launch.launch_salt))
}

pub(super) async fn graduate<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: GraduationArgs,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let factory = load_deployment(&args.factory_program_path, args.factory_program_account)?;
    let curve = load_deployment(&args.curve_program_path, args.curve_program_account)?;
    let sale = factory_sale(
        namespace,
        &factory,
        &curve,
        args.launch.launch_salt,
        args.collateral_definition,
    );
    let wallet = open_wallet()?;
    let state = load_factory_state(&wallet, sale).await?;
    state
        .graduation_hook
        .as_ref()
        .ok_or(launchpad_client::ClientError::GraduationNotConfigured)?;
    let treasury = launchpad_client::prepare_fee_collection(
        &wallet,
        namespace,
        curve.account_id,
        sale.collateral_definition,
    )
    .await?;
    let target = launchpad_client::GraduationTarget::Curve {
        sale,
        state: &state,
        creator_hint: args.creator_hint,
    };
    let mut invocation = if args.distribution {
        launchpad_client::distribution::graduation(target, treasury)?
    } else {
        launchpad_client::build_graduation_invocation(target, treasury)?
    };
    invocation.fee_payer = Some(args.fee_payer);
    let transaction_hash = submit_public_invocation(&wallet, &factory, invocation).await?;
    Ok(WriteSuccess::single(
        transaction_hash,
        Some(args.launch.launch_salt),
    ))
}

pub(super) async fn claim_creator_allocation<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: FactoryLifecycleArgs,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let factory_program =
        load_deployment(&args.factory_program_path, args.factory_program_account)?;
    let curve_program = load_deployment(&args.curve_program_path, args.curve_program_account)?;
    let sale = factory_sale(
        namespace,
        &factory_program,
        &curve_program,
        args.launch.launch_salt,
        args.collateral_definition.account_id(),
    );

    let AccountLocation::Public(creator) = args.creator else {
        return Err(launchpad_client::ClientError::UnsupportedPrivateAuthority.into());
    };
    let wallet = open_wallet()?;
    load_factory_state(&wallet, sale).await?;
    let call = build_claim_creator_allocation_invocation(sale, creator, current_time()?);
    let transaction_hash = submit_public_invocation(&wallet, &factory_program, call).await?;
    Ok(WriteSuccess::single(
        transaction_hash,
        Some(args.launch.launch_salt),
    ))
}

pub(super) async fn collect_fees<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: CollectFeesArgs,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let wallet = open_wallet()?;
    let curve = load_deployment(&args.curve_program_path, args.curve_program_account)?;
    let collateral_definition = args.collateral_definition.account_id();
    let pools = args
        .pool
        .into_iter()
        .map(|pool| PoolFeeTarget {
            pool: pool.account_id(),
            collateral_definition,
        })
        .collect::<Vec<_>>();
    let hashes =
        launchpad_client::collect_pool_fees(&wallet, namespace, &curve, &pools, args.fee_payer)
            .await?;
    Ok(WriteSuccess::new(hashes))
}
