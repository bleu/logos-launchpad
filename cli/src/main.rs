//! The `launchpad` binary. Argument parsing belongs here; wallet handling,
//! account derivation, quoting, and transaction construction belong in
//! `launchpad-client`.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use std::{
    io::{self, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use clap::Parser;
use eyre::{Context, Result};
use launchpad_client::{
    FactorySale, NamespaceAction, ProgramHandle, PublicWriteWallet, WriteTransport,
    load_curve_config, submit_namespace_action,
};
use wallet::{WalletCore, WalletSequencerClient};

mod args;
mod distribution;
mod lifecycle;
mod sale_info;
mod trade;
mod write_result;

use args::{Cli, Command, ConfigArgs, DistributionCommand};
use write_result::WriteSuccess;

#[cfg(test)]
mod command_tests;
#[cfg(test)]
mod error_tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
enum ErrorCategory {
    #[error("slippage_floor")]
    SlippageFloor,
    #[error("sale_reserve_overshoot")]
    SaleReserveOvershoot,
    #[error("collateral_reserve_overshoot")]
    CollateralReserveOvershoot,
    #[error("general")]
    General,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let result = run_command(
        Cli::parse(),
        read_client,
        open_wallet,
        &mut io::stdout(),
        &mut io::stderr(),
    )
    .await;
    match result {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            eprintln!("Error writing command result: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn read_client() -> Result<impl WalletSequencerClient> {
    use wallet::{config::WalletConfig, utils::fetch_config_path};
    let path = fetch_config_path().map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))?;
    let config = match std::fs::File::open(&path) {
        Ok(file) => serde_json::from_reader::<_, WalletConfig>(io::BufReader::new(file)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(WalletConfig::default()),
        Err(error) => return Err(error).wrap_err_with(|| format!("opening {}", path.display())),
    }
    .wrap_err_with(|| format!("reading {}", path.display()))?;
    WalletCore::client_from_config(&config)
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))
}

fn open_wallet() -> Result<PublicWriteWallet<impl WalletSequencerClient>> {
    WalletCore::from_env_with_client(WriteTransport::new(read_client()?))
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))
        .context("opening the project wallet")
}

async fn run_command<R: WalletSequencerClient, C: WalletSequencerClient>(
    cli: Cli,
    read_client: impl FnOnce() -> Result<R>,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
    output: &mut impl Write,
    errors: &mut impl Write,
) -> io::Result<u8> {
    let json = cli.json;
    let public_write = matches!(
        cli.command,
        Command::Configure(_)
            | Command::UpdateFee(_)
            | Command::UpdateTreasury(_)
            | Command::TransferAdmin(_)
            | Command::RenounceAdmin(_)
            | Command::CreateSale(_)
            | Command::Withdraw(_)
            | Command::Graduate(_)
            | Command::Claim(_)
            | Command::CollectFees(_)
            | Command::Buy(_)
            | Command::BuyWithCollateral(_)
            | Command::Sell(_)
            | Command::Distribution {
                command: DistributionCommand::Initialize { .. }
                    | DistributionCommand::Remaining { .. },
            }
    );
    match run(json, cli, read_client, open_wallet, output).await {
        Ok(result) => {
            if let Some(result) = result {
                result.write(json, output)?;
            }
            Ok(0)
        }
        Err(error) => {
            use launchpad_client::ClientError;
            let (source, executed) = match error.downcast_ref::<ClientError>() {
                Some(ClientError::WriteInterrupted {
                    executed_hashes,
                    source,
                }) => (Some(source.as_ref()), executed_hashes.as_slice()),
                Some(source @ ClientError::CreationVerificationFailed { hash, .. }) => {
                    (Some(source), std::slice::from_ref(hash))
                }
                source => (source, &[][..]),
            };
            let write_failure = match source {
                Some(ClientError::PublicWritePending { hash, .. }) => {
                    let submission = if hash.is_some() {
                        "acknowledged"
                    } else {
                        "unknown"
                    };
                    Some(("pending", submission, *hash, 3))
                }
                Some(ClientError::PublicWriteReverted { hash }) => {
                    Some(("reverted", "acknowledged", Some(*hash), 1))
                }
                Some(ClientError::CreationVerificationFailed { hash, .. }) => {
                    Some(("verification_failed", "acknowledged", Some(*hash), 1))
                }
                Some(ClientError::PublicWriteRejected(_)) => {
                    Some(("rejected", "not_sent", None, 1))
                }
                _ if public_write => Some(("preparation_failed", "not_sent", None, 1)),
                _ => None,
            };
            if let Some((status, submission, hash, code)) = write_failure {
                let hash = hash.map(hex::encode);
                let executed = executed.iter().map(hex::encode).collect::<Vec<_>>();
                if json {
                    let (category, message) = if executed.is_empty() {
                        (command_error_category(&error), error.to_string())
                    } else {
                        let cause = source.expect("interrupted write has a cause");
                        (cause.into(), cause.to_string())
                    };
                    let view = serde_json::json!({
                        "status": status,
                        "submission": submission,
                        "transaction_hash": hash,
                        "executed_transaction_hashes": executed,
                        "error": { "category": category, "message": message },
                    });
                    writeln!(output, "{view}")?;
                } else {
                    write!(errors, "{status}: submission={submission}")?;
                    if let Some(hash) = hash {
                        write!(errors, " tx_hash={hash}")?;
                    }
                    writeln!(errors, ": {error:#}")?;
                    for executed_hash in executed {
                        writeln!(errors, "executed tx_hash={executed_hash}")?;
                    }
                }
                return Ok(code);
            }
            if json {
                let view = serde_json::json!({
                    "status": "error",
                    "error": {
                        "category": command_error_category(&error),
                        "message": error.to_string(),
                    },
                });
                writeln!(output, "{view}")?;
            } else {
                writeln!(errors, "Error: {error:#}")?;
            }
            Ok(1)
        }
    }
}

async fn run<R: WalletSequencerClient, C: WalletSequencerClient>(
    json: bool,
    cli: Cli,
    read_client: impl FnOnce() -> Result<R>,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
    output: &mut impl Write,
) -> Result<Option<WriteSuccess>> {
    cli.validate_route()?;
    let router = cli.authority_router_path.as_deref();
    if let Command::Distribution { command } = &cli.command
        && distribution::run_offline(command, json, output)?
    {
        return Ok(None);
    }
    let namespace = cli
        .namespace
        .ok_or_else(|| eyre::eyre!("--namespace is required; use the stable namespace AccountId"))?
        .account_id();
    let receipt = match cli.command {
        Command::Distribution { command } => {
            distribution::run(namespace, command, open_wallet).await?
        }
        Command::CreateSale(args) => {
            lifecycle::create_sale(namespace, *args, router, open_wallet).await?
        }
        Command::CollectFees(args) => lifecycle::collect_fees(namespace, args, open_wallet).await?,
        Command::Withdraw(args) => {
            lifecycle::withdraw_factory_proceeds(namespace, args, router, open_wallet).await?
        }
        Command::Graduate(args) => lifecycle::graduate(namespace, args, open_wallet).await?,
        Command::Claim(args) => {
            lifecycle::claim_creator_allocation(namespace, args, open_wallet).await?
        }
        Command::Buy(args) => trade::buy(namespace, args, open_wallet).await?,
        Command::BuyWithCollateral(args) => {
            trade::buy_with_collateral(namespace, args, open_wallet).await?
        }
        Command::PrivateBuy(args) => {
            return trade::private_buy(namespace, args).await.map(|()| None);
        }
        Command::Sell(args) => trade::sell(namespace, args, open_wallet).await?,
        Command::Price(args) => return trade::price(namespace, json, args).await.map(|()| None),
        Command::SaleInfo(args) => {
            sale_info::show(namespace, args, &read_client()?, json, output).await?;
            return Ok(None);
        }
        Command::Configure(args) => configure(namespace, args, open_wallet).await?,
        Command::CreateAuthority(_) | Command::TransferAuthority(_) => unreachable!(),
        Command::NamespaceInfo(args) => {
            let program = load_deployment(&args.curve_program_path, args.curve_program_account)?;
            let wallet = open_wallet()?;
            let config = load_curve_config(namespace, &wallet, program.account_id).await?;
            writeln!(
                output,
                "{}",
                serde_json::json!({"namespace": namespace.to_string(), "protocol_fee_bps": config.protocol_fee_bps, "treasury": config.treasury.to_string()})
            )?;
            return Ok(None);
        }
        Command::UpdateFee(args) => {
            namespace_write(
                namespace,
                &args.authority.curve_program_path,
                args.authority.curve_program_account,
                NamespaceAction::UpdateFee {
                    admin: args.authority.admin,
                    protocol_fee_bps: args.protocol_fee_bps,
                },
                open_wallet,
            )
            .await?
        }
        Command::UpdateTreasury(args) => {
            namespace_write(
                namespace,
                &args.authority.curve_program_path,
                args.authority.curve_program_account,
                NamespaceAction::UpdateTreasury {
                    admin: args.authority.admin,
                    treasury: args.treasury,
                },
                open_wallet,
            )
            .await?
        }
        Command::TransferAdmin(args) => {
            namespace_write(
                namespace,
                &args.authority.curve_program_path,
                args.authority.curve_program_account,
                NamespaceAction::TransferAdmin {
                    admin: args.authority.admin,
                    new_admin: args.new_admin,
                },
                open_wallet,
            )
            .await?
        }
        Command::RenounceAdmin(args) => {
            if !args.confirm_renounce {
                eyre::bail!(
                    "renounce-admin permanently removes the namespace administrator; pass --confirm-renounce to proceed"
                );
            }
            namespace_write(
                namespace,
                &args.authority.curve_program_path,
                args.authority.curve_program_account,
                NamespaceAction::RenounceAdmin {
                    admin: args.authority.admin,
                },
                open_wallet,
            )
            .await?
        }
    };
    Ok(Some(receipt))
}

fn command_error_category(error: &eyre::Report) -> &'static str {
    match error_category(error) {
        ErrorCategory::General => error
            .downcast_ref::<launchpad_client::ClientError>()
            .map_or("general", Into::into),
        category => category.into(),
    }
}

fn error_category(error: &eyre::Report) -> ErrorCategory {
    if let Some(category) = error.downcast_ref::<ErrorCategory>() {
        return *category;
    }
    let Some(error) = error.downcast_ref::<launchpad_client::ClientError>() else {
        return ErrorCategory::General;
    };
    use launchpad_client::ClientError;
    use pool::SwapError;
    match error {
        ClientError::QuoteBuy(SwapError::SlippageExceeded)
        | ClientError::QuoteBuyWithCollateral(SwapError::SlippageExceeded)
        | ClientError::QuoteSell(SwapError::SlippageExceeded) => ErrorCategory::SlippageFloor,
        ClientError::QuoteBuy(SwapError::InsufficientRealOutputReserve)
        | ClientError::QuoteBuyWithCollateral(SwapError::InsufficientRealOutputReserve) => {
            ErrorCategory::SaleReserveOvershoot
        }
        ClientError::QuoteSell(SwapError::InsufficientRealOutputReserve) => {
            ErrorCategory::CollateralReserveOvershoot
        }
        _ => ErrorCategory::General,
    }
}

async fn configure<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    args: ConfigArgs,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    namespace_write(
        namespace,
        &args.curve_program_path,
        args.curve_program_account,
        NamespaceAction::Initialize {
            protocol_fee_bps: args.protocol_fee_bps,
            treasury: args.treasury,
        },
        open_wallet,
    )
    .await
}

async fn namespace_write<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    program_path: &Path,
    program_account: lee::AccountId,
    action: NamespaceAction,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    let program = load_deployment(program_path, program_account)?;
    let wallet = open_wallet()?;
    let receipt = submit_namespace_action(&wallet, &program, namespace, action).await?;
    Ok(WriteSuccess::namespace(receipt))
}

fn factory_sale(
    namespace: lee::AccountId,
    factory_program: &ProgramHandle,
    curve_program: &ProgramHandle,
    launch_salt: [u8; 32],
    collateral_definition: lee::AccountId,
) -> FactorySale {
    FactorySale {
        namespace,
        factory_program_id: factory_program.account_id,
        curve_program_id: curve_program.account_id,
        launch_salt,
        collateral_definition,
    }
}

fn load_deployment(
    path: &Path,
    account_id: lee::AccountId,
) -> launchpad_client::Result<ProgramHandle> {
    Ok(ProgramHandle {
        account_id,
        image: launchpad_client::load_program(path)?,
    })
}

fn current_time() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("reading the current time")?
        .as_secs())
}
