use eyre::{Context, Result};
use launchpad_client::{
    ExactInputTradeTerms, ExactOutputTradeTerms, PrivateBuyRequest, PublicWriteWallet,
    load_clock_block_id, load_curve_config, load_factory_pool, prepare_buy_invocation,
    prepare_buy_with_collateral_invocation, prepare_sell_invocation, quote_buy,
    quote_buy_with_collateral, quote_sell, submit_public_invocation,
};
use serde::Serialize;
use wallet::{WalletCore, WalletSequencerClient};

use super::{
    args::{BuyArgs, BuyWithCollateralArgs, PriceArgs, PrivateBuyArgs, SellArgs},
    current_time, factory_sale, load_deployment,
    write_result::WriteSuccess,
};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum QuoteMode {
    Buy,
    Sell,
}

#[derive(Serialize)]
struct PriceQuote {
    namespace: String,
    protocol_fee_bps: u16,
    treasury: String,
    kind: QuoteMode,
    amount_in: u128,
    raw_amount_out: u128,
    amount_out: u128,
    protocol_fee: u128,
}

pub(super) async fn price(namespace: lee::AccountId, json: bool, args: PriceArgs) -> Result<()> {
    let factory_program = load_deployment(
        &args.launch.factory_program_path,
        args.launch.factory_program_account,
    )?;
    let curve_program = load_deployment(
        &args.launch.curve_program_path,
        args.launch.curve_program_account,
    )?;
    let sale = factory_sale(
        namespace,
        &factory_program,
        &curve_program,
        args.launch.launch.launch_salt,
        args.launch.collateral_definition.account_id(),
    );
    let wallet = WalletCore::from_env()
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))
        .context("opening the project wallet")?;
    let pool = load_factory_pool(&wallet, sale).await?;
    let config = load_curve_config(namespace, &wallet, curve_program.account_id).await?;
    let now = current_time()?;
    let observed_block = if !args.sell && pool.block_allocation.limit.is_some() {
        Some(load_clock_block_id(&wallet).await?)
    } else {
        None
    };
    let (kind, quote) = match (args.tokens, args.collateral) {
        (Some(tokens), None) if args.sell => {
            (QuoteMode::Sell, quote_sell(&pool, &config, tokens, now)?)
        }
        (Some(tokens), None) => (
            QuoteMode::Buy,
            quote_buy(&pool, &config, tokens, now, observed_block)?,
        ),
        (None, Some(collateral)) => (
            QuoteMode::Buy,
            quote_buy_with_collateral(&pool, &config, collateral, now, observed_block)?,
        ),
        _ => eyre::bail!("provide exactly one of --tokens or --collateral"),
    };
    let output = PriceQuote {
        namespace: namespace.to_string(),
        protocol_fee_bps: config.protocol_fee_bps,
        treasury: config.treasury.to_string(),
        kind,
        amount_in: quote.amount_in,
        raw_amount_out: quote.raw_amount_out,
        amount_out: quote.amount_out,
        protocol_fee: quote.protocol_fee,
    };
    if json {
        println!("{}", serde_json::to_string(&output)?);
    } else {
        let kind = match output.kind {
            QuoteMode::Buy => "buy",
            QuoteMode::Sell => "sell",
        };
        println!(
            "{kind} quote: namespace={} fee_bps={} treasury={} input={} raw_output={} output={} protocol_fee={}",
            output.namespace,
            output.protocol_fee_bps,
            output.treasury,
            output.amount_in,
            output.raw_amount_out,
            output.amount_out,
            output.protocol_fee
        );
    }
    Ok(())
}

pub(super) async fn buy<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: BuyArgs,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let factory_program = load_deployment(
        &args.trade.factory_program_path,
        args.trade.factory_program_account,
    )?;
    let curve_program = load_deployment(
        &args.trade.curve_program_path,
        args.trade.curve_program_account,
    )?;
    let sale = factory_sale(
        namespace,
        &factory_program,
        &curve_program,
        args.trade.launch.launch_salt,
        args.trade.collateral_definition.account_id(),
    );
    let participant = args.trade.participant.account_id();
    let wallet = open_wallet()?;
    let config = load_curve_config(namespace, &wallet, curve_program.account_id).await?;
    let pool = load_factory_pool(&wallet, sale).await?;
    let now = current_time()?;
    let observed_block = if pool.block_allocation.limit.is_some() {
        Some(load_clock_block_id(&wallet).await?)
    } else {
        None
    };
    let invocation = prepare_buy_invocation(
        sale,
        participant,
        &pool,
        &config,
        ExactOutputTradeTerms {
            amount_out: args.tokens,
            max_amount_in: args.max_collateral,
        },
        None,
        now,
        observed_block,
    )?;
    let transaction_hash = submit_public_invocation(&wallet, &curve_program, invocation).await?;
    Ok(WriteSuccess::single(
        transaction_hash,
        Some(args.trade.launch.launch_salt),
    ))
}

pub(super) async fn buy_with_collateral<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: BuyWithCollateralArgs,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let factory_program = load_deployment(
        &args.trade.factory_program_path,
        args.trade.factory_program_account,
    )?;
    let curve_program = load_deployment(
        &args.trade.curve_program_path,
        args.trade.curve_program_account,
    )?;
    let sale = factory_sale(
        namespace,
        &factory_program,
        &curve_program,
        args.trade.launch.launch_salt,
        args.trade.collateral_definition.account_id(),
    );
    let participant = args.trade.participant.account_id();
    let wallet = open_wallet()?;
    let config = load_curve_config(namespace, &wallet, curve_program.account_id).await?;
    let pool = load_factory_pool(&wallet, sale).await?;
    let now = current_time()?;
    let observed_block = if pool.block_allocation.limit.is_some() {
        Some(load_clock_block_id(&wallet).await?)
    } else {
        None
    };
    let invocation = prepare_buy_with_collateral_invocation(
        sale,
        participant,
        &pool,
        &config,
        ExactInputTradeTerms {
            amount_in: args.collateral,
            min_amount_out: args.min_tokens,
        },
        None,
        now,
        observed_block,
    )?;
    let transaction_hash = submit_public_invocation(&wallet, &curve_program, invocation).await?;
    Ok(WriteSuccess::single(
        transaction_hash,
        Some(args.trade.launch.launch_salt),
    ))
}

pub(super) async fn private_buy(_namespace: lee::AccountId, args: PrivateBuyArgs) -> Result<()> {
    launchpad_client::validate_private_buy_request(PrivateBuyRequest {
        amount_out: args.tokens,
        max_collateral_in: args.max_collateral,
        from_private: args.from_private,
        to_private: args.to_private,
        gas_reserve: args.gas_reserve,
    })?;
    Err(launchpad_client::ClientError::UnsupportedPrivateRoute.into())
}

pub(super) async fn sell<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: SellArgs,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let factory_program = load_deployment(
        &args.trade.factory_program_path,
        args.trade.factory_program_account,
    )?;
    let curve_program = load_deployment(
        &args.trade.curve_program_path,
        args.trade.curve_program_account,
    )?;
    let sale = factory_sale(
        namespace,
        &factory_program,
        &curve_program,
        args.trade.launch.launch_salt,
        args.trade.collateral_definition.account_id(),
    );
    let participant = args.trade.participant.account_id();
    let wallet = open_wallet()?;
    let config = load_curve_config(namespace, &wallet, curve_program.account_id).await?;
    let pool = load_factory_pool(&wallet, sale).await?;
    let now = current_time()?;
    let invocation = prepare_sell_invocation(
        sale,
        participant,
        &pool,
        &config,
        ExactInputTradeTerms {
            amount_in: args.tokens,
            min_amount_out: args.min_collateral,
        },
        now,
    )?;
    let transaction_hash = submit_public_invocation(&wallet, &curve_program, invocation).await?;
    Ok(WriteSuccess::single(
        transaction_hash,
        Some(args.trade.launch.launch_salt),
    ))
}
