use std::path::Path;

use clap::Parser;

use super::create_sale;
use crate::{
    args::{Cli, Command},
    open_wallet,
};

fn creation_arguments(file: &Path) -> Vec<String> {
    let mut args: Vec<String> = [
        "launchpad",
        "create-sale",
        "--engine",
        "bonding-curve",
        "--trade-mode",
        "buy-only",
        "--name",
        "Launch",
        "--uri",
        "https://example.invalid/token",
        "--sale-reserve",
        "800",
        "--creator-allocation",
        "100",
        "--virtual-token-reserve",
        "2000",
        "--virtual-collateral-reserve",
        "100",
        "--collateral-definition",
        "11111111111111111111111111111111",
        "--creator",
        "Public/11111111111111111111111111111111",
        "--factory-program-path",
        "missing-factory.bin",
        "--curve-program-path",
        "missing-curve.bin",
        "--engine-program-account",
        "11111111111111111111111111111111",
        "--graduation-launch-bps",
        "5000",
        "--graduation-collateral-bps",
        "5000",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    args.extend([
        "--launch-salt".into(),
        hex::encode([3; 32]),
        "--factory-program-account".into(),
        lee::AccountId::new([7; 32]).to_string(),
        "--graduation-hook-program-id".into(),
        lee::AccountId::new([9; 32]).to_string(),
        "--distribution-configuration".into(),
        file.to_str().unwrap().into(),
    ]);
    args
}

fn configuration(destination: lee::AccountId) -> launchpad_client::distribution::Configuration {
    use launchpad_client::distribution::{Configuration, Rule, VERSION};
    Configuration {
        version: VERSION,
        launch_rules: vec![Rule {
            destination,
            bps: 10000,
        }],
        collateral_rules: vec![Rule {
            destination,
            bps: 10000,
        }],
    }
}

#[tokio::test]
async fn reserve_input_is_saved_and_supply_overflow_fails_before_io() {
    let mut arguments = creation_arguments(Path::new("missing-rules.json"));
    arguments.extend(["--graduation-reserve".into(), "200".into()]);
    let Command::CreateSale(args) = Cli::try_parse_from(&arguments).unwrap().command else {
        panic!("create sale")
    };
    let request = args.sale_request();
    assert_eq!(request.graduation_reserve, 200);
    assert_eq!(request.total_supply().unwrap(), 1100);
    *arguments.last_mut().unwrap() = u128::MAX.to_string();
    let Command::CreateSale(args) = Cli::try_parse_from(arguments).unwrap().command else {
        panic!("create sale")
    };
    let error = create_sale(lee::AccountId::new([40; 32]), *args, None, open_wallet)
        .await
        .unwrap_err();
    assert!(matches!(
        error.downcast_ref::<launchpad_client::ClientError>(),
        Some(launchpad_client::ClientError::FactoryParameters(_))
    ));
}

#[tokio::test]
async fn creation_rejects_custody_before_loading_programs_or_wallet() {
    use launchpad_client::distribution::{DistributionError, GraduationIntentKind, initialize};
    let namespace = lee::AccountId::new([40; 32]);
    let custody = initialize(
        lee::AccountId::new([9; 32]),
        lee::AccountId::new([7; 32]),
        namespace,
        [3; 32],
        GraduationIntentKind::Curve,
    )
    .destination
    .account_id;
    for side in 0..2 {
        let mut config = configuration(lee::AccountId::new([80; 32]));
        if side == 0 {
            config.launch_rules[0].destination = custody;
        } else {
            config.collateral_rules[0].destination = custody;
        }
        let file = std::env::temp_dir().join(format!(
            "launchpad-invalid-distribution-{}-{side}.json",
            std::process::id()
        ));
        std::fs::write(&file, serde_json::to_vec(&config).unwrap()).unwrap();
        let cli = Cli::try_parse_from(creation_arguments(&file)).unwrap();
        cli.validate_route().unwrap();
        let Command::CreateSale(args) = cli.command else {
            panic!("create sale")
        };
        let result = create_sale(namespace, *args, None, open_wallet).await;
        std::fs::remove_file(file).unwrap();
        assert!(matches!(
            result
                .unwrap_err()
                .downcast_ref::<launchpad_client::ClientError>(),
            Some(launchpad_client::ClientError::Distribution(
                DistributionError::InvalidConfiguration
            ))
        ));
    }
}

#[test]
fn creation_saves_valid_rules_and_requires_one_configuration_source() {
    let config = configuration(lee::AccountId::new([80; 32]));
    let file = std::env::temp_dir().join(format!(
        "launchpad-valid-distribution-{}.json",
        std::process::id()
    ));
    std::fs::write(&file, serde_json::to_vec(&config).unwrap()).unwrap();
    let arguments = creation_arguments(&file);
    let cli = Cli::try_parse_from(&arguments).unwrap();
    let Command::CreateSale(args) = cli.command else {
        panic!("create sale")
    };
    let request = args.prepared_sale_request(args.sale(lee::AccountId::new([40; 32])));
    std::fs::remove_file(file).unwrap();
    let saved = request.unwrap().graduation_hook.unwrap();
    assert_eq!(saved.program_id, lee::AccountId::new([9; 32]));
    assert_eq!(
        launchpad_client::distribution::Configuration::decode(&saved.configuration).unwrap(),
        config
    );

    let mut conflicting = arguments.clone();
    conflicting.extend(["--graduation-hook-configuration".into(), "00".into()]);
    assert!(Cli::try_parse_from(conflicting).is_err());
    let mut missing_program = arguments;
    let index = missing_program
        .iter()
        .position(|arg| arg == "--graduation-hook-program-id")
        .unwrap();
    missing_program.drain(index..index + 2);
    assert!(Cli::try_parse_from(missing_program).is_err());
}

#[test]
fn offline_distribution_commands_parse_without_wallet_or_namespace() {
    for args in [
        vec![
            "launchpad",
            "distribution",
            "encode",
            "--file",
            "rules.json",
        ],
        vec![
            "launchpad",
            "distribution",
            "preview",
            "--file",
            "rules.json",
            "--launch-balance",
            "101",
            "--collateral-balance",
            "0",
        ],
    ] {
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.namespace.is_none());
        cli.validate_route().unwrap();
    }
}
