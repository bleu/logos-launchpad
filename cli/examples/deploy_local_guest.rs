use std::{env, fs, path::PathBuf};

use clap::Parser;
use eyre::{Result, WrapErr, ensure, eyre};
use lee::{AccountId, ProgramId};
use program_loader_core::{MAX_PROGRAM_SEGMENTS, segment_count};
use risc0_binfmt::ProgramBinary;
use wallet::{SUPPRESS_VERBOSE_PRINTS, WalletCore, program_facades::program_loader::ProgramLoader};

#[path = "../tests/local_node/wallet_config.rs"]
mod wallet_config;
use wallet_config::local_wallet_config;

#[derive(Parser)]
#[command(about = "Deploy a guest to a disposable standalone LEZ node")]
struct Args {
    /// Guest .bin file.
    guest: PathBuf,
    /// Funded public test account in the wallet (base58).
    #[arg(long)]
    payer: AccountId,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    local_wallet_config()?;
    ensure!(
        env::var_os(SUPPRESS_VERBOSE_PRINTS).is_some(),
        "set {SUPPRESS_VERBOSE_PRINTS}=1 for the disposable LEZ test"
    );
    let bytecode = fs::read(&args.guest)
        .wrap_err_with(|| format!("read guest binary {}", args.guest.display()))?;
    let binary =
        ProgramBinary::decode(&bytecode).map_err(|err| eyre!(err.into_boxed_dyn_error()))?;
    let count = segment_count(binary.user_elf);
    ensure!(
        (1..=MAX_PROGRAM_SEGMENTS).contains(&count),
        "guest needs {count} segments; expected 1 to {MAX_PROGRAM_SEGMENTS}"
    );
    let image_id: ProgramId = binary
        .compute_image_id()
        .map_err(|err| eyre!(err.into_boxed_dyn_error()))?
        .into();

    let mut wallet = WalletCore::from_env().map_err(|err| eyre!(err.into_boxed_dyn_error()))?;
    let (header, _) = wallet.create_new_account_public(None);
    let segments: Vec<_> = (0..count)
        .map(|_| wallet.create_new_account_public(None).0)
        .collect();
    wallet
        .store_persistent_data()
        .map_err(|err| eyre!(err.into_boxed_dyn_error()))?;

    let loader = ProgramLoader(&wallet);
    let account = loader
        .deploy(header, &segments, bytecode, true, Some(args.payer))
        .await
        .map_err(|err| eyre!(err.into_boxed_dyn_error()))?;
    let installed = loader
        .program(account)
        .await
        .map_err(|err| eyre!(err.into_boxed_dyn_error()))?
        .ok_or_else(|| eyre!("no program at {account}"))?;
    ensure!(installed.id() == image_id, "image ID mismatch at {account}");
    println!("program_account={account}");
    println!("program_account_hex={}", hex::encode(account.value()));
    Ok(())
}
