use std::{io::Write, path::Path};

use eyre::Result;
use launchpad_client::{PublicWriteWallet, submit_public_invocation};
use wallet::WalletSequencerClient;

use super::{
    args::{DistributionCommand, DistributionKind},
    load_deployment,
    write_result::WriteSuccess,
};

pub(super) fn run_offline(
    command: &DistributionCommand,
    json: bool,
    output: &mut impl Write,
) -> Result<bool> {
    match command {
        DistributionCommand::Encode { file } => {
            let config = read_distribution_configuration(file)?;
            let bytes = launchpad_client::distribution::encode_configuration(&config)?;
            let encoded = hex::encode(bytes);
            if json {
                writeln!(
                    output,
                    "{}",
                    serde_json::json!({ "configuration": encoded })
                )?;
            } else {
                writeln!(output, "{encoded}")?;
            }
            Ok(true)
        }
        DistributionCommand::Preview {
            file,
            launch_balance,
            collateral_balance,
        } => {
            let config = read_distribution_configuration(file)?;
            let plan = config
                .plan([*launch_balance, *collateral_balance])
                .map_err(|error| eyre::eyre!("{error:?}"))?;
            writeln!(output, "{}", serde_json::to_string_pretty(&plan)?)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub(super) fn read_distribution_configuration(
    path: &Path,
) -> Result<launchpad_client::distribution::Configuration> {
    let config = serde_json::from_slice(&std::fs::read(path)?)?;
    launchpad_client::distribution::encode_configuration(&config)?;
    Ok(config)
}

pub(super) async fn run<C: WalletSequencerClient>(
    namespace: lee::AccountId,
    command: DistributionCommand,
    open_wallet: impl FnOnce() -> Result<PublicWriteWallet<C>>,
) -> Result<WriteSuccess> {
    use launchpad_client::distribution;
    let (identity, kind) = match command {
        DistributionCommand::Initialize { adapter, kind } => (adapter, Some(kind)),
        DistributionCommand::Remaining { adapter } => (adapter, None),
        _ => unreachable!("offline commands handled before wallet setup"),
    };
    let adapter = load_deployment(
        &identity.adapter_program_path,
        identity.adapter_program_account,
    )?;
    let wallet = open_wallet()?;
    let mut call = if let Some(kind) = kind {
        let kind = match kind {
            DistributionKind::Curve => distribution::GraduationIntentKind::Curve,
            DistributionKind::FixedPrice => distribution::GraduationIntentKind::FixedPrice,
            DistributionKind::Weighted => distribution::GraduationIntentKind::Weighted,
        };
        distribution::initialize(
            adapter.account_id,
            identity.factory_program_account,
            namespace,
            identity.launch.launch_salt,
            kind,
        )
    } else {
        let state = distribution::load_state(
            &wallet,
            adapter.account_id,
            identity.factory_program_account,
            namespace,
            identity.launch.launch_salt,
        )
        .await?;
        distribution::distribute_remaining(adapter.account_id, &state)
            .map_err(|error| eyre::eyre!("{error:?}"))?
    };
    call.fee_payer = Some(identity.fee_payer);
    let transaction_hash = submit_public_invocation(&wallet, &adapter, call).await?;
    Ok(WriteSuccess::single(
        transaction_hash,
        Some(identity.launch.launch_salt),
    ))
}
