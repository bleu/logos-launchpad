use launchpad_client::PublicWriteWallet;
use lee::AccountId;

use crate::{args::Cli, run_command};

#[path = "../../verify/fixtures/launchpad_transport.rs"]
mod launchpad_transport;
#[path = "../../verify/fixtures/lbp_sale.rs"]
mod lbp_sale;

mod creation;
mod curve_fixture;
mod lifecycle;
mod namespace_writes;
mod reads;

use launchpad_transport::FixtureTransport;

const SALT: [u8; 32] = [0x11; 32];

fn id(byte: u8) -> AccountId {
    AccountId::new([byte; 32])
}

fn sale_args(engine: &str) -> Vec<String> {
    [
        "launchpad".into(),
        "--namespace".into(),
        id(1).to_string(),
        "--json".into(),
        "sale-info".into(),
        "--engine".into(),
        engine.into(),
        "--launch-salt".into(),
        hex::encode(SALT),
        "--collateral-definition".into(),
        id(4).to_string(),
        "--factory-program-account".into(),
        id(2).to_string(),
        "--engine-program-account".into(),
        id(3).to_string(),
    ]
    .into()
}

async fn run_sale_info(
    cli: Cli,
    read_client: impl FnOnce() -> eyre::Result<FixtureTransport>,
    output: &mut impl std::io::Write,
    errors: &mut impl std::io::Write,
) -> std::io::Result<u8> {
    run_command(
        cli,
        read_client,
        || -> eyre::Result<PublicWriteWallet<FixtureTransport>> {
            panic!("sale-info must not open a wallet")
        },
        output,
        errors,
    )
    .await
}
