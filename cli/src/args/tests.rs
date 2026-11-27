use clap::{CommandFactory, Parser};
use launchpad_client::AccountLocation;

use super::{
    BuyArgs, Cli, Command, parse_account_location, parse_private_account_id,
    parse_public_account_id,
};

#[test]
fn create_authority_requires_a_public_fee_payer() {
    let command = [
        "launchpad",
        "create-authority",
        "--token-program-path",
        "token.bin",
        "--name",
        "Creator",
        "--uri",
        "https://example.invalid/creator",
    ];
    let error = Cli::try_parse_from(command).unwrap_err();
    assert_eq!(
        error.kind(),
        clap::error::ErrorKind::MissingRequiredArgument
    );
    assert!(error.to_string().contains("--fee-payer"));
    for payer in [
        "11111111111111111111111111111111",
        "Public/11111111111111111111111111111111",
    ] {
        let cli = Cli::try_parse_from(command.into_iter().chain(["--fee-payer", payer])).unwrap();
        let Command::CreateAuthority(args) = cli.command else {
            panic!("expected create-authority");
        };
        assert_eq!(args.fee_payer, parse_public_account_id(payer).unwrap());
    }
    assert!(
        Cli::try_parse_from(
            command
                .into_iter()
                .chain(["--fee-payer", "Private/11111111111111111111111111111111",])
        )
        .is_err()
    );
}

#[test]
fn legacy_authority_command_cannot_claim_to_create_a_launch_role() {
    let cli = Cli::try_parse_from([
        "launchpad",
        "create-authority",
        "--token-program-path",
        "token.bin",
        "--fee-payer",
        "11111111111111111111111111111111",
        "--name",
        "Creator",
        "--uri",
        "https://example.invalid/creator",
    ])
    .unwrap();
    assert!(
        cli.validate_route()
            .unwrap_err()
            .to_string()
            .contains("role authority")
    );
}

#[test]
fn private_nft_transfer_is_rejected_before_wallet_access() {
    let cli = Cli::try_parse_from([
        "launchpad",
        "transfer-authority",
        "--token-program-path",
        "token.bin",
        "--definition",
        "11111111111111111111111111111111",
        "--from",
        "Public/11111111111111111111111111111111",
        "--to",
        "Private/11111111111111111111111111111111",
    ])
    .unwrap();
    let error = cli.validate_route().unwrap_err();
    assert!(matches!(
        error.downcast_ref::<launchpad_client::ClientError>(),
        Some(launchpad_client::ClientError::UnsupportedPrivateAuthority)
    ));
}

#[test]
fn private_creator_lifecycle_is_rejected_before_wallet_access() {
    let salt = "11".repeat(32);
    for command in ["withdraw", "claim"] {
        let cli = Cli::try_parse_from([
            "launchpad",
            command,
            "--launch-salt",
            &salt,
            "--collateral-definition",
            "11111111111111111111111111111111",
            "--creator",
            "Private/11111111111111111111111111111111",
            "--factory-program-path",
            "factory.bin",
            "--factory-program-account",
            "11111111111111111111111111111111",
            "--curve-program-path",
            "curve.bin",
            "--curve-program-account",
            "11111111111111111111111111111111",
        ])
        .unwrap();
        assert!(matches!(
            cli.validate_route()
                .unwrap_err()
                .downcast_ref::<launchpad_client::ClientError>(),
            Some(launchpad_client::ClientError::UnsupportedPrivateAuthority)
        ));
    }
}

#[test]
fn private_sale_selectors_cannot_alias_public_actors() {
    let salt = "11".repeat(32);
    let public = "11111111111111111111111111111111";
    let private = "Private/11111111111111111111111111111111";
    for (namespace, collateral) in [(private, public), (public, private)] {
        let cli = Cli::try_parse_from([
            "launchpad",
            "--namespace",
            namespace,
            "buy",
            "--launch-salt",
            &salt,
            "--tokens",
            "1",
            "--max-collateral",
            "2",
            "--collateral-definition",
            collateral,
            "--participant",
            public,
            "--factory-program-path",
            "factory.bin",
            "--factory-program-account",
            public,
            "--curve-program-path",
            "curve.bin",
            "--curve-program-account",
            public,
        ])
        .unwrap();
        assert!(cli.validate_route().is_err());
    }
}

#[test]
fn public_trade_rejects_private_participant_before_wallet_access() {
    let salt = "11".repeat(32);
    let cli = Cli::try_parse_from([
        "launchpad",
        "buy",
        "--launch-salt",
        &salt,
        "--tokens",
        "1",
        "--max-collateral",
        "2",
        "--collateral-definition",
        "11111111111111111111111111111111",
        "--participant",
        "Private/11111111111111111111111111111111",
        "--factory-program-path",
        "factory.bin",
        "--factory-program-account",
        "11111111111111111111111111111111",
        "--curve-program-path",
        "curve.bin",
        "--curve-program-account",
        "11111111111111111111111111111111",
    ])
    .unwrap();
    assert!(cli.validate_route().is_err());
}

#[test]
fn legacy_authority_syntax_and_private_namespace_admin_are_distinct() {
    assert!(
        Cli::try_parse_from([
            "launchpad",
            "create-authority",
            "--token-program-path",
            "token.bin",
            "--fee-payer",
            "11111111111111111111111111111111",
            "--name",
            "Admin",
            "--uri",
            "https://example.invalid/admin"
        ])
        .is_ok()
    );
    assert!(
        Cli::try_parse_from([
            "launchpad",
            "--namespace",
            "11111111111111111111111111111111",
            "--authority-router-path",
            "private_authority.bin",
            "configure",
            "--curve-program-path",
            "curve.bin",
            "--curve-program-account",
            "11111111111111111111111111111111",
            "--admin",
            "Private/11111111111111111111111111111111",
            "--treasury",
            "Public/11111111111111111111111111111111"
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "launchpad",
            "transfer-authority",
            "--token-program-path",
            "token.bin",
            "--definition",
            "11111111111111111111111111111111",
            "--from",
            "Public/11111111111111111111111111111111",
            "--to",
            "Private/11111111111111111111111111111111"
        ])
        .is_ok()
    );
}

#[test]
fn namespace_admin_commands_accept_public_initialization_and_renunciation() {
    let cli = Cli::try_parse_from([
        "launchpad",
        "--namespace",
        "Public/11111111111111111111111111111111",
        "configure",
        "--curve-program-path",
        "curve.bin",
        "--curve-program-account",
        "11111111111111111111111111111111",
        "--admin",
        "Public/11111111111111111111111111111111",
        "--protocol-fee-bps",
        "0",
        "--treasury",
        "Public/11111111111111111111111111111111",
    ])
    .unwrap();
    assert!(matches!(cli.namespace, Some(AccountLocation::Public(_))));
    assert!(cli.validate_route().is_ok());
    let Command::Configure(config) = cli.command else {
        panic!("configure command")
    };
    assert_eq!(config.admin, cli.namespace.unwrap().account_id());
    for command in ["namespace-info", "renounce-admin"] {
        let mut args = vec![
            "launchpad",
            "--namespace",
            "Public/11111111111111111111111111111111",
            command,
            "--curve-program-path",
            "curve.bin",
            "--curve-program-account",
            "11111111111111111111111111111111",
        ];
        if command == "renounce-admin" {
            args.extend(["--admin", "Public/11111111111111111111111111111111"]);
        }
        assert!(Cli::try_parse_from(args).is_ok());
    }
}

#[test]
fn launchpad_exposes_creator_and_participant_commands() {
    let help = Cli::command().render_help().to_string();
    for command in [
        "create-sale",
        "withdraw",
        "claim",
        "buy",
        "sell",
        "price",
        "sale-info",
    ] {
        assert!(help.contains(command), "missing command: {command}");
    }
    assert!(
        !Cli::command()
            .get_subcommands()
            .any(|command| command.get_name() == "close")
    );
}

#[test]
fn buy_requires_exact_output_and_collateral_cap() {
    let cli = Cli::try_parse_from([
        "launchpad",
        "buy",
        "--launch-salt",
        &"11".repeat(32),
        "--tokens",
        "25",
        "--max-collateral",
        "100",
        "--collateral-definition",
        "11111111111111111111111111111111",
        "--participant",
        "Public/11111111111111111111111111111111",
        "--factory-program-path",
        "factory.bin",
        "--factory-program-account",
        "11111111111111111111111111111111",
        "--curve-program-path",
        "curve.bin",
        "--curve-program-account",
        "11111111111111111111111111111111",
    ])
    .expect("buy command should parse");
    assert!(matches!(
        cli.command,
        Command::Buy(BuyArgs {
            tokens: 25,
            max_collateral: 100,
            ..
        })
    ));
}

#[test]
fn private_buy_requires_private_source_and_explicit_gas_reserve() {
    let command = Cli::command();
    let private_buy = command
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "private-buy")
        .expect("private buy command should exist");

    assert!(
        private_buy
            .get_arguments()
            .any(|argument| argument.get_long() == Some("from-private")
                && argument.is_required_set())
    );
    assert!(
        private_buy.get_arguments().any(
            |argument| argument.get_long() == Some("gas-reserve") && argument.is_required_set()
        )
    );
    assert!(
        private_buy
            .get_arguments()
            .any(|argument| argument.get_long() == Some("to-private")
                && argument.is_required_set())
    );
    assert!(
        private_buy
            .get_arguments()
            .any(
                |argument| argument.get_long() == Some("private-buy-program-path")
                    && argument.is_required_set()
            )
    );
}

#[test]
fn selectors_parse_at_the_cli_boundary() {
    assert!(matches!(
        parse_account_location("11111111111111111111111111111111"),
        Ok(AccountLocation::Public(_))
    ));
    assert!(matches!(
        parse_account_location("Private/11111111111111111111111111111111"),
        Ok(AccountLocation::Private(_))
    ));
    assert!(parse_account_location("Unknown/11111111111111111111111111111111").is_err());
    assert!(parse_private_account_id("11111111111111111111111111111111").is_ok());
    assert!(parse_private_account_id("Public/11111111111111111111111111111111").is_err());
}

#[test]
fn sell_accepts_the_factory_and_curve_context() {
    let cli = Cli::try_parse_from([
        "launchpad",
        "sell",
        "--launch-salt",
        &"11".repeat(32),
        "--tokens",
        "25",
        "--min-collateral",
        "10",
        "--collateral-definition",
        "11111111111111111111111111111111",
        "--participant",
        "Public/11111111111111111111111111111111",
        "--factory-program-path",
        "factory.bin",
        "--factory-program-account",
        "11111111111111111111111111111111",
        "--curve-program-path",
        "curve.bin",
        "--curve-program-account",
        "11111111111111111111111111111111",
    ])
    .expect("sell command should parse");
    assert!(matches!(cli.command, Command::Sell(_)));
}

#[test]
fn collateral_buy_accepts_a_collateral_budget_and_token_floor() {
    let cli = Cli::try_parse_from([
        "launchpad",
        "buy-with-collateral",
        "--launch-salt",
        &"11".repeat(32),
        "--collateral",
        "25",
        "--min-tokens",
        "100",
        "--collateral-definition",
        "11111111111111111111111111111111",
        "--participant",
        "Public/11111111111111111111111111111111",
        "--factory-program-path",
        "factory.bin",
        "--factory-program-account",
        "11111111111111111111111111111111",
        "--curve-program-path",
        "curve.bin",
        "--curve-program-account",
        "11111111111111111111111111111111",
    ])
    .expect("collateral buy command should parse");
    assert!(matches!(cli.command, Command::BuyWithCollateral(_)));
}

#[test]
fn graduation_uses_a_typed_callback_without_arbitrary_adapter_accounts() {
    let salt = "11".repeat(32);
    let mut base = vec![
        "launchpad",
        "graduate",
        "--creator-hint",
        "11111111111111111111111111111111",
        "--launch-salt",
        &salt,
        "--collateral-definition",
        "11111111111111111111111111111111",
        "--factory-program-path",
        "factory.bin",
        "--factory-program-account",
        "11111111111111111111111111111111",
        "--curve-program-path",
        "curve.bin",
        "--curve-program-account",
        "11111111111111111111111111111111",
        "--fee-payer",
        "11111111111111111111111111111111",
    ];
    assert!(Cli::try_parse_from(&base).is_ok());
    base.extend(["--adapter-account", "11111111111111111111111111111111"]);
    assert!(Cli::try_parse_from(base).is_err());
}

#[test]
fn graduation_rejects_private_accounts_and_authority_routing() {
    let salt = "11".repeat(32);
    let base = vec![
        "launchpad",
        "graduate",
        "--creator-hint",
        "11111111111111111111111111111111",
        "--fee-payer",
        "11111111111111111111111111111111",
        "--launch-salt",
        &salt,
        "--collateral-definition",
        "Public/11111111111111111111111111111111",
        "--factory-program-path",
        "factory.bin",
        "--factory-program-account",
        "11111111111111111111111111111111",
        "--curve-program-path",
        "curve.bin",
        "--curve-program-account",
        "11111111111111111111111111111111",
    ];
    assert!(Cli::try_parse_from(&base).unwrap().validate_route().is_ok());
    let mut routed = base.clone();
    routed.extend(["--authority-router-path", "private_authority.bin"]);
    assert!(
        Cli::try_parse_from(routed)
            .unwrap()
            .validate_route()
            .is_err()
    );
    let mut private_namespace = base.clone();
    private_namespace.extend(["--namespace", "Private/11111111111111111111111111111111"]);
    assert!(
        Cli::try_parse_from(private_namespace)
            .unwrap()
            .validate_route()
            .is_err()
    );
    let mut private_adapter = base.clone();
    private_adapter.extend([
        "--adapter-account",
        "Private/11111111111111111111111111111111",
    ]);
    assert!(Cli::try_parse_from(private_adapter).is_err());
    let mut private_collateral = base;
    private_collateral[7] = "Private/11111111111111111111111111111111";
    assert!(Cli::try_parse_from(private_collateral).is_err());
}

#[test]
fn withdraw_accepts_the_factory_lifecycle_context() {
    let cli = Cli::try_parse_from([
        "launchpad",
        "withdraw",
        "--launch-salt",
        &"11".repeat(32),
        "--collateral-definition",
        "11111111111111111111111111111111",
        "--creator",
        "Public/11111111111111111111111111111111",
        "--factory-program-path",
        "factory.bin",
        "--factory-program-account",
        "11111111111111111111111111111111",
        "--curve-program-path",
        "curve.bin",
        "--curve-program-account",
        "11111111111111111111111111111111",
    ])
    .expect("withdraw command should parse");
    assert!(matches!(cli.command, Command::Withdraw(_)));
}

#[test]
fn create_sale_requires_trade_mode_with_only_the_two_named_values() {
    let command = Cli::command();
    let create = command
        .get_subcommands()
        .find(|command| command.get_name() == "create-sale")
        .unwrap();
    let mode = create
        .get_arguments()
        .find(|arg| arg.get_long() == Some("trade-mode"))
        .expect("trade mode argument");
    assert!(mode.is_required_set());
    let values: Vec<_> = mode
        .get_value_parser()
        .possible_values()
        .unwrap()
        .map(|value| value.get_name().to_owned())
        .collect();
    assert_eq!(values, ["buy-only", "buy-and-sell"]);
}

#[test]
fn create_sale_accepts_the_program_binaries_and_creator_account() {
    let cli = Cli::try_parse_from([
        "launchpad",
        "create-sale",
        "--engine",
        "bonding-curve",
        "--trade-mode",
        "buy-and-sell",
        "--launch-salt",
        &"11".repeat(32),
        "--name",
        "E2E token",
        "--uri",
        "https://example.invalid/e2e-token.json",
        "--sale-reserve",
        "800",
        "--creator-allocation",
        "50",
        "--virtual-token-reserve",
        "2000",
        "--virtual-collateral-reserve",
        "100",
        "--collateral-definition",
        "11111111111111111111111111111111",
        "--creator",
        "Public/11111111111111111111111111111111",
        "--factory-program-path",
        "factory.bin",
        "--factory-program-account",
        "11111111111111111111111111111111",
        "--curve-program-path",
        "curve.bin",
        "--engine-program-account",
        "11111111111111111111111111111111",
    ])
    .expect("create sale command should parse");
    assert!(matches!(cli.command, Command::CreateSale(_)));
}

#[test]
fn create_sale_saves_optional_hook_program_and_hex_configuration() {
    let salt = "11".repeat(32);
    let program = lee::AccountId::new([1; 32]).to_string();
    let mut arguments = vec![
        "launchpad",
        "create-sale",
        "--engine",
        "bonding-curve",
        "--trade-mode",
        "buy-only",
        "--launch-salt",
        &salt,
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
        "factory.bin",
        "--factory-program-account",
        "11111111111111111111111111111111",
        "--curve-program-path",
        "curve.bin",
        "--engine-program-account",
        "11111111111111111111111111111111",
    ];
    let without_hook = Cli::try_parse_from(&arguments).unwrap();
    let Command::CreateSale(args) = without_hook.command else {
        panic!("create sale")
    };
    assert!(args.sale_request().graduation_hook.is_none());
    assert_eq!(args.sale_request().sell_fee_bps, 0);
    for rate in ["0", "125", "9999"] {
        let mut selected = arguments.clone();
        selected.extend(["--sell-fee-bps", rate]);
        let Command::CreateSale(args) = Cli::try_parse_from(selected).unwrap().command else {
            panic!("create sale")
        };
        assert_eq!(
            args.sale_request().sell_fee_bps,
            rate.parse::<u16>().unwrap()
        );
    }
    for rate in ["10000", "65535", "65536", "-1"] {
        let mut selected = arguments.clone();
        selected.extend(["--sell-fee-bps", rate]);
        assert!(Cli::try_parse_from(selected).is_err());
    }

    assert_eq!(
        args.sale_request().ending_policy,
        launchpad_client::EndingPolicy::Depletion
    );
    assert_eq!(
        args.sale_request().pause_policy,
        launchpad_client::PausePolicy::Disabled
    );
    for (options, expected) in [
        (vec![], launchpad_client::EndingPolicy::Depletion),
        (
            vec!["--end-timestamp", "100"],
            launchpad_client::EndingPolicy::Either,
        ),
        (
            vec!["--ending-policy", "depletion"],
            launchpad_client::EndingPolicy::Depletion,
        ),
        (
            vec!["--ending-policy", "scheduled", "--end-timestamp", "100"],
            launchpad_client::EndingPolicy::Scheduled,
        ),
        (
            vec!["--ending-policy", "either", "--end-timestamp", "100"],
            launchpad_client::EndingPolicy::Either,
        ),
    ] {
        let mut selected = arguments.clone();
        selected.extend(options);
        let cli = Cli::try_parse_from(selected).unwrap();
        cli.validate_route().unwrap();
        let Command::CreateSale(args) = cli.command else {
            panic!("create sale")
        };
        assert_eq!(args.sale_request().ending_policy, expected);
    }
    for ending in ["scheduled", "either", "manual", "unknown"] {
        let mut missing_end = arguments.clone();
        missing_end.extend(["--ending-policy", ending]);
        assert!(Cli::try_parse_from(missing_end).is_err());
    }
    let mut depletion_with_end = arguments.clone();
    depletion_with_end.extend(["--ending-policy", "depletion", "--end-timestamp", "100"]);
    assert!(
        Cli::try_parse_from(depletion_with_end)
            .unwrap()
            .validate_route()
            .is_err()
    );
    let mut pausable = arguments.clone();
    pausable.push("--pausable");
    let Command::CreateSale(args) = Cli::try_parse_from(pausable).unwrap().command else {
        panic!("create sale")
    };
    assert_eq!(
        args.sale_request().pause_policy,
        launchpad_client::PausePolicy::AuthorityControlled
    );
    for flag in ["--graduation-launch-bps", "--graduation-collateral-bps"] {
        for value in ["0", "10000"] {
            let mut selected = arguments.clone();
            selected.extend([flag, value]);
            let cli = Cli::try_parse_from(selected).unwrap();
            assert_eq!(cli.validate_route().is_ok(), value == "0");
        }
        for value in ["10001", "65536", "-1"] {
            let mut selected = arguments.clone();
            selected.extend([flag, value]);
            assert!(Cli::try_parse_from(selected).is_err());
        }
    }
    let mut missing_split = arguments.clone();
    missing_split.extend(["--graduation-hook-program-id", &program]);
    assert!(Cli::try_parse_from(&missing_split).is_err());
    for flag in ["--graduation-launch-bps", "--graduation-collateral-bps"] {
        let mut selected = missing_split.clone();
        selected.extend([flag, "0"]);
        assert!(Cli::try_parse_from(selected).is_err());
    }
    let mut configuration_without_program = arguments.clone();
    configuration_without_program.extend(["--graduation-hook-configuration", "070b"]);
    assert!(Cli::try_parse_from(configuration_without_program).is_err());
    for invalid_program in ["00", "zz", &"00".repeat(33)] {
        let mut invalid = arguments.clone();
        invalid.extend(["--graduation-hook-program-id", invalid_program]);
        assert!(Cli::try_parse_from(invalid).is_err());
    }
    let mut invalid_configuration = arguments.clone();
    invalid_configuration.extend([
        "--graduation-hook-program-id",
        &program,
        "--graduation-hook-configuration",
        "07z",
    ]);
    assert!(Cli::try_parse_from(invalid_configuration).is_err());
    let mut empty_configuration = arguments.clone();
    empty_configuration.extend([
        "--graduation-hook-program-id",
        &program,
        "--graduation-launch-bps",
        "0",
        "--graduation-collateral-bps",
        "10000",
    ]);
    let Command::CreateSale(args) = Cli::try_parse_from(empty_configuration).unwrap().command
    else {
        panic!("create sale")
    };
    assert_eq!(
        args.sale_request().graduation_hook,
        Some(launchpad_client::GraduationHook {
            program_id: lee::AccountId::new([1; 32]),
            configuration: vec![],
        })
    );
    arguments.extend([
        "--graduation-hook-program-id",
        &program,
        "--graduation-hook-configuration",
        "070b",
        "--graduation-launch-bps",
        "2500",
        "--graduation-collateral-bps",
        "7500",
    ]);
    let cli = Cli::try_parse_from(arguments).unwrap();
    let Command::CreateSale(args) = cli.command else {
        panic!("create sale")
    };
    assert_eq!(
        args.sale_request().graduation_hook,
        Some(launchpad_client::GraduationHook {
            program_id: lee::AccountId::new([1; 32]),
            configuration: vec![7, 11],
        })
    );
}

#[test]
fn close_command_is_not_available() {
    assert!(Cli::try_parse_from(["launchpad", "close"]).is_err());
}

#[test]
fn price_accepts_an_exact_output_buy_quote_with_live_program_context() {
    let cli = Cli::try_parse_from([
        "launchpad",
        "price",
        "--launch-salt",
        &"11".repeat(32),
        "--tokens",
        "25",
        "--collateral-definition",
        "11111111111111111111111111111111",
        "--factory-program-path",
        "factory.bin",
        "--factory-program-account",
        "11111111111111111111111111111111",
        "--curve-program-path",
        "curve.bin",
        "--curve-program-account",
        "11111111111111111111111111111111",
    ])
    .expect("price command should parse");
    assert!(matches!(cli.command, Command::Price(_)));
}

#[test]
fn configure_accepts_the_curve_admin_and_treasury_context() {
    let cli = Cli::try_parse_from([
        "launchpad",
        "--json",
        "configure",
        "--admin",
        "Public/11111111111111111111111111111111",
        "--treasury",
        "Public/11111111111111111111111111111111",
        "--curve-program-path",
        "curve.bin",
        "--curve-program-account",
        "11111111111111111111111111111111",
    ])
    .expect("configure command should parse");
    assert!(matches!(cli.command, Command::Configure(_)));
}

#[test]
fn launch_salt_must_be_exactly_32_bytes() {
    let account = "11111111111111111111111111111111";
    let error = Cli::try_parse_from([
        "launchpad",
        "sale-info",
        "--engine",
        "bonding-curve",
        "--launch-salt",
        "00",
        "--collateral-definition",
        account,
        "--factory-program-account",
        account,
        "--engine-program-account",
        account,
    ])
    .expect_err("short launch salts must be rejected");
    assert!(error.to_string().contains("exactly 32 bytes"));
}
