use std::{
    path::{Path, PathBuf},
    process::Command,
};

use curve_core::Instruction as CurveInstruction;
use factory_core::Instruction as FactoryInstruction;
use launchpad_client::{CreateSaleRequest, FactorySale, build_create_sale_invocation};
use lee::{AccountId, program::Program};
use private_flow_core::PrivateBuyInstruction;
use serde::Serialize;
use serde_json::Value;

const CLI_PROGRAM_ID: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Clone, Copy)]
struct AccountInput {
    name: &'static str,
    id: Option<AccountId>,
}

fn account(id: u8) -> AccountId {
    AccountId::new([id; 32])
}

fn account_hex(id: u8) -> String {
    hex::encode([id; 32])
}

fn account_id_hex(account: AccountId) -> String {
    hex::encode(account.value())
}

fn program_id(first_word: u32) -> [u32; 8] {
    std::array::from_fn(|offset| first_word + u32::try_from(offset).unwrap())
}

fn program_arg(program_id: [u32; 8]) -> String {
    program_id
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn words_hex(words: Vec<u32>) -> String {
    hex::encode(
        words
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_owned()
}

fn account_inputs(names: &[&'static str], first_id: u8) -> Vec<AccountInput> {
    names
        .iter()
        .enumerate()
        .map(|(offset, name)| AccountInput {
            name,
            id: Some(account(first_id + u8::try_from(offset).unwrap())),
        })
        .collect()
}

fn named_account_inputs(names: &[&'static str], ids: Vec<AccountId>) -> Vec<AccountInput> {
    assert_eq!(names.len(), ids.len());
    names
        .iter()
        .zip(ids)
        .map(|(&name, id)| AccountInput { name, id: Some(id) })
        .collect()
}

fn create_pool_arguments(
    curve_program_id: [u32; 8],
    close_timestamp: &str,
    close_on_depletion: &str,
    owner_program_id: &str,
    owner_program_seed: &str,
) -> Vec<(&'static str, String)> {
    vec![
        ("--allowed-input-token", "none".into()),
        ("--defer-funding", "false".into()),
        ("--namespace", account_hex(44)),
        ("--token0-amount", "1000".into()),
        ("--token1-amount", "2000".into()),
        ("--virtual-reserve0", "3000".into()),
        ("--virtual-reserve1", "4000".into()),
        ("--close-timestamp", close_timestamp.into()),
        ("--close-on-depletion", close_on_depletion.into()),
        ("--owner", account_hex(45)),
        ("--owner-program-id", owner_program_id.into()),
        ("--owner-program-seed", owner_program_seed.into()),
        ("--curve-program-id", program_arg(curve_program_id)),
    ]
}

fn create_factory_pool_arguments(
    curve_program_id: [u32; 8],
    end_timestamp: &str,
) -> Vec<(&'static str, String)> {
    vec![
        ("--trade-mode", "1".into()),
        ("--graduation-hook-program-id", program_arg(program_id(60))),
        ("--graduation-hook-configuration", "7,11".into()),
        ("--namespace", account_hex(75)),
        ("--launch-salt", hex::encode([76; 32])),
        ("--name", "Launch".into()),
        ("--uri", "https://example.invalid/launch".into()),
        ("--sale-reserve", "1000".into()),
        ("--dex-seed-reserve", "200".into()),
        ("--creator-allocation", "300".into()),
        ("--virtual-token-reserve", "4000".into()),
        ("--virtual-collateral-reserve", "5000".into()),
        ("--end-timestamp", end_timestamp.into()),
        ("--curve-program-id", program_arg(curve_program_id)),
    ]
}

fn create_sale_request(end_timestamp: Option<u64>) -> CreateSaleRequest {
    CreateSaleRequest {
        trade_mode: launchpad_client::TradeMode::BuyAndSell,
        graduation_hook: Some(launchpad_client::GraduationHook {
            program_id: program_id(60),
            configuration: vec![7, 11],
        }),
        name: "Launch".into(),
        uri: "https://example.invalid/launch".into(),
        sale_reserve: 1000,
        dex_seed_reserve: 200,
        creator_allocation: 300,
        virtual_token_reserve: 4000,
        virtual_collateral_reserve: 5000,
        end_timestamp,
    }
}

fn dry_run_command(
    program: &str,
    instruction: &str,
    arguments: Vec<(&str, String)>,
    accounts: &[AccountInput],
) -> Command {
    let root = root();
    let idl_path = root.join("idl").join(format!("{program}.json"));
    let mut command = Command::new("lgs");
    command.current_dir(&root).args([
        "spel",
        "--",
        "--idl",
        idl_path.to_str().unwrap(),
        "--program",
        CLI_PROGRAM_ID,
        "--dry-run=json",
        "--",
        instruction,
    ]);
    for (name, value) in arguments {
        command.arg(name).arg(value);
    }
    for account in accounts {
        if let Some(id) = account.id {
            command
                .arg(format!("--{}", account.name.replace('_', "-")))
                .arg(account_id_hex(id));
        }
    }

    command
}

fn run_case<I: Serialize>(
    program: &str,
    instruction: &str,
    arguments: Vec<(&str, String)>,
    expected_instruction: I,
    accounts: Vec<AccountInput>,
    expected_argument_order: &[&str],
) {
    let idl_path = root().join("idl").join(format!("{program}.json"));
    let output = dry_run_command(program, instruction, arguments, &accounts)
        .output()
        .expect("run pinned lgs SPEL CLI");
    assert!(
        output.status.success(),
        "{} {instruction} failed: {}",
        program,
        String::from_utf8_lossy(&output.stderr)
    );
    let dry_run: Value = serde_json::from_slice(&output.stdout).expect("parse lgs dry-run JSON");
    assert_eq!(dry_run["instruction"], instruction.replace('-', "_"));
    assert_eq!(
        dry_run["instruction_data"],
        words_hex(Program::serialize_instruction(expected_instruction).unwrap())
    );

    let actual_accounts = dry_run["accounts"].as_array().unwrap();
    assert_eq!(actual_accounts.len(), accounts.len());
    for (actual, expected) in actual_accounts.iter().zip(&accounts) {
        assert_eq!(actual["name"], expected.name);
        match expected.id {
            Some(id) => assert_eq!(actual["id"], format!("0x{}", account_id_hex(id))),
            None => {
                assert!(actual["id"].is_null());
                assert!(
                    actual["flags"]
                        .as_array()
                        .unwrap()
                        .contains(&Value::String("rest".into()))
                );
            }
        }
    }

    let idl: Value = serde_json::from_slice(&std::fs::read(idl_path).unwrap()).unwrap();
    let idl_instruction = idl["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["name"] == instruction.replace('-', "_"))
        .unwrap();
    assert_eq!(
        idl_instruction["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|argument| argument["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected_argument_order
    );
    assert_eq!(
        idl_instruction["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|account| account["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        accounts
            .iter()
            .map(|account| account.name)
            .collect::<Vec<_>>()
    );
    if program != "private_buy" {
        assert!(
            idl_instruction["accounts"]
                .as_array()
                .unwrap()
                .iter()
                .all(|account| account.get("pda").is_none())
        );
    }
}

#[test]
fn curve_public_instruction_wire_matches_the_pinned_cli() {
    let curve_program_id = program_id(10);
    run_case(
        "curve",
        "update-config",
        vec![
            ("--namespace", account_hex(40)),
            ("--admin", account_hex(41)),
            ("--protocol-fee-bps", "100".into()),
            ("--treasury", account_hex(42)),
        ],
        CurveInstruction::UpdateConfig {
            namespace: account(40),
            admin: account(41),
            protocol_fee_bps: 100,
            treasury: account(42),
        },
        vec![
            AccountInput {
                name: "config",
                id: Some(curve_core::compute_config_pda(
                    account(40),
                    curve_program_id,
                )),
            },
            AccountInput {
                name: "authority",
                id: Some(account(1)),
            },
        ],
        &["namespace", "admin", "protocol_fee_bps", "treasury"],
    );
    run_case(
        "curve",
        "renounce-admin",
        vec![("--namespace", account_hex(43))],
        CurveInstruction::RenounceAdmin {
            namespace: account(43),
        },
        account_inputs(&["config", "authority"], 3),
        &["namespace"],
    );
    let create_pool_accounts = named_account_inputs(
        &[
            "pool",
            "owner_authority",
            "token0_definition",
            "token1_definition",
            "owner_token0_ata",
            "owner_token1_ata",
            "pool_token0_ata",
            "pool_token1_ata",
            "clock",
            "config",
        ],
        vec![
            curve_core::compute_pool_pda(
                account(44),
                curve_program_id,
                account(47),
                account(48),
                account(45),
            ),
            account(45),
            account(47),
            account(48),
            account(49),
            account(50),
            account(51),
            account(52),
            clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
            curve_core::compute_config_pda(account(44), curve_program_id),
        ],
    );
    let create_pool_argument_order = [
        "allowed_input_token",
        "defer_funding",
        "namespace",
        "token0_amount",
        "token1_amount",
        "virtual_reserve0",
        "virtual_reserve1",
        "close_timestamp",
        "close_on_depletion",
        "owner",
        "owner_program_id",
        "owner_program_seed",
        "curve_program_id",
    ];
    run_case(
        "curve",
        "create-pool",
        create_pool_arguments(
            curve_program_id,
            "5000",
            "1",
            &program_arg(program_id(20)),
            &hex::encode([46; 32]),
        ),
        CurveInstruction::CreatePool {
            allowed_input_token: None,
            defer_funding: false,
            namespace: account(44),
            token0_amount: 1000,
            token1_amount: 2000,
            virtual_reserve0: 3000,
            virtual_reserve1: 4000,
            close_timestamp: Some(5000),
            close_on_depletion: Some(1),
            owner: account(45),
            owner_program_id: Some(program_id(20)),
            owner_program_seed: Some([46; 32]),
            curve_program_id,
        },
        create_pool_accounts.clone(),
        &create_pool_argument_order,
    );
    run_case(
        "curve",
        "create-pool",
        create_pool_arguments(curve_program_id, "none", "none", "none", "none"),
        CurveInstruction::CreatePool {
            allowed_input_token: None,
            defer_funding: false,
            namespace: account(44),
            token0_amount: 1000,
            token1_amount: 2000,
            virtual_reserve0: 3000,
            virtual_reserve1: 4000,
            close_timestamp: None,
            close_on_depletion: None,
            owner: account(45),
            owner_program_id: None,
            owner_program_seed: None,
            curve_program_id,
        },
        create_pool_accounts,
        &create_pool_argument_order,
    );
    run_case(
        "curve",
        "swap-exact-input",
        vec![
            ("--amount-in", "100".into()),
            ("--min-amount-out", "90".into()),
            ("--token-in", account_hex(47)),
        ],
        CurveInstruction::SwapExactInput {
            amount_in: 100,
            min_amount_out: 90,
            token_in: account(47),
        },
        account_inputs(
            &[
                "pool",
                "config",
                "participant",
                "participant_token_in_ata",
                "pool_token_in_ata",
                "pool_token_out_ata",
                "participant_token_out_ata",
                "clock",
            ],
            15,
        ),
        &["amount_in", "min_amount_out", "token_in"],
    );
    run_case(
        "curve",
        "swap-exact-output",
        vec![
            ("--amount-out", "100".into()),
            ("--max-amount-in", "110".into()),
            ("--token-in", account_hex(48)),
        ],
        CurveInstruction::SwapExactOutput {
            amount_out: 100,
            max_amount_in: 110,
            token_in: account(48),
        },
        account_inputs(
            &[
                "pool",
                "config",
                "participant",
                "participant_token_in_ata",
                "pool_token_in_ata",
                "pool_token_out_ata",
                "participant_token_out_ata",
                "clock",
            ],
            23,
        ),
        &["amount_out", "max_amount_in", "token_in"],
    );
    run_case(
        "curve",
        "close-pool",
        vec![],
        CurveInstruction::ClosePool,
        account_inputs(&["pool", "owner", "clock"], 31),
        &[],
    );
    run_case(
        "curve",
        "activate-pool",
        vec![],
        CurveInstruction::ActivatePool,
        account_inputs(
            &[
                "pool",
                "owner_authority",
                "token0_definition",
                "token1_definition",
                "owner_token0_ata",
                "owner_token1_ata",
                "pool_token0_ata",
                "pool_token1_ata",
                "clock",
                "config",
            ],
            34,
        ),
        &[],
    );
    let withdraw_accounts = account_inputs(
        &[
            "pool",
            "owner",
            "owner_token0_ata",
            "owner_token1_ata",
            "pool_token0_ata",
            "pool_token1_ata",
            "clock",
            "config",
        ],
        44,
    );
    let mut without_treasury = withdraw_accounts.clone();
    without_treasury.push(AccountInput {
        name: "treasury_collateral_atas",
        id: None,
    });
    run_case(
        "curve",
        "withdraw-reserves",
        vec![],
        CurveInstruction::WithdrawReserves,
        without_treasury,
        &[],
    );
    let mut with_treasury = withdraw_accounts;
    with_treasury.push(AccountInput {
        name: "treasury_collateral_atas",
        id: Some(account(52)),
    });
    run_case(
        "curve",
        "withdraw-reserves",
        vec![],
        CurveInstruction::WithdrawReserves,
        with_treasury,
        &[],
    );
    let collection_accounts = account_inputs(&["pool", "config", "pool_collateral_ata"], 53);
    let mut without_treasury = collection_accounts.clone();
    without_treasury.push(AccountInput {
        name: "treasury_collateral_atas",
        id: None,
    });
    run_case(
        "curve",
        "collect-fees",
        vec![],
        CurveInstruction::CollectFees,
        without_treasury,
        &[],
    );
    let mut with_treasury = collection_accounts;
    with_treasury.push(AccountInput {
        name: "treasury_collateral_atas",
        id: Some(account(56)),
    });
    run_case(
        "curve",
        "collect-fees",
        vec![],
        CurveInstruction::CollectFees,
        with_treasury,
        &[],
    );
}

#[test]
fn curve_freeze_instruction_wire_matches_the_pinned_cli() {
    for frozen in [false, true] {
        run_case(
            "curve",
            "set-frozen",
            vec![
                ("--namespace", account_hex(57)),
                ("--frozen", frozen.to_string()),
            ],
            CurveInstruction::SetFrozen {
                namespace: account(57),
                frozen,
            },
            account_inputs(&["config", "authority"], 58),
            &["namespace", "frozen"],
        );
    }
    let idl: Value =
        serde_json::from_slice(&std::fs::read(root().join("idl/curve.json")).unwrap()).unwrap();
    let accounts = idl["instructions"][9]["accounts"].as_array().unwrap();
    assert_eq!(accounts[0]["signer"], false);
    assert_eq!(accounts[1]["signer"], true);
}

#[test]
fn curve_emergency_transfer_wire_matches_the_pinned_cli() {
    run_case(
        "curve",
        "transfer-emergency-authority",
        vec![
            ("--namespace", account_hex(57)),
            ("--new-authority", account_hex(60)),
        ],
        CurveInstruction::TransferEmergencyAuthority {
            namespace: account(57),
            new_authority: account(60),
        },
        account_inputs(&["config", "authority"], 58),
        &["namespace", "new_authority"],
    );
    let idl: Value =
        serde_json::from_slice(&std::fs::read(root().join("idl/curve.json")).unwrap()).unwrap();
    let accounts = idl["instructions"][10]["accounts"].as_array().unwrap();
    assert_eq!(accounts[0]["signer"], false);
    assert_eq!(accounts[1]["signer"], true);
}

#[test]
fn factory_public_instruction_wire_matches_the_pinned_cli() {
    let curve_program_id = program_id(30);
    let sale = FactorySale {
        namespace: account(75),
        factory_program_id: program_id(29),
        curve_program_id,
        launch_salt: [76; 32],
        collateral_definition: account(77),
    };
    let creator = account(78);
    let create_account_names = [
        "factory",
        "token_definition",
        "mint",
        "metadata",
        "creator_escrow",
        "creator",
        "creator_token_ata",
        "collateral_definition",
        "factory_token_ata",
        "factory_collateral_ata",
        "pool",
        "pool_token_ata",
        "pool_collateral_ata",
        "clock",
        "config",
    ];
    let create_argument_order = [
        "trade_mode",
        "graduation_hook_program_id",
        "graduation_hook_configuration",
        "namespace",
        "launch_salt",
        "name",
        "uri",
        "sale_reserve",
        "dex_seed_reserve",
        "creator_allocation",
        "virtual_token_reserve",
        "virtual_collateral_reserve",
        "end_timestamp",
        "curve_program_id",
    ];
    let present_create =
        build_create_sale_invocation(sale, creator, create_sale_request(Some(6000)))
            .expect("build factory create invocation");
    run_case(
        "factory",
        "create-factory-pool",
        create_factory_pool_arguments(curve_program_id, "6000"),
        present_create.instruction,
        named_account_inputs(&create_account_names, present_create.account_ids),
        &create_argument_order,
    );
    let absent_create = build_create_sale_invocation(sale, creator, create_sale_request(None))
        .expect("build factory create invocation");
    run_case(
        "factory",
        "create-factory-pool",
        create_factory_pool_arguments(curve_program_id, "none"),
        absent_create.instruction,
        named_account_inputs(&create_account_names, absent_create.account_ids),
        &create_argument_order,
    );
    let create_accounts = named_account_inputs(
        &create_account_names,
        build_create_sale_invocation(sale, creator, create_sale_request(Some(6000)))
            .expect("build factory create invocation")
            .account_ids,
    );
    run_case(
        "factory",
        "continue-creation",
        vec![],
        FactoryInstruction::ContinueCreation,
        create_accounts,
        &[],
    );
    run_case(
        "factory",
        "claim-creator-allocation",
        vec![],
        FactoryInstruction::ClaimCreatorAllocation,
        account_inputs(
            &[
                "factory",
                "pool",
                "creator_escrow",
                "creator",
                "token_definition",
                "creator_token_ata",
                "clock",
            ],
            77,
        ),
        &[],
    );
    let withdraw_accounts = account_inputs(
        &[
            "factory",
            "pool",
            "creator",
            "token_definition",
            "collateral_definition",
            "factory_token_ata",
            "factory_collateral_ata",
            "pool_token_ata",
            "pool_collateral_ata",
            "creator_token_ata",
            "creator_collateral_ata",
            "clock",
            "config",
        ],
        84,
    );
    let mut without_treasury = withdraw_accounts.clone();
    without_treasury.push(AccountInput {
        name: "treasury_collateral_atas",
        id: None,
    });
    run_case(
        "factory",
        "withdraw-factory-proceeds",
        vec![],
        FactoryInstruction::WithdrawFactoryProceeds,
        without_treasury,
        &[],
    );
    let mut with_treasury = withdraw_accounts;
    with_treasury.push(AccountInput {
        name: "treasury_collateral_atas",
        id: Some(account(97)),
    });
    run_case(
        "factory",
        "withdraw-factory-proceeds",
        vec![],
        FactoryInstruction::WithdrawFactoryProceeds,
        with_treasury,
        &[],
    );
}

#[test]
fn pinned_cli_reports_its_empty_hook_configuration_limit() {
    let accounts = account_inputs(
        &[
            "factory",
            "token_definition",
            "mint",
            "metadata",
            "creator_escrow",
            "creator",
            "creator_token_ata",
            "collateral_definition",
            "factory_token_ata",
            "factory_collateral_ata",
            "pool",
            "pool_token_ata",
            "pool_collateral_ata",
            "clock",
            "config",
        ],
        1,
    );
    for configuration in ["", "[]"] {
        let mut arguments = create_factory_pool_arguments(program_id(30), "none");
        for (name, value) in &mut arguments {
            match *name {
                "--graduation-hook-program-id" => *value = "none".into(),
                "--graduation-hook-configuration" => *value = configuration.into(),
                _ => {}
            }
        }
        let output = dry_run_command("factory", "create-factory-pool", arguments, &accounts)
            .output()
            .expect("run pinned lgs SPEL CLI");
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("type mismatch: expected Vec"), "{error}");
        assert!(error.contains("got Raw"), "{error}");
    }
}

#[test]
fn graduation_instruction_wire_matches_the_pinned_cli() {
    let mut accounts = account_inputs(
        &[
            "factory",
            "pool",
            "adapter_custody",
            "token_definition",
            "collateral_definition",
            "factory_token_ata",
            "factory_collateral_ata",
            "pool_token_ata",
            "pool_collateral_ata",
            "adapter_token_ata",
            "adapter_collateral_ata",
            "clock",
            "config",
        ],
        100,
    );
    accounts.push(AccountInput {
        name: "treasury_and_adapter_accounts",
        id: Some(account(113)),
    });
    run_case(
        "factory",
        "execute-graduation-hook",
        vec![("--remaining-accounts", "0".into())],
        FactoryInstruction::ExecuteGraduationHook {
            remaining_accounts: 0,
        },
        accounts,
        &["remaining_accounts"],
    );
    let idl: Value =
        serde_json::from_slice(&std::fs::read(root().join("idl/factory.json")).unwrap()).unwrap();
    assert!(
        idl["instructions"][4]["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|account| account["signer"] == false)
    );
}

#[test]
fn private_buy_execute_wire_matches_the_pinned_cli() {
    let curve_program_id = program_id(40);
    let token_program_id = program_id(50);
    let ata_program_id = program_id(60);
    let native_transfer_program_id = program_id(70);
    run_case(
        "private_buy",
        "execute",
        vec![
            ("--curve-program-id", program_arg(curve_program_id)),
            ("--token-program-id", program_arg(token_program_id)),
            ("--ata-program-id", program_arg(ata_program_id)),
            (
                "--native-transfer-program-id",
                program_arg(native_transfer_program_id),
            ),
            ("--amount-out", "100".into()),
            ("--max-collateral-in", "125".into()),
            ("--gas-reserve", "5".into()),
            ("--collateral-definition-id", account_hex(110)),
        ],
        PrivateBuyInstruction::Execute {
            curve_program_id,
            token_program_id,
            ata_program_id,
            native_transfer_program_id,
            amount_out: 100,
            max_collateral_in: 125,
            gas_reserve: 5,
            collateral_definition_id: account(110),
        },
        account_inputs(
            &[
                "private_source",
                "transient",
                "collateral_definition",
                "launch_definition",
                "transient_collateral_ata",
                "transient_token_ata",
                "pool",
                "config",
                "pool_collateral_ata",
                "pool_token_ata",
                "clock",
                "private_destination",
            ],
            111,
        ),
        &[
            "curve_program_id",
            "token_program_id",
            "ata_program_id",
            "native_transfer_program_id",
            "amount_out",
            "max_collateral_in",
            "gas_reserve",
            "collateral_definition_id",
        ],
    );
}
