use lee_core::account::AccountId;
use serde_json::{Value, json};

fn schema() -> Value {
    serde_json::from_str(include_str!("../../../idl/launchpad.json")).unwrap()
}

fn variant(module: &str, message: &str, name: &str) -> Value {
    schema()["modules"][module][message]["variants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["name"] == name)
        .unwrap()
        .clone()
}

#[test]
fn namespace_treasury_update_keeps_the_destination_on_wire() {
    assert_wire(
        curve_core::Message::UpdateTreasury {
            namespace: AccountId::new([1; 32]),
            treasury: AccountId::new([2; 32]),
        },
        concat!(
            "0e",
            "0101010101010101010101010101010101010101010101010101010101010101",
            "0202020202020202020202020202020202020202020202020202020202020202"
        ),
    );
    assert_eq!(
        variant("curve", "Message", "UpdateTreasury"),
        json!({
            "tag":14,"name":"UpdateTreasury","fields":[
                {"name":"namespace","type":"AccountId"},{"name":"treasury","type":"AccountId"}
            ]
        })
    );
}

#[test]
fn namespace_admin_transfer_distinguishes_signer_and_pda_candidates_on_wire() {
    for (candidate, encoded) in [
        (
            curve_core::AdminCandidate::Signer,
            concat!(
                "0d",
                "0101010101010101010101010101010101010101010101010101010101010101",
                "0202020202020202020202020202020202020202020202020202020202020202",
                "00"
            ),
        ),
        (
            curve_core::AdminCandidate::Pda {
                program_id: AccountId::new([3; 32]),
                seed: [4; 32],
            },
            concat!(
                "0d",
                "0101010101010101010101010101010101010101010101010101010101010101",
                "0202020202020202020202020202020202020202020202020202020202020202",
                "01",
                "0303030303030303030303030303030303030303030303030303030303030303",
                "0404040404040404040404040404040404040404040404040404040404040404"
            ),
        ),
    ] {
        assert_wire(
            curve_core::Message::TransferAdmin {
                namespace: AccountId::new([1; 32]),
                new_admin: AccountId::new([2; 32]),
                candidate,
            },
            encoded,
        );
    }
    assert_eq!(
        variant("curve", "Message", "TransferAdmin"),
        json!({
            "tag":13,"name":"TransferAdmin","fields":[
                {"name":"namespace","type":"AccountId"},{"name":"new_admin","type":"AccountId"},
                {"name":"candidate","type":"AdminCandidate"}
            ]
        })
    );
    assert_eq!(
        variant("curve", "AdminCandidate", "Signer"),
        json!({"tag":0,"name":"Signer","fields":[]})
    );
    assert_eq!(
        variant("curve", "AdminCandidate", "Pda"),
        json!({
            "tag":1,"name":"Pda","fields":[
                {"name":"program_id","type":"AccountId"},{"name":"seed","type":"[u8;32]"}
            ]
        })
    );
}

#[test]
fn namespace_admin_renunciation_has_no_successor_on_wire() {
    assert_wire(
        curve_core::Message::RenounceAdmin {
            namespace: AccountId::new([1; 32]),
        },
        concat!(
            "0b",
            "0101010101010101010101010101010101010101010101010101010101010101"
        ),
    );
    assert_eq!(
        variant("curve", "Message", "RenounceAdmin"),
        json!({
            "tag":11,"name":"RenounceAdmin","fields":[{"name":"namespace","type":"AccountId"}]
        })
    );
}

#[test]
fn namespace_read_selects_its_namespace_on_wire() {
    assert_wire(
        curve_core::Message::ReadNamespace {
            namespace: AccountId::new([1; 32]),
        },
        concat!(
            "0c",
            "0101010101010101010101010101010101010101010101010101010101010101"
        ),
    );
    assert_eq!(
        variant("curve", "Message", "ReadNamespace"),
        json!({
            "tag":12,"name":"ReadNamespace","fields":[{"name":"namespace","type":"AccountId"}]
        })
    );
}

#[test]
fn emergency_freeze_and_unfreeze_keep_literal_boolean_bytes() {
    for (frozen, encoded) in [
        (
            true,
            concat!(
                "0a",
                "0101010101010101010101010101010101010101010101010101010101010101",
                "01"
            ),
        ),
        (
            false,
            concat!(
                "0a",
                "0101010101010101010101010101010101010101010101010101010101010101",
                "00"
            ),
        ),
    ] {
        assert_wire(
            curve_core::Message::SetFrozen {
                namespace: AccountId::new([1; 32]),
                frozen,
            },
            encoded,
        );
    }
    assert_eq!(
        variant("curve", "Message", "SetFrozen"),
        json!({
            "tag":10,"name":"SetFrozen","fields":[
                {"name":"namespace","type":"AccountId"},{"name":"frozen","type":"bool"}
            ]
        })
    );
}

#[test]
fn emergency_transfer_has_a_separate_successor_on_wire() {
    assert_wire(
        curve_core::Message::TransferEmergencyAuthority {
            namespace: AccountId::new([1; 32]),
            new_authority: AccountId::new([2; 32]),
        },
        concat!(
            "09",
            "0101010101010101010101010101010101010101010101010101010101010101",
            "0202020202020202020202020202020202020202020202020202020202020202"
        ),
    );
    assert_eq!(
        variant("curve", "Message", "TransferEmergencyAuthority"),
        json!({
            "tag":9,"name":"TransferEmergencyAuthority","fields":[
                {"name":"namespace","type":"AccountId"},{"name":"new_authority","type":"AccountId"}
            ]
        })
    );
}

#[test]
fn role_transfer_to_a_pda_keeps_owner_program_and_seed_on_wire() {
    assert_wire(
        lez_compat_core::authority::Message::TransferPda {
            role: AccountId::new([1; 32]),
            program: AccountId::new([2; 32]),
            seed: lee_core::program::PdaSeed::new([3; 32]),
        },
        concat!(
            "09",
            "0101010101010101010101010101010101010101010101010101010101010101",
            "0202020202020202020202020202020202020202020202020202020202020202",
            "0303030303030303030303030303030303030303030303030303030303030303"
        ),
    );
    let schema: Value =
        serde_json::from_str(include_str!("../../../packages/lez-compat/idl/compat.json")).unwrap();
    let variants = schema["modules"]["authority"]["Message"]["variants"]
        .as_array()
        .unwrap();
    assert_eq!(
        variants
            .iter()
            .find(|entry| entry["name"] == "TransferPda")
            .unwrap(),
        &json!({
            "tag":9,"name":"TransferPda","fields":[
                {"name":"role","type":"AccountId"},{"name":"program","type":"AccountId"},
                {"name":"seed","type":"PdaSeed"}
            ]
        })
    );
}

#[test]
fn role_successor_consent_keeps_the_role_and_epoch_on_wire() {
    assert_wire(
        lez_compat_core::authority::Message::Consent {
            role: AccountId::new([1; 32]),
            epoch: 0x0102030405060708,
        },
        concat!(
            "01",
            "0101010101010101010101010101010101010101010101010101010101010101",
            "0807060504030201"
        ),
    );
    let schema: Value =
        serde_json::from_str(include_str!("../../../packages/lez-compat/idl/compat.json")).unwrap();
    let variants = schema["modules"]["authority"]["Message"]["variants"]
        .as_array()
        .unwrap();
    assert_eq!(
        variants
            .iter()
            .find(|entry| entry["name"] == "Consent")
            .unwrap(),
        &json!({
            "tag":1,"name":"Consent","fields":[
                {"name":"role","type":"AccountId"},{"name":"epoch","type":"u64"}
            ]
        })
    );
}

#[test]
fn curve_exact_input_keeps_token_direction_and_minimum_output_on_wire() {
    assert_wire(
        curve_core::Message::SwapExactInput {
            amount_in: 100,
            min_amount_out: 25,
            token_in: AccountId::new([4; 32]),
            now: 9,
        },
        concat!(
            "05",
            "64000000000000000000000000000000",
            "19000000000000000000000000000000",
            "0404040404040404040404040404040404040404040404040404040404040404",
            "0900000000000000"
        ),
    );
    assert_eq!(
        variant("curve", "Message", "SwapExactInput"),
        json!({
            "tag":5,"name":"SwapExactInput","fields":[
                {"name":"amount_in","type":"u128"},{"name":"min_amount_out","type":"u128"},
                {"name":"token_in","type":"AccountId"},{"name":"now","type":"u64"}
            ]
        })
    );
}

#[test]
fn curve_fee_collection_has_no_caller_selected_treasury_on_wire() {
    assert_wire(curve_core::Message::CollectFees, "03");
    assert_eq!(
        variant("curve", "Message", "CollectFees"),
        json!({
            "tag":3,"name":"CollectFees","fields":[]
        })
    );
}

#[test]
fn curve_lifecycle_calls_keep_separate_tags_and_clock_bounds_on_wire() {
    for (message, name, tag, encoded) in [
        (
            curve_core::Message::WithdrawReserves { now: 9 },
            "WithdrawReserves",
            1,
            "010900000000000000",
        ),
        (
            curve_core::Message::ClosePool { now: 9 },
            "ClosePool",
            2,
            "020900000000000000",
        ),
        (
            curve_core::Message::ActivatePool { now: 9 },
            "ActivatePool",
            6,
            "060900000000000000",
        ),
    ] {
        assert_wire(message, encoded);
        assert_eq!(
            variant("curve", "Message", name),
            json!({
                "tag":tag,"name":name,"fields":[{"name":"now","type":"u64"}]
            })
        );
    }
}

#[test]
fn factory_creator_calls_keep_distinct_tags_creator_and_clock_on_wire() {
    for (message, name, tag, encoded) in [
        (
            factory_core::Message::ContinueCreation {
                creator: AccountId::new([5; 32]),
                now: 9,
            },
            "ContinueCreation",
            1,
            concat!(
                "01",
                "0505050505050505050505050505050505050505050505050505050505050505",
                "0900000000000000"
            ),
        ),
        (
            factory_core::Message::ClaimCreatorAllocation {
                creator: AccountId::new([5; 32]),
                now: 9,
            },
            "ClaimCreatorAllocation",
            2,
            concat!(
                "02",
                "0505050505050505050505050505050505050505050505050505050505050505",
                "0900000000000000"
            ),
        ),
        (
            factory_core::Message::WithdrawFactoryProceeds {
                creator: AccountId::new([5; 32]),
                now: 9,
            },
            "WithdrawFactoryProceeds",
            3,
            concat!(
                "03",
                "0505050505050505050505050505050505050505050505050505050505050505",
                "0900000000000000"
            ),
        ),
    ] {
        assert_wire(message, encoded);
        assert_eq!(
            variant("factory", "Message", name),
            json!({
                "tag":tag,"name":name,"fields":[
                    {"name":"creator","type":"AccountId"},{"name":"now","type":"u64"}
                ]
            })
        );
    }
}

#[test]
fn graduation_execution_cannot_override_the_saved_adapter_on_wire() {
    assert_wire(
        factory_core::Message::ExecuteGraduationHook { now: 9 },
        "040900000000000000",
    );
    assert_eq!(
        variant("factory", "Message", "ExecuteGraduationHook"),
        json!({
            "tag":4,"name":"ExecuteGraduationHook","fields":[{"name":"now","type":"u64"}]
        })
    );
}

#[test]
fn fixed_price_settlement_keeps_creator_and_clock_on_wire() {
    assert_wire(
        lbp_core::Message::SettleFixedPrice {
            creator: AccountId::new([5; 32]),
            now: 9,
        },
        concat!(
            "07",
            "0505050505050505050505050505050505050505050505050505050505050505",
            "0900000000000000"
        ),
    );
    assert_eq!(
        variant("lbp", "Message", "SettleFixedPrice"),
        json!({
            "tag":7,"name":"SettleFixedPrice","fields":[
                {"name":"creator","type":"AccountId"},{"name":"now","type":"u64"}
            ]
        })
    );
}

#[test]
fn direct_pool_creation_keeps_namespace_owner_and_funding_terms_on_wire() {
    assert_wire(
        curve_core::Message::CreatePool {
            namespace: AccountId::new([1; 32]),
            token0_definition_id: AccountId::new([2; 32]),
            token1_definition_id: AccountId::new([3; 32]),
            allowed_input_token: Some(1),
            defer_funding: true,
            token0_amount: 4,
            token1_amount: 5,
            virtual_reserve0: 6,
            virtual_reserve1: 7,
            close_timestamp: Some(8),
            close_on_depletion: Some(0),
            owner: AccountId::new([9; 32]),
            owner_program_id: Some(AccountId::new([10; 32])),
            owner_program_seed: Some([11; 32]),
            curve_program_id: AccountId::new([12; 32]),
            now: 13,
        },
        concat!(
            "08",
            "0101010101010101010101010101010101010101010101010101010101010101",
            "0202020202020202020202020202020202020202020202020202020202020202",
            "0303030303030303030303030303030303030303030303030303030303030303",
            "010101",
            "04000000000000000000000000000000",
            "05000000000000000000000000000000",
            "06000000000000000000000000000000",
            "07000000000000000000000000000000",
            "010800000000000000",
            "0100",
            "0909090909090909090909090909090909090909090909090909090909090909",
            "01",
            "0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a",
            "01",
            "0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b",
            "0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c",
            "0d00000000000000"
        ),
    );
    assert_eq!(
        variant("curve", "Message", "CreatePool"),
        json!({
            "tag":8,"name":"CreatePool","fields":[
                {"name":"namespace","type":"AccountId"},
                {"name":"token0_definition_id","type":"AccountId"},
                {"name":"token1_definition_id","type":"AccountId"},
                {"name":"allowed_input_token","type":"Option<u8>"},
                {"name":"defer_funding","type":"bool"},
                {"name":"token0_amount","type":"u128"},{"name":"token1_amount","type":"u128"},
                {"name":"virtual_reserve0","type":"u128"},{"name":"virtual_reserve1","type":"u128"},
                {"name":"close_timestamp","type":"Option<u64>"},{"name":"close_on_depletion","type":"Option<u8>"},
                {"name":"owner","type":"AccountId"},{"name":"owner_program_id","type":"Option<AccountId>"},
                {"name":"owner_program_seed","type":"Option<[u8;32]>"},
                {"name":"curve_program_id","type":"AccountId"},{"name":"now","type":"u64"}
            ]
        })
    );
}

#[test]
fn factory_creation_keeps_creator_namespace_and_saved_hook_terms_on_wire() {
    assert_wire(
        factory_core::Message::CreateFactoryPool {
            trade_mode: 1,
            graduation_hook_program_id: Some(AccountId::new([1; 32])),
            graduation_hook_configuration: vec![2, 3],
            namespace: AccountId::new([4; 32]),
            launch_salt: [5; 32],
            name: "N".into(),
            uri: "U".into(),
            sale_reserve: 6,
            dex_seed_reserve: 7,
            creator_allocation: 8,
            virtual_token_reserve: 9,
            virtual_collateral_reserve: 10,
            end_timestamp: Some(11),
            curve_program_id: AccountId::new([12; 32]),
            collateral_definition_id: AccountId::new([13; 32]),
            creator: AccountId::new([14; 32]),
            now: 15,
        },
        concat!(
            "000101",
            "0101010101010101010101010101010101010101010101010101010101010101",
            "020000000203",
            "0404040404040404040404040404040404040404040404040404040404040404",
            "0505050505050505050505050505050505050505050505050505050505050505",
            "010000004e",
            "0100000055",
            "06000000000000000000000000000000",
            "07000000000000000000000000000000",
            "08000000000000000000000000000000",
            "09000000000000000000000000000000",
            "0a000000000000000000000000000000",
            "010b00000000000000",
            "0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c",
            "0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d",
            "0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e",
            "0f00000000000000"
        ),
    );
    assert_eq!(
        variant("factory", "Message", "CreateFactoryPool"),
        json!({
            "tag":0,"name":"CreateFactoryPool","fields":[
                {"name":"trade_mode","type":"u8"},
                {"name":"graduation_hook_program_id","type":"Option<AccountId>"},
                {"name":"graduation_hook_configuration","type":"Vec<u8>"},
                {"name":"namespace","type":"AccountId"},{"name":"launch_salt","type":"[u8;32]"},
                {"name":"name","type":"String"},{"name":"uri","type":"String"},
                {"name":"sale_reserve","type":"u128"},{"name":"dex_seed_reserve","type":"u128"},
                {"name":"creator_allocation","type":"u128"},{"name":"virtual_token_reserve","type":"u128"},
                {"name":"virtual_collateral_reserve","type":"u128"},{"name":"end_timestamp","type":"Option<u64>"},
                {"name":"curve_program_id","type":"AccountId"},{"name":"collateral_definition_id","type":"AccountId"},
                {"name":"creator","type":"AccountId"},{"name":"now","type":"u64"}
            ]
        })
    );
}

#[test]
fn graduation_custody_initialization_keeps_saved_sale_identity_on_wire() {
    assert_wire(
        factory_core::AdapterMessage::Initialize {
            factory_program_id: AccountId::new([1; 32]),
            namespace: AccountId::new([2; 32]),
            launch_salt: [3; 32],
            ready: true,
        },
        concat!(
            "00",
            "0101010101010101010101010101010101010101010101010101010101010101",
            "0202020202020202020202020202020202020202020202020202020202020202",
            "0303030303030303030303030303030303030303030303030303030303030303",
            "01"
        ),
    );
    assert_eq!(
        variant("hook", "AdapterMessage", "Initialize"),
        json!({
            "tag":0,"name":"Initialize","fields":[
                {"name":"factory_program_id","type":"AccountId"},{"name":"namespace","type":"AccountId"},
                {"name":"launch_salt","type":"[u8;32]"},{"name":"ready","type":"bool"}
            ]
        })
    );
}

#[test]
fn state_reads_keep_their_program_specific_tags_on_wire() {
    assert_wire(curve_core::Message::ReadState, "00");
    assert_wire(factory_core::Message::ReadState, "07");
    assert_wire(factory_core::AdapterMessage::ReadState, "01");
    for (module, message, tag) in [
        ("curve", "Message", 0),
        ("factory", "Message", 7),
        ("hook", "AdapterMessage", 1),
    ] {
        assert_eq!(
            variant(module, message, "ReadState"),
            json!({
                "tag":tag,"name":"ReadState","fields":[]
            })
        );
    }
}

fn assert_wire<T>(message: T, encoded: &str)
where
    T: borsh::BorshSerialize + borsh::BorshDeserialize + std::fmt::Debug + PartialEq,
{
    let expected = hex::decode(encoded).unwrap();
    assert_eq!(borsh::to_vec(&message).unwrap(), expected);
    assert_eq!(borsh::from_slice::<T>(&expected).unwrap(), message);
}

#[test]
fn namespace_fee_update_keeps_the_namespace_and_u32_rate_on_wire() {
    assert_wire(
        curve_core::Message::UpdateFee {
            namespace: AccountId::new([1; 32]),
            protocol_fee_bps: 0x01020304,
        },
        concat!(
            "0f",
            "0101010101010101010101010101010101010101010101010101010101010101",
            "04030201"
        ),
    );
    assert_eq!(
        variant("curve", "Message", "UpdateFee"),
        json!({"tag":15,"name":"UpdateFee","fields":[
            {"name":"namespace","type":"AccountId"},
            {"name":"protocol_fee_bps","type":"u32"}
        ]})
    );
}

#[test]
fn participant_purchase_envelope_keeps_deployment_pool_and_explicit_limits_on_wire() {
    let message = curve_core::Message::Execute {
        pool: AccountId::new([3; 32]),
        instruction: Box::new(curve_core::Message::SwapExactOutput {
            amount_out: 25,
            max_amount_in: 100,
            token_in: AccountId::new([4; 32]),
            now: 9,
        }),
    };
    let expected = hex::decode(concat!(
        "07",
        "0303030303030303030303030303030303030303030303030303030303030303",
        "04",
        "19000000000000000000000000000000",
        "64000000000000000000000000000000",
        "0404040404040404040404040404040404040404040404040404040404040404",
        "0900000000000000"
    ))
    .unwrap();
    assert_eq!(borsh::to_vec(&message).unwrap(), expected);
    assert_eq!(
        borsh::from_slice::<curve_core::Message>(&expected).unwrap(),
        message
    );
    assert_eq!(variant("curve", "Message", "Execute")["tag"], 7);
    assert_eq!(variant("curve", "Message", "SwapExactOutput")["tag"], 4);
    assert_eq!(
        variant("curve", "Message", "SwapExactOutput")["fields"],
        json!([
            {"name":"amount_out","type":"u128"}, {"name":"max_amount_in","type":"u128"},
            {"name":"token_in","type":"AccountId"}, {"name":"now","type":"u64"}
        ])
    );
}

#[test]
fn factory_resume_keeps_creator_stage_and_clock_lower_bound_on_wire() {
    let message = factory_core::Message::ContinueFixedPriceCreation {
        expected_stage: factory_core::FixedPriceCreationStage::PoolPrepared,
        creator: AccountId::new([5; 32]),
        now: 0,
    };
    let expected = hex::decode(concat!(
        "0602",
        "0505050505050505050505050505050505050505050505050505050505050505",
        "0000000000000000"
    ))
    .unwrap();
    assert_eq!(borsh::to_vec(&message).unwrap(), expected);
    assert_eq!(
        borsh::from_slice::<factory_core::Message>(&expected).unwrap(),
        message
    );
    assert_eq!(
        variant("factory", "Message", "ContinueFixedPriceCreation"),
        json!({
            "tag":6,"name":"ContinueFixedPriceCreation","fields":[
                {"name":"expected_stage","type":"FixedPriceCreationStage"},
                {"name":"creator","type":"AccountId"},{"name":"now","type":"u64"}
            ]
        })
    );
    assert_eq!(
        variant("fixed_price", "FixedPriceCreationStage", "PoolPrepared")["tag"],
        2
    );
}

#[test]
fn start_now_preserves_zero_and_scheduled_start_has_a_separate_tag() {
    let now = factory_core::SaleStart::Now { timestamp: 0 };
    let scheduled = factory_core::SaleStart::At { timestamp: 9 };
    let now_bytes = hex::decode("000000000000000000").unwrap();
    let scheduled_bytes = hex::decode("010900000000000000").unwrap();
    assert_eq!(borsh::to_vec(&now).unwrap(), now_bytes);
    assert_eq!(
        borsh::from_slice::<factory_core::SaleStart>(&now_bytes).unwrap(),
        now
    );
    assert_eq!(borsh::to_vec(&scheduled).unwrap(), scheduled_bytes);
    assert_eq!(
        borsh::from_slice::<factory_core::SaleStart>(&scheduled_bytes).unwrap(),
        scheduled
    );
    assert_eq!(
        variant("fixed_price", "SaleStart", "Now"),
        json!({"tag":0,"name":"Now","fields":[{"name":"timestamp","type":"u64"}]})
    );
    assert_eq!(variant("fixed_price", "SaleStart", "At")["tag"], 1);
}

#[test]
fn graduation_callback_has_a_typed_request_without_an_account_extension() {
    let message = factory_core::AdapterMessage::Graduate {
        request: factory_core::GraduationRequest {
            factory_program_id: AccountId::new([1; 32]),
            namespace: AccountId::new([2; 32]),
            launch_salt: [3; 32],
            token_definition_id: AccountId::new([4; 32]),
            collateral_definition_id: AccountId::new([5; 32]),
            dex_seed_reserve: 6,
            collateral_amount: 7,
            configuration: vec![8, 9],
        },
    };
    let expected = hex::decode(concat!(
        "02",
        "0101010101010101010101010101010101010101010101010101010101010101",
        "0202020202020202020202020202020202020202020202020202020202020202",
        "0303030303030303030303030303030303030303030303030303030303030303",
        "0404040404040404040404040404040404040404040404040404040404040404",
        "0505050505050505050505050505050505050505050505050505050505050505",
        "06000000000000000000000000000000",
        "07000000000000000000000000000000",
        "020000000809"
    ))
    .unwrap();
    assert_eq!(borsh::to_vec(&message).unwrap(), expected);
    assert_eq!(
        borsh::from_slice::<factory_core::AdapterMessage>(&expected).unwrap(),
        message
    );
    assert_eq!(
        variant("hook", "AdapterMessage", "Graduate"),
        json!({"tag":2,"name":"Graduate","fields":[{"name":"request","type":"GraduationRequest"}]})
    );
    assert_eq!(
        schema()["modules"]["hook"]["GraduationRequest"]["fields"],
        json!([
            {"name":"factory_program_id","type":"AccountId"},{"name":"namespace","type":"AccountId"},
            {"name":"launch_salt","type":"[u8;32]"},{"name":"token_definition_id","type":"AccountId"},
            {"name":"collateral_definition_id","type":"AccountId"},{"name":"dex_seed_reserve","type":"u128"},
            {"name":"collateral_amount","type":"u128"},{"name":"configuration","type":"Vec<u8>"}
        ])
    );
}

#[test]
fn fixed_price_lbp_price_and_activation_have_independent_wire_bytes() {
    let price = lbp_core::FixedPrice {
        collateral_units: 25,
        token_units: 3,
    };
    let expected =
        hex::decode("1900000000000000000000000000000003000000000000000000000000000000").unwrap();
    assert_eq!(borsh::to_vec(&price).unwrap(), expected);
    assert_eq!(
        borsh::from_slice::<lbp_core::FixedPrice>(&expected).unwrap(),
        price
    );
    assert_eq!(
        borsh::to_vec(&lbp_core::Message::ActivateFixedPrice).unwrap(),
        [1]
    );
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&[1]).unwrap(),
        lbp_core::Message::ActivateFixedPrice
    );
    assert_eq!(
        schema()["modules"]["lbp"]["FixedPrice"]["fields"],
        json!([
            {"name":"collateral_units","type":"u128"},{"name":"token_units","type":"u128"}
        ])
    );
    assert_eq!(
        variant("lbp", "Message", "ActivateFixedPrice"),
        json!({"tag":1,"name":"ActivateFixedPrice","fields":[]})
    );
}

#[test]
fn fixed_price_buys_and_collection_keep_explicit_limits_on_wire() {
    let exact_input = lbp_core::Message::BuyExactInput {
        amount_in: 100,
        min_amount_out: 25,
        now: 9,
    };
    let exact_output = lbp_core::Message::BuyExactOutput {
        amount_out: 25,
        max_amount_in: 100,
        now: 9,
    };
    for (message, encoded) in [
        (
            exact_input,
            concat!(
                "03",
                "64000000000000000000000000000000",
                "19000000000000000000000000000000",
                "0900000000000000"
            ),
        ),
        (
            exact_output,
            concat!(
                "04",
                "19000000000000000000000000000000",
                "64000000000000000000000000000000",
                "0900000000000000"
            ),
        ),
        (lbp_core::Message::CollectFees, "05"),
    ] {
        let expected = hex::decode(encoded).unwrap();
        assert_eq!(borsh::to_vec(&message).unwrap(), expected);
        assert_eq!(
            borsh::from_slice::<lbp_core::Message>(&expected).unwrap(),
            message
        );
    }
    assert_eq!(
        variant("lbp", "Message", "BuyExactInput"),
        json!({"tag":3,"name":"BuyExactInput","fields":[
            {"name":"amount_in","type":"u128"},
            {"name":"min_amount_out","type":"u128"},
            {"name":"now","type":"u64"}
        ]})
    );
    assert_eq!(
        variant("lbp", "Message", "BuyExactOutput"),
        json!({"tag":4,"name":"BuyExactOutput","fields":[
            {"name":"amount_out","type":"u128"},
            {"name":"max_amount_in","type":"u128"},
            {"name":"now","type":"u64"}
        ]})
    );
    assert_eq!(
        variant("lbp", "Message", "CollectFees"),
        json!({"tag":5,"name":"CollectFees","fields":[]})
    );
}

#[test]
fn fixed_price_forwarding_keeps_pool_and_purchase_on_wire() {
    let message = lbp_core::Message::Execute {
        pool: AccountId::new([3; 32]),
        instruction: Box::new(lbp_core::Message::BuyExactInput {
            amount_in: 100,
            min_amount_out: 25,
            now: 9,
        }),
    };
    let expected = hex::decode(concat!(
        "06",
        "0303030303030303030303030303030303030303030303030303030303030303",
        "03",
        "64000000000000000000000000000000",
        "19000000000000000000000000000000",
        "0900000000000000"
    ))
    .unwrap();
    assert_eq!(borsh::to_vec(&message).unwrap(), expected);
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&expected).unwrap(),
        message
    );
    assert_eq!(
        variant("lbp", "Message", "Execute"),
        json!({"tag":6,"name":"Execute","fields":[
            {"name":"pool","type":"AccountId"},
            {"name":"instruction","type":"Box<Message>"}
        ]})
    );
}

#[test]
fn fixed_price_optional_end_is_part_of_factory_and_pool_schemas() {
    for (module, name) in [
        ("fixed_price", "FixedPriceInputs"),
        ("fixed_price", "FixedPriceLaunchState"),
        ("lbp", "FixedPricePoolInputs"),
        ("lbp", "PoolAccount"),
    ] {
        let schema = schema();
        let field = schema["modules"][module][name]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|field| field["name"] == "end_timestamp")
            .unwrap();
        assert_eq!(field["type"], "Option<u64>", "{module}::{name}");
    }
    assert_eq!(borsh::to_vec(&None::<u64>).unwrap(), [0]);
    assert_eq!(
        borsh::to_vec(&Some(9u64)).unwrap(),
        [1, 9, 0, 0, 0, 0, 0, 0, 0]
    );
}

#[test]
fn namespace_initialization_has_independent_borsh_bytes_and_schema() {
    let message = curve_core::Message::InitializeNamespace {
        namespace: AccountId::new([1; 32]),
        protocol_fee_bps: 75,
        treasury: AccountId::new([2; 32]),
    };
    let expected = hex::decode(concat!(
        "10",
        "0101010101010101010101010101010101010101010101010101010101010101",
        "4b000000",
        "0202020202020202020202020202020202020202020202020202020202020202"
    ))
    .unwrap();
    assert_eq!(borsh::to_vec(&message).unwrap(), expected);
    assert_eq!(
        borsh::from_slice::<curve_core::Message>(&expected).unwrap(),
        message
    );
    assert_eq!(schema()["encoding"], "borsh");
    assert_eq!(schema()["version"], 1);
    assert_eq!(schema()["upstream_types"]["AccountId"], "[u8;32]");
    assert_eq!(
        variant("curve", "Message", "InitializeNamespace"),
        json!({
            "tag": 16, "name": "InitializeNamespace", "fields": [
                {"name": "namespace", "type": "AccountId"},
                {"name": "protocol_fee_bps", "type": "u32"},
                {"name": "treasury", "type": "AccountId"}
            ]
        })
    );
}
