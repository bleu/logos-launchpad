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
fn allowlist_policy_and_membership_have_fixed_independent_wire_bytes() {
    let policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: [2; 32],
        sale_policy_id: [3; 32],
        root: [4; 32],
    });
    let mut expected = vec![1, 1];
    expected.extend([2; 32]);
    expected.extend([3; 32]);
    expected.extend([4; 32]);
    assert_eq!(borsh::to_vec(&policy).unwrap(), expected);
    assert_eq!(
        borsh::from_slice::<curve_core::BuyPolicy>(&expected).unwrap(),
        policy
    );
    assert_eq!(borsh::to_vec(&curve_core::BuyPolicy::Open).unwrap(), [0]);
    assert_eq!(
        variant("eligibility_policy", "BuyPolicy", "Allowlist"),
        json!({
            "tag":1,"name":"Allowlist","fields":[{"name":"0","type":"AllowlistPolicy"}]
        })
    );
    let membership = Some(curve_core::MembershipPath {
        index: 0x01020304,
        salt: [7; 32],
        siblings: [[8; 32]; 17],
    });
    let mut expected = vec![1, 4, 3, 2, 1];
    expected.extend([7; 32]);
    expected.extend([8; 544]);
    assert_eq!(borsh::to_vec(&membership).unwrap(), expected);
    assert_eq!(
        borsh::from_slice::<Option<curve_core::MembershipPath>>(&expected).unwrap(),
        membership
    );
    assert_eq!(
        schema()["modules"]["eligibility_policy"]["MembershipPath"]["fields"],
        json!([
            {"name":"index","type":"u32"},{"name":"salt","type":"[u8;32]"},{"name":"siblings","type":"[[u8;32];17]"}
        ])
    );
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
            membership: None,
            now: 9,
        },
        concat!(
            "05",
            "64000000000000000000000000000000",
            "19000000000000000000000000000000",
            "0404040404040404040404040404040404040404040404040404040404040404",
            "00",
            "0900000000000000"
        ),
    );
    assert_eq!(
        variant("curve", "Message", "SwapExactInput"),
        json!({
            "tag":5,"name":"SwapExactInput","fields":[
                {"name":"amount_in","type":"u128"},{"name":"min_amount_out","type":"u128"},
                {"name":"token_in","type":"AccountId"},{"name":"membership","type":"Option<MembershipPath>"},{"name":"now","type":"u64"}
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
        factory_core::Message::ExecuteGraduationHook { creator_hint: None },
        "0400",
    );
    assert_eq!(
        variant("factory", "Message", "ExecuteGraduationHook"),
        json!({
            "tag":4,"name":"ExecuteGraduationHook","fields":[{"name":"creator_hint","type":"Option<AccountId>"}]
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
            buy_policy: curve_core::BuyPolicy::Open,
            pause_policy: curve_core::PausePolicy::Disabled,
            ending_policy: curve_core::EndingPolicy::Either,
            sell_fee_bps: 0x0102,
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
            "080000030201",
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
                {"name":"buy_policy","type":"BuyPolicy"},
                {"name":"pause_policy","type":"PausePolicy"},
                {"name":"ending_policy","type":"EndingPolicy"},
                {"name":"sell_fee_bps","type":"u16"},
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
            buy_policy: curve_core::BuyPolicy::Open,
            pause_policy: curve_core::PausePolicy::Disabled,
            ending_policy: curve_core::EndingPolicy::Either,
            trade_mode: 1,
            sell_fee_bps: 0x0102,
            graduation_hook_program_id: Some(AccountId::new([1; 32])),
            graduation_hook_configuration: vec![2, 3],
            namespace: AccountId::new([4; 32]),
            launch_salt: [5; 32],
            name: "N".into(),
            uri: "U".into(),
            graduation_reserve: 0,
            sale_reserve: 6,
            graduation_policy: launchpad_client::GraduationPolicy {
                launch_bps: 2500,
                collateral_bps: 7500,
            },
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
            "0000000301020101",
            "0101010101010101010101010101010101010101010101010101010101010101",
            "020000000203",
            "0404040404040404040404040404040404040404040404040404040404040404",
            "0505050505050505050505050505050505050505050505050505050505050505",
            "010000004e",
            "0100000055",
            "06000000000000000000000000000000",
            "00000000000000000000000000000000",
            "c4094c1d",
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
                {"name":"buy_policy","type":"BuyPolicy"},
                {"name":"pause_policy","type":"PausePolicy"},
                {"name":"ending_policy","type":"EndingPolicy"},
                {"name":"trade_mode","type":"u8"},
                {"name":"sell_fee_bps","type":"u16"},
                {"name":"graduation_hook_program_id","type":"Option<AccountId>"},
                {"name":"graduation_hook_configuration","type":"Vec<u8>"},
                {"name":"namespace","type":"AccountId"},{"name":"launch_salt","type":"[u8;32]"},
                {"name":"name","type":"String"},{"name":"uri","type":"String"},
                {"name":"sale_reserve","type":"u128"},{"name":"graduation_reserve","type":"u128"},{"name":"graduation_policy","type":"GraduationPolicy"},
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
            accepted_intent: factory_core::GraduationIntentKind::Curve,
        },
        concat!(
            "00",
            "0101010101010101010101010101010101010101010101010101010101010101",
            "0202020202020202020202020202020202020202020202020202020202020202",
            "0303030303030303030303030303030303030303030303030303030303030303",
            "00"
        ),
    );
    assert_eq!(
        variant("hook", "AdapterMessage", "Initialize"),
        json!({
            "tag":0,"name":"Initialize","fields":[
                {"name":"factory_program_id","type":"AccountId"},{"name":"namespace","type":"AccountId"},
                {"name":"launch_salt","type":"[u8;32]"},{"name":"accepted_intent","type":"GraduationIntentKind"}
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
    let mut trailing = expected;
    trailing.push(0);
    assert!(borsh::from_slice::<T>(&trailing).is_err());
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
            membership: None,
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
        "00",
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
            {"name":"token_in","type":"AccountId"}, {"name":"membership","type":"Option<MembershipPath>"}, {"name":"now","type":"u64"}
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
                {"name":"expected_stage","type":"LbpCreationStage"},
                {"name":"creator","type":"AccountId"},{"name":"now","type":"u64"}
            ]
        })
    );
    assert_eq!(
        variant("lbp_setup", "LbpCreationStage", "PoolPrepared")["tag"],
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
        variant("lbp_setup", "SaleStart", "Now"),
        json!({"tag":0,"name":"Now","fields":[{"name":"timestamp","type":"u64"}]})
    );
    assert_eq!(variant("lbp_setup", "SaleStart", "At")["tag"], 1);
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
            intent: factory_core::GraduationIntent::Curve {
                launch_tokens: 6,
                collateral: 7,
            },
            reserve_tokens: 0,
            unsold_tokens: 6,
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
        "00",
        "06000000000000000000000000000000",
        "07000000000000000000000000000000",
        "00000000000000000000000000000000",
        "06000000000000000000000000000000",
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
            {"name":"collateral_definition_id","type":"AccountId"},{"name":"intent","type":"GraduationIntent"},
            {"name":"reserve_tokens","type":"u128"},{"name":"unsold_tokens","type":"u128"},
            {"name":"configuration","type":"Vec<u8>"}
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
        membership: None,
        amount_in: 100,
        min_amount_out: 25,
        now: 9,
    };
    let exact_output = lbp_core::Message::BuyExactOutput {
        membership: None,
        amount_out: 25,
        max_amount_in: 100,
        now: 9,
    };
    for (message, encoded) in [
        (
            exact_input,
            concat!(
                "0300",
                "64000000000000000000000000000000",
                "19000000000000000000000000000000",
                "0900000000000000"
            ),
        ),
        (
            exact_output,
            concat!(
                "0400",
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
            {"name":"membership","type":"Option<MembershipPath>"},
            {"name":"amount_in","type":"u128"},
            {"name":"min_amount_out","type":"u128"},
            {"name":"now","type":"u64"}
        ]})
    );
    assert_eq!(
        variant("lbp", "Message", "BuyExactOutput"),
        json!({"tag":4,"name":"BuyExactOutput","fields":[
            {"name":"membership","type":"Option<MembershipPath>"},
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
            membership: None,
            amount_in: 100,
            min_amount_out: 25,
            now: 9,
        }),
    };
    let expected = hex::decode(concat!(
        "06",
        "0303030303030303030303030303030303030303030303030303030303030303",
        "0300",
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
        ("lbp_setup", "FixedPriceInputs"),
        ("lbp_setup", "LbpLaunchState"),
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
fn weighted_buys_keep_only_amount_limits_on_wire() {
    let exact_in = lbp_core::Message::BuyWeightedExactInput {
        membership: None,
        amount_in: 25,
        min_amount_out: 100,
    };
    let exact_out = lbp_core::Message::BuyWeightedExactOutput {
        membership: None,
        amount_out: 25,
        max_amount_in: 100,
    };
    for (message, name, tag, amount, limit) in [
        (
            exact_in,
            "BuyWeightedExactInput",
            "0a",
            "amount_in",
            "min_amount_out",
        ),
        (
            exact_out,
            "BuyWeightedExactOutput",
            "0b",
            "amount_out",
            "max_amount_in",
        ),
    ] {
        let mut expected = hex::decode(tag).unwrap();
        expected.push(0);
        expected.extend(
            hex::decode(concat!(
                "19000000000000000000000000000000",
                "64000000000000000000000000000000"
            ))
            .unwrap(),
        );
        assert_eq!(borsh::to_vec(&message).unwrap(), expected);
        assert_eq!(
            borsh::from_slice::<lbp_core::Message>(&expected).unwrap(),
            message
        );
        assert_eq!(
            variant("lbp", "Message", name),
            json!({
                "tag":expected[0], "name":name, "fields":[
                    {"name":"membership","type":"Option<MembershipPath>"},
                    {"name":amount,"type":"u128"}, {"name":limit,"type":"u128"}
                ]
            })
        );
        let mut old_wire = expected.clone();
        old_wire.extend(9u64.to_le_bytes());
        old_wire.extend(69u64.to_le_bytes());
        assert!(borsh::from_slice::<lbp_core::Message>(&old_wire).is_err());
        let envelope = lbp_core::Message::Execute {
            pool: AccountId::new([3; 32]),
            instruction: Box::new(message),
        };
        let mut expected_envelope = vec![6];
        expected_envelope.extend([3; 32]);
        expected_envelope.extend(expected);
        assert_eq!(borsh::to_vec(&envelope).unwrap(), expected_envelope);
        assert_eq!(
            borsh::from_slice::<lbp_core::Message>(&expected_envelope).unwrap(),
            envelope
        );
    }
}

#[test]
fn weighted_controls_settlement_and_poke_have_independent_wire_contracts() {
    let creator = AccountId::new([5; 32]);
    for (message, name, tag, fields) in [
        (
            lbp_core::Message::SetWeightedPaused { paused: true },
            "SetWeightedPaused",
            12,
            json!([{"name":"paused","type":"bool"}]),
        ),
        (
            lbp_core::Message::SettleWeighted { creator },
            "SettleWeighted",
            13,
            json!([{"name":"creator","type":"AccountId"}]),
        ),
        (
            lbp_core::Message::PokeWeighted,
            "PokeWeighted",
            14,
            json!([]),
        ),
    ] {
        let mut expected = vec![tag];
        match tag {
            12 => expected.push(1),
            13 => expected.extend([5; 32]),
            14 => (),
            _ => unreachable!(),
        }
        assert_eq!(borsh::to_vec(&message).unwrap(), expected);
        assert_eq!(
            borsh::from_slice::<lbp_core::Message>(&expected).unwrap(),
            message
        );
        assert_eq!(
            variant("lbp", "Message", name),
            json!({"tag":tag,"name":name,"fields":fields})
        );
    }
    for (message, name, tag, fields) in [
        (
            factory_core::Message::SetWeightedPaused {
                creator,
                paused: false,
            },
            "SetWeightedPaused",
            10,
            json!([{"name":"creator","type":"AccountId"},{"name":"paused","type":"bool"}]),
        ),
        (
            factory_core::Message::WithdrawWeightedProceeds { creator },
            "WithdrawWeightedProceeds",
            11,
            json!([{"name":"creator","type":"AccountId"}]),
        ),
        (
            factory_core::Message::ClaimWeightedAllocation { creator },
            "ClaimWeightedAllocation",
            12,
            json!([{"name":"creator","type":"AccountId"}]),
        ),
    ] {
        let mut expected = vec![tag];
        expected.extend([5; 32]);
        if tag == 10 {
            expected.push(0);
        }
        assert_eq!(borsh::to_vec(&message).unwrap(), expected);
        assert_eq!(
            borsh::from_slice::<factory_core::Message>(&expected).unwrap(),
            message
        );
        assert_eq!(
            variant("factory", "Message", name),
            json!({"tag":tag,"name":name,"fields":fields})
        );
    }
}

#[test]
fn weighted_pool_mode_keeps_pricing_and_observer_fields_on_wire() {
    let mode = lbp_core::PoolMode::Weighted {
        token_weight_start: 95,
        token_weight_end: 50,
        collateral_seed: 100,
        pricing_collateral: 150,
        checkpoint_time: 1500,
        checkpoint_token_weight: 75,
    };
    let mut expected = vec![1]; // Weighted
    for value in [95u128, 50, 100, 150] {
        expected.extend(value.to_le_bytes());
    }
    expected.extend(1500u64.to_le_bytes());
    expected.extend(75u128.to_le_bytes());
    assert_eq!(borsh::to_vec(&mode).unwrap(), expected);
    assert_eq!(
        borsh::from_slice::<lbp_core::PoolMode>(&expected).unwrap(),
        mode
    );
    assert_eq!(
        variant("lbp", "PoolMode", "Weighted"),
        json!({
            "tag":1,"name":"Weighted","fields":[
                {"name":"token_weight_start","type":"u128"},
                {"name":"token_weight_end","type":"u128"},
                {"name":"collateral_seed","type":"u128"},
                {"name":"pricing_collateral","type":"u128"},
                {"name":"checkpoint_time","type":"u64"},
                {"name":"checkpoint_token_weight","type":"u128"}
            ]
        })
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

#[test]
fn weighted_factory_messages_keep_distinct_tags_and_normalized_weights() {
    let creator = AccountId::new([5; 32]);
    let message = factory_core::Message::ContinueWeightedCreation {
        expected_stage: factory_core::LbpCreationStage::PoolPrepared,
        creator,
    };
    let mut expected = vec![9, 2];
    expected.extend_from_slice(&[5; 32]);
    assert_eq!(borsh::to_vec(&message).unwrap(), expected);
    assert_eq!(
        borsh::from_slice::<factory_core::Message>(&expected).unwrap(),
        message
    );
    assert_eq!(variant("factory", "Message", "ReadState")["tag"], 7);
    assert_eq!(variant("factory", "Message", "CreateWeightedLbp")["tag"], 8);
    assert_eq!(
        variant("factory", "Message", "ContinueWeightedCreation")["tag"],
        9
    );
    let inputs = factory_core::WeightedInputs {
        buy_policy: curve_core::BuyPolicy::Open,
        pause_policy: curve_core::PausePolicy::AuthorityControlled,
        ending_policy: curve_core::EndingPolicy::Scheduled,
        trade_mode: factory_core::TradeMode::BuyOnly,
        sell_fee_bps: 0x0102,
        graduation_hook: None,
        graduation_policy: launchpad_client::GraduationPolicy::default(),
        namespace: creator,
        launch_salt: [6; 32],
        name: "W".into(),
        uri: "u".into(),
        graduation_reserve: 0,
        sale_inventory: 800,
        creator_allocation: 200,
        collateral_seed: 100,
        token_weight_start: lbp_core::WEIGHT_TOTAL / 2 + 1,
        token_weight_end: lbp_core::WEIGHT_TOTAL / 2 - 1,
        start_timestamp: 150,
        end_timestamp: 300,
        allocation_release: factory_core::CreatorAllocationRelease::AfterEnd,
        namespace_program_id: creator,
        lbp_program_id: creator,
        collateral_definition_id: creator,
        creator,
    };
    let message = factory_core::Message::CreateWeightedLbp {
        inputs: inputs.clone(),
    };
    let mut expected = vec![8, 0, 1, 1, 0, 2, 1, 0, 0, 0, 0, 0];
    expected.extend_from_slice(creator.value());
    expected.extend_from_slice(&[6; 32]);
    expected.extend_from_slice(&[1, 0, 0, 0, b'W', 1, 0, 0, 0, b'u']);
    for value in [
        800u128,
        0,
        200,
        100,
        inputs.token_weight_start,
        inputs.token_weight_end,
    ] {
        expected.extend_from_slice(&value.to_le_bytes());
    }
    expected.extend_from_slice(&150u64.to_le_bytes());
    expected.extend_from_slice(&300u64.to_le_bytes());
    expected.push(1); // AfterEnd
    for _ in 0..4 {
        expected.extend_from_slice(creator.value());
    }
    assert_eq!(borsh::to_vec(&message).unwrap(), expected);
    assert_eq!(
        borsh::from_slice::<factory_core::Message>(&expected).unwrap(),
        message
    );
}

#[test]
fn shared_pause_controls_have_appended_tags_and_keep_weighted_aliases() {
    for (paused, suffix) in [(false, "00"), (true, "01")] {
        assert_wire(
            curve_core::Message::SetPaused { paused },
            &format!("11{suffix}"),
        );
        assert_wire(
            lbp_core::Message::SetPaused { paused },
            &format!("0f{suffix}"),
        );
        assert_wire(
            factory_core::Message::SetPaused {
                creator: AccountId::new([5; 32]),
                paused,
            },
            &format!("0d{}{suffix}", "05".repeat(32)),
        );
    }
    for (module, tag) in [("curve", 17), ("lbp", 15)] {
        assert_eq!(
            variant(module, "Message", "SetPaused"),
            json!({
                "tag":tag,"name":"SetPaused","fields":[{"name":"paused","type":"bool"}]
            })
        );
    }
    assert_eq!(
        variant("factory", "Message", "SetPaused"),
        json!({
            "tag":13,"name":"SetPaused","fields":[
                {"name":"creator","type":"AccountId"},{"name":"paused","type":"bool"}
            ]
        })
    );
}

#[test]
fn pause_policy_and_trade_control_have_independent_wire_tags() {
    for (policy, tag, name) in [
        (curve_core::PausePolicy::Disabled, 0, "Disabled"),
        (
            curve_core::PausePolicy::AuthorityControlled,
            1,
            "AuthorityControlled",
        ),
    ] {
        assert_wire(policy, &format!("{tag:02x}"));
        assert_eq!(
            variant("controls", "PausePolicy", name),
            json!({"tag":tag,"name":name,"fields":[]})
        );
    }
    for (control, tag, name) in [
        (curve_core::TradeControl::Unrestricted, 0, "Unrestricted"),
        (curve_core::TradeControl::Running, 1, "Running"),
        (curve_core::TradeControl::Paused, 2, "Paused"),
    ] {
        assert_wire(control, &format!("{tag:02x}"));
        assert_eq!(
            variant("controls", "TradeControl", name),
            json!({"tag":tag,"name":name,"fields":[]})
        );
    }
    for (module, name, next) in [
        ("factory", "FactoryState", "pause_policy"),
        ("lbp_setup", "FixedPriceInputs", "pause_policy"),
        ("lbp_setup", "WeightedInputs", "pause_policy"),
        ("lbp_setup", "LbpLaunchState", "pause_policy"),
        ("lbp", "FixedPricePoolInputs", "pause_policy"),
        ("lbp", "WeightedPoolInputs", "pause_policy"),
        ("curve", "PoolAccount", "trade_control"),
        ("lbp", "PoolAccount", "trade_control"),
    ] {
        let schema = schema();
        let fields = schema["modules"][module][name]["fields"]
            .as_array()
            .unwrap();
        let index = fields
            .iter()
            .position(|field| field["name"] == "buy_policy")
            .unwrap();
        assert_eq!(
            fields[index + 1],
            json!({
                "name":next,"type":if next == "pause_policy" { "PausePolicy" } else { "TradeControl" }
            }),
            "{module}::{name}"
        );
    }
}

#[test]
fn ending_policies_have_independent_wire_tags() {
    let schema = schema();
    let imports = schema["imports"]["sale_policy"].as_array().unwrap();
    assert!(imports.contains(&json!("controls")));
    assert!(imports.contains(&json!("terms")));
    for (policy, tag, name) in [
        (curve_core::EndingPolicy::Manual, 0, "Manual"),
        (curve_core::EndingPolicy::Scheduled, 1, "Scheduled"),
        (curve_core::EndingPolicy::Depletion, 2, "Depletion"),
        (curve_core::EndingPolicy::Either, 3, "Either"),
    ] {
        assert_wire(policy, &format!("{tag:02x}"));
        assert_eq!(
            variant("terms", "EndingPolicy", name),
            json!({"tag":tag,"name":name,"fields":[]})
        );
    }
}

#[test]
fn immutable_policy_fields_follow_controls_except_inside_the_curve_pool() {
    let schema = schema();
    for (module, name, previous) in [
        ("factory", "FactoryState", "pause_policy"),
        ("lbp_setup", "FixedPriceInputs", "pause_policy"),
        ("lbp_setup", "WeightedInputs", "pause_policy"),
        ("lbp_setup", "LbpLaunchState", "pause_policy"),
        ("lbp", "FixedPricePoolInputs", "pause_policy"),
        ("lbp", "WeightedPoolInputs", "pause_policy"),
        ("lbp", "PoolAccount", "trade_control"),
        ("pool", "Pool", "fees_collected"),
    ] {
        let fields = schema["modules"][module][name]["fields"]
            .as_array()
            .unwrap();
        let index = fields
            .iter()
            .position(|field| field["name"] == previous)
            .unwrap();
        assert_eq!(
            fields[index + 1],
            json!({"name":"ending_policy","type":"EndingPolicy"}),
            "{module}::{name}"
        );
        if module == "pool" {
            assert_eq!(
                fields[index + 2],
                json!({"name":"sell_fee_bps","type":"u16"})
            );
            assert_eq!(fields[index + 3]["name"], "close_timestamp");
        }
    }
    let outer = schema["modules"]["curve"]["PoolAccount"]["fields"]
        .as_array()
        .unwrap();
    assert!(
        outer
            .iter()
            .all(|field| field["name"] != "fee_policy" && field["name"] != "ending_policy")
    );
}

#[test]
fn fixed_pool_state_tuple_places_policies_after_trade_control() {
    let account = AccountId::new([5; 32]);
    let mut bytes = borsh::to_vec(&(
        curve_core::BuyPolicy::Open,
        curve_core::TradeControl::Unrestricted,
        lbp_core::EndingPolicy::Either,
        factory_core::TradeMode::BuyOnly,
        0u16,
        None::<lee_core::account::Actor>,
        launchpad_client::GraduationPolicy::default(),
        lbp_core::PoolMode::FixedPrice,
        (true, false),
        (
            account, account, account, account, [6u8; 32], account, account,
        ),
        (800u128, 800u128, 200u128),
        lbp_core::FixedPrice {
            collateral_units: 25,
            token_units: 10,
        },
        (
            9u64,
            Some(100u64),
            lbp_core::CreatorAllocationRelease::AfterEnd,
        ),
        (0u128, 0u128, lbp_core::SettlementProgress::Unselected),
    ))
    .unwrap();
    bytes.push(0); // No pending operation.
    let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    assert!(!pool.closure_observed);
    assert_eq!(pool.ending_policy, lbp_core::EndingPolicy::Either);
    assert_eq!(pool.remaining_inventory, 800);
    assert_eq!(pool.graduation_adapter, None);
    assert_eq!(
        pool.settlement_progress,
        lbp_core::SettlementProgress::Unselected
    );
    assert_eq!(&bytes[..3], &[0, 0, 3]);
    assert_eq!(borsh::to_vec(&pool).unwrap(), bytes);
}

#[test]
fn settlement_progress_has_explicit_wire_tags_and_a_shared_schema() {
    let schema = schema();
    assert!(
        schema["imports"]["sale_policy"]
            .as_array()
            .unwrap()
            .contains(&json!("settlement"))
    );
    for (progress, tag, name) in [
        (lbp_core::SettlementProgress::Unselected, 0, "Unselected"),
        (
            lbp_core::SettlementProgress::ManualStarted,
            1,
            "ManualStarted",
        ),
        (
            lbp_core::SettlementProgress::ManualComplete,
            2,
            "ManualComplete",
        ),
        (lbp_core::SettlementProgress::Graduated, 3, "Graduated"),
    ] {
        assert_wire(progress, &format!("{tag:02x}"));
        assert_eq!(
            variant("settlement", "SettlementProgress", name),
            json!({"tag":tag,"name":name,"fields":[]})
        );
    }
    assert_eq!(
        schema["modules"]["settlement"]["SettlementProgress"]["variants"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert!(borsh::from_slice::<lbp_core::SettlementProgress>(&[4]).is_err());
    let fields = schema["modules"]["lbp"]["PoolAccount"]["fields"]
        .as_array()
        .unwrap();
    assert!(
        fields
            .iter()
            .all(|field| field["name"] != "proceeds_settled")
    );
    let index = fields
        .iter()
        .position(|field| field["name"] == "accrued_fees")
        .unwrap();
    assert_eq!(
        fields[index + 1],
        json!({"name":"settlement_progress","type":"SettlementProgress"})
    );
    assert_eq!(fields[index + 2]["name"], "pending");
}

#[test]
fn fixed_pool_terminal_states_use_progress_tags_instead_of_boolean_completion() {
    let account = AccountId::new([5; 32]);
    for (progress, tag) in [
        (lbp_core::SettlementProgress::ManualComplete, 2),
        (lbp_core::SettlementProgress::Graduated, 3),
    ] {
        let mut bytes = borsh::to_vec(&(
            curve_core::BuyPolicy::Open,
            curve_core::TradeControl::Unrestricted,
            lbp_core::EndingPolicy::Either,
            factory_core::TradeMode::BuyOnly,
            0u16,
            None::<lee_core::account::Actor>,
            launchpad_client::GraduationPolicy::default(),
            lbp_core::PoolMode::FixedPrice,
            (true, true),
            (
                account, account, account, account, [6u8; 32], account, account,
            ),
            (800u128, 0u128, 200u128),
            lbp_core::FixedPrice {
                collateral_units: 25,
                token_units: 10,
            },
            (
                9u64,
                Some(100u64),
                lbp_core::CreatorAllocationRelease::AfterEnd,
            ),
            (0u128, 0u128, progress),
        ))
        .unwrap();
        bytes.push(0);
        let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
        assert!(pool.closure_observed);
        assert_eq!(pool.settlement_progress, progress);
        assert_eq!(pool.remaining_inventory, 0);
        assert_eq!(bytes[bytes.len() - 2], tag);
        assert_eq!(borsh::to_vec(&pool).unwrap(), bytes);
    }
}

#[test]
fn curve_settlement_stages_preserve_the_unsold_snapshot() {
    use factory_core::SettlementStage;
    for (stage, tag, name, payload_len) in [
        (SettlementStage::Unstarted, 0, "Unstarted", 0),
        (SettlementStage::Ready, 1, "Ready", 0),
        (
            SettlementStage::Withdrawn {
                unsold: 0,
                collateral_due: 0,
            },
            2,
            "Withdrawn",
            32,
        ),
        (
            SettlementStage::HookReady {
                unsold: 0,
                collateral_due: 0,
            },
            3,
            "HookReady",
            32,
        ),
        (SettlementStage::Complete, 4, "Complete", 0),
        (SettlementStage::Graduated, 5, "Graduated", 0),
    ] {
        assert_wire(stage, &format!("{tag:02x}{}", "00".repeat(payload_len)));
        assert_eq!(variant("factory", "SettlementStage", name)["tag"], tag);
    }
    assert_eq!(
        schema()["modules"]["factory"]["SettlementStage"]["variants"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert!(borsh::from_slice::<SettlementStage>(&[6]).is_err());
}

#[test]
fn graduation_intent_kinds_and_amounts_have_independent_wire_tags() {
    use factory_core::{GraduationIntent, GraduationIntentKind};
    for (kind, intent, tag, name, fields, payload) in [
        (
            GraduationIntentKind::Curve,
            GraduationIntent::Curve {
                launch_tokens: 6,
                collateral: 7,
            },
            0,
            "Curve",
            json!([{"name":"launch_tokens","type":"u128"},{"name":"collateral","type":"u128"}]),
            "0600000000000000000000000000000007000000000000000000000000000000",
        ),
        (
            GraduationIntentKind::FixedPrice,
            GraduationIntent::FixedPrice {
                launch_tokens: 6,
                collateral: 7,
            },
            1,
            "FixedPrice",
            json!([{"name":"launch_tokens","type":"u128"},{"name":"collateral","type":"u128"}]),
            "0600000000000000000000000000000007000000000000000000000000000000",
        ),
        (
            GraduationIntentKind::Weighted,
            GraduationIntent::Weighted {
                launch_tokens: 6,
                collateral: 7,
            },
            2,
            "Weighted",
            json!([{"name":"launch_tokens","type":"u128"},{"name":"collateral","type":"u128"}]),
            "0600000000000000000000000000000007000000000000000000000000000000",
        ),
    ] {
        assert_eq!(intent.kind(), kind);
        assert_eq!(intent.amounts(), (6, 7));
        assert_wire(kind, &format!("{tag:02x}"));
        assert_wire(intent, &format!("{tag:02x}{payload}"));
        assert_eq!(
            variant("hook", "GraduationIntentKind", name),
            json!({"tag":tag,"name":name,"fields":[]})
        );
        assert_eq!(
            variant("hook", "GraduationIntent", name),
            json!({"tag":tag,"name":name,"fields":fields})
        );
        let message = factory_core::AdapterMessage::Initialize {
            factory_program_id: AccountId::new([1; 32]),
            namespace: AccountId::new([2; 32]),
            launch_salt: [3; 32],
            accepted_intent: kind,
        };
        assert_wire(
            message,
            &format!(
                "00{}{}{}{tag:02x}",
                "01".repeat(32),
                "02".repeat(32),
                "03".repeat(32)
            ),
        );
    }
}

#[test]
fn graduation_result_binds_the_factory_to_the_exact_typed_request_hash() {
    let request = factory_core::GraduationRequest {
        factory_program_id: AccountId::new([1; 32]),
        namespace: AccountId::new([2; 32]),
        launch_salt: [3; 32],
        token_definition_id: AccountId::new([4; 32]),
        collateral_definition_id: AccountId::new([5; 32]),
        intent: factory_core::GraduationIntent::FixedPrice {
            launch_tokens: 6,
            collateral: 7,
        },
        reserve_tokens: 0,
        unsold_tokens: 6,
        configuration: vec![8, 9],
    };
    let expected_hash =
        hex::decode("a07afd3311c8822717e2622333cf21955f9596ee412620f1ee27c2b20180e92a").unwrap();
    assert_eq!(request.hash().as_slice(), expected_hash);
    let result = factory_core::GraduationResult {
        factory: AccountId::new([6; 32]),
        request_hash: request.hash(),
    };
    assert_wire(
        result,
        &format!("{}{}", "06".repeat(32), hex::encode(&expected_hash)),
    );
    assert_eq!(
        schema()["modules"]["hook"]["GraduationResult"]["fields"],
        json!([
            {"name":"factory","type":"AccountId"},{"name":"request_hash","type":"[u8;32]"}
        ])
    );
}

#[test]
fn lbp_graduation_hint_is_optional_data_at_the_existing_factory_tag() {
    assert_wire(
        factory_core::Message::ExecuteGraduationHook {
            creator_hint: Some(AccountId::new([5; 32])),
        },
        &format!("0401{}", "05".repeat(32)),
    );
}

#[test]
fn immutable_graduation_terms_follow_the_ending_policy_on_wire() {
    let schema = schema();
    for (module, name, next, ty) in [
        (
            "lbp_setup",
            "FixedPriceInputs",
            "graduation_hook",
            "Option<GraduationHook>",
        ),
        (
            "lbp_setup",
            "WeightedInputs",
            "graduation_hook",
            "Option<GraduationHook>",
        ),
        (
            "lbp_setup",
            "LbpLaunchState",
            "graduation_hook",
            "Option<GraduationHook>",
        ),
        (
            "lbp",
            "FixedPricePoolInputs",
            "graduation_adapter",
            "Option<Actor>",
        ),
        (
            "lbp",
            "WeightedPoolInputs",
            "graduation_adapter",
            "Option<Actor>",
        ),
        ("lbp", "PoolAccount", "graduation_adapter", "Option<Actor>"),
    ] {
        let fields = schema["modules"][module][name]["fields"]
            .as_array()
            .unwrap();
        let index = fields
            .iter()
            .position(|field| field["name"] == "ending_policy")
            .unwrap();
        assert_eq!(
            fields[index + 1],
            json!({"name":"trade_mode","type":"TradeMode"}),
            "{module}::{name}"
        );
        assert_eq!(
            fields[index + 2],
            json!({"name":"sell_fee_bps","type":"u16"}),
            "{module}::{name}"
        );
        assert_eq!(
            fields[index + 3],
            json!({"name":next,"type":ty}),
            "{module}::{name}"
        );
    }
    let fields = schema["modules"]["lbp_setup"]["LbpLaunchState"]["fields"]
        .as_array()
        .unwrap();
    let index = fields
        .iter()
        .position(|field| field["name"] == "creation_stage")
        .unwrap();
    assert_eq!(
        fields[index + 1],
        json!({"name":"settlement_progress","type":"SettlementProgress"})
    );
}

#[test]
fn lbp_settlement_and_finalize_append_message_tags_without_clock_or_signer_fields() {
    let adapter = lee_core::account::Actor::new(AccountId::new([6; 32]), AccountId::new([7; 32]));
    assert_wire(
        lbp_core::Message::Settle {
            route: lbp_core::SettlementRoute::Graduation,
            creator: AccountId::new([5; 32]),
            adapter: Some(adapter),
        },
        &format!(
            "1001{}01{}{}",
            "05".repeat(32),
            "06".repeat(32),
            "07".repeat(32)
        ),
    );
    assert_wire(
        lbp_core::Message::FinalizeSettlement {
            receipt_hash: [8; 32],
        },
        &format!("11{}", "08".repeat(32)),
    );
    assert_eq!(
        variant("lbp", "Message", "Settle"),
        json!({"tag":16,"name":"Settle","fields":[
            {"name":"route","type":"SettlementRoute"},{"name":"creator","type":"AccountId"},{"name":"adapter","type":"Option<Actor>"}
        ]})
    );
    assert_eq!(
        variant("lbp", "Message", "FinalizeSettlement"),
        json!({"tag":17,"name":"FinalizeSettlement","fields":[
            {"name":"receipt_hash","type":"[u8;32]"}
        ]})
    );
}

#[test]
fn weighted_pool_binds_the_split_and_keeps_seed_in_recorded_collateral() {
    let account = AccountId::new([5; 32]);
    let adapter = lee_core::account::Actor::new(AccountId::new([6; 32]), AccountId::new([7; 32]));
    let mut bytes = borsh::to_vec(&(
        curve_core::BuyPolicy::Open,
        curve_core::TradeControl::Unrestricted,
        lbp_core::EndingPolicy::Scheduled,
        factory_core::TradeMode::BuyOnly,
        0u16,
        Some(adapter),
        launchpad_client::GraduationPolicy {
            launch_bps: 2500,
            collateral_bps: 7500,
        },
        lbp_core::PoolMode::Weighted {
            token_weight_start: lbp_core::WEIGHT_TOTAL / 2 + 1,
            token_weight_end: lbp_core::WEIGHT_TOTAL / 2 - 1,
            collateral_seed: 100,
            pricing_collateral: 140,
            checkpoint_time: 9,
            checkpoint_token_weight: lbp_core::WEIGHT_TOTAL / 2 + 1,
        },
        (true, false),
        (
            account, account, account, account, [6u8; 32], account, account,
        ),
        (800u128, 700u128, 200u128),
        lbp_core::FixedPrice {
            collateral_units: 0,
            token_units: 0,
        },
        (
            9u64,
            Some(100u64),
            lbp_core::CreatorAllocationRelease::AfterEnd,
        ),
        (140u128, 0u128, lbp_core::SettlementProgress::Unselected),
    ))
    .unwrap();
    bytes.push(0);
    let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    assert!(!pool.closure_observed);
    assert_eq!(&bytes[..7], &[0, 0, 1, 0, 0, 0, 1]);
    assert_eq!(pool.graduation_adapter, Some(adapter));
    assert_eq!(pool.recorded_collateral, 140);
    assert_eq!(pool.graduation_policy.collateral_bps, 7500);
    assert_eq!(borsh::to_vec(&pool).unwrap(), bytes);
}

#[test]
fn closure_observation_follows_funding_in_both_pool_state_schemas() {
    let schema = schema();
    for module in ["curve", "lbp"] {
        let fields = schema["modules"][module]["PoolAccount"]["fields"]
            .as_array()
            .unwrap();
        let index = fields
            .iter()
            .position(|field| field["name"] == "funded")
            .unwrap();
        assert_eq!(
            fields[index + 1],
            json!({"name":"closure_observed","type":"bool"})
        );
        assert_eq!(fields[index + 2]["name"], "namespace");
    }
}

#[test]
fn sale_event_envelope_has_a_versioned_selector_and_independent_wire_bytes() {
    use curve_core::events::{EventSubjectV1, PricingEngineV1, SaleEventKindV1, SaleEventV1};
    let event = SaleEventV1::new(
        [2; 32],
        PricingEngineV1::Weighted,
        EventSubjectV1::Launch {
            factory: [3; 32],
            pool: [4; 32],
        },
        SaleEventKindV1::PauseChanged { paused: true },
    );
    let encoded = format!(
        "01{}0200{}{}0401",
        "02".repeat(32),
        "03".repeat(32),
        "04".repeat(32)
    );
    assert_wire(event.clone(), &encoded);
    assert_eq!(SaleEventV1::VERSION, 1);
    assert_eq!(SaleEventV1::SELECTOR_NAME, "launchpad::SaleEventV1");
    let runtime = curve_core::sale_event(event);
    assert_eq!(
        runtime.selector,
        [0xe1, 0x96, 0x1d, 0x35, 0xac, 0x82, 0x20, 0x6c]
    );
    assert_eq!(runtime.data, hex::decode(encoded).unwrap());
    assert_eq!(
        schema()["modules"]["events"]["SaleEventV1"]["fields"],
        json!([
            {"name":"version","type":"u8"}, {"name":"namespace","type":"[u8;32]"},
            {"name":"engine","type":"PricingEngineV1"}, {"name":"subject","type":"EventSubjectV1"},
            {"name":"kind","type":"SaleEventKindV1"}
        ])
    );
    assert!(
        schema()["imports"]["sale_policy"]
            .as_array()
            .unwrap()
            .contains(&json!("events"))
    );
}

#[test]
fn event_engines_subjects_and_policy_fields_have_fixed_wire_order() {
    use curve_core::events::{EventPoliciesV1, EventSubjectV1, PricingEngineV1};
    for (engine, tag, name) in [
        (PricingEngineV1::Curve, 0, "Curve"),
        (PricingEngineV1::FixedPrice, 1, "FixedPrice"),
        (PricingEngineV1::Weighted, 2, "Weighted"),
    ] {
        assert_wire(engine, &format!("{tag:02x}"));
        assert_eq!(
            variant("events", "PricingEngineV1", name),
            json!({"tag":tag,"name":name,"fields":[]})
        );
    }
    assert_wire(
        EventSubjectV1::Launch {
            factory: [3; 32],
            pool: [4; 32],
        },
        &format!("00{}{}", "03".repeat(32), "04".repeat(32)),
    );
    assert_wire(
        EventSubjectV1::Pool { pool: [4; 32] },
        &format!("01{}", "04".repeat(32)),
    );
    assert_eq!(
        variant("events", "EventSubjectV1", "Launch"),
        json!({"tag":0,"name":"Launch","fields":[
            {"name":"factory","type":"[u8;32]"},{"name":"pool","type":"[u8;32]"}
        ]})
    );
    assert_eq!(
        variant("events", "EventSubjectV1", "Pool"),
        json!({"tag":1,"name":"Pool","fields":[{"name":"pool","type":"[u8;32]"}]})
    );
    assert_wire(
        EventPoliciesV1 {
            ending: curve_core::EndingPolicy::Scheduled,
            pause: curve_core::PausePolicy::AuthorityControlled,
            sell_fee_bps: 0x0304,
        },
        "01010403",
    );
    assert_eq!(
        schema()["modules"]["events"]["EventPoliciesV1"]["fields"],
        json!([
            {"name":"ending","type":"EndingPolicy"},{"name":"pause","type":"PausePolicy"},
            {"name":"sell_fee_bps","type":"u16"}
        ])
    );
}

#[test]
fn event_kinds_keep_creation_trade_and_settlement_amounts_distinct_on_wire() {
    use curve_core::events::*;
    let policies = EventPoliciesV1 {
        ending: curve_core::EndingPolicy::Scheduled,
        pause: curve_core::PausePolicy::AuthorityControlled,
        sell_fee_bps: 0x0304,
    };
    let a = "01".repeat(32);
    let b = "02".repeat(32);
    let c = "03".repeat(32);
    let d = "04".repeat(32);
    let n = |value: u128| hex::encode(value.to_le_bytes());
    for (kind, tag, name, payload) in [
        (
            SaleEventKindV1::Created {
                creator: [1; 32],
                creator_role: [2; 32],
                token_definition: [3; 32],
                collateral_definition: [4; 32],
                graduation_reserve: 0,
                sale_inventory: 5,
                creator_allocation: 6,
                graduation_policy: launchpad_client::GraduationPolicy {
                    launch_bps: 2500,
                    collateral_bps: 7500,
                },
                collateral_seed: 8,
                start_timestamp: None,
                end_timestamp: Some(9),
                policies,
            },
            0,
            "Created",
            format!(
                "{a}{b}{c}{d}{}{}{}{}{}0001090000000000000001010403",
                n(5),
                n(0),
                n(6),
                "c4094c1d",
                n(8)
            ),
        ),
        (
            SaleEventKindV1::Prepared {
                owner: [1; 32],
                token0_definition: [2; 32],
                token1_definition: [3; 32],
                reserve0: 5,
                reserve1: 6,
                policies,
            },
            1,
            "Prepared",
            format!("{a}{b}{c}{}{}01010403", n(5), n(6)),
        ),
        (
            SaleEventKindV1::Activated {
                reserve0: 5,
                reserve1: 6,
                collateral_seed: 7,
            },
            2,
            "Activated",
            format!("{}{}{}", n(5), n(6), n(7)),
        ),
        (
            SaleEventKindV1::Trade {
                trader: [1; 32],
                form: TradeFormV1::ExactOutput,
                token_in: [2; 32],
                token_out: [3; 32],
                amount_in: 5,
                pricing_input: 6,
                gross_output: 7,
                amount_out: 8,
                fee_definition: [4; 32],
                fee_amount: 9,
                fee_bps: 0x0102,
                retained_fee: 11,
                sell_fee_bps: 0x0304,
                pricing_time: Some(10),
            },
            3,
            "Trade",
            format!(
                "{a}01{b}{c}{}{}{}{}{d}{}0201{}0403010a00000000000000",
                n(5),
                n(6),
                n(7),
                n(8),
                n(9),
                n(11)
            ),
        ),
        (
            SaleEventKindV1::PauseChanged { paused: true },
            4,
            "PauseChanged",
            "01".into(),
        ),
        (
            SaleEventKindV1::FeesCollected {
                definition: [1; 32],
                treasury: [2; 32],
                liability_discharged: 5,
                tokens_transferred: 0,
            },
            5,
            "FeesCollected",
            format!("{a}{b}{}{}", n(5), n(0)),
        ),
        (
            SaleEventKindV1::AllocationReleased {
                definition: [1; 32],
                recipient: [2; 32],
                amount: 5,
                release: AllocationReleaseV1::AfterEnd,
            },
            6,
            "AllocationReleased",
            format!("{a}{b}{}01", n(5)),
        ),
        (
            SaleEventKindV1::Closed {
                reason: ClosureReasonV1::ScheduledEnd { timestamp: 9 },
                inclusion_bounds: EventTimeBoundsV1 {
                    at_least: Some(9),
                    before: None,
                },
            },
            7,
            "Closed",
            "02090000000000000001090000000000000000".into(),
        ),
        (
            SaleEventKindV1::Settlement {
                step: SettlementStepV1::Completed,
                route: Some(SettlementRouteV1::Manual),
                effects: vec![],
            },
            8,
            "Settlement",
            "03010000000000".into(),
        ),
        (
            SaleEventKindV1::Graduated {
                adapter_program: [1; 32],
                adapter_custody: [2; 32],
                launch_tokens: 5,
                collateral: 6,
            },
            9,
            "Graduated",
            format!("{a}{b}{}{}", n(5), n(6)),
        ),
    ] {
        assert_wire(kind, &format!("{tag:02x}{payload}"));
        assert_eq!(variant("events", "SaleEventKindV1", name)["tag"], tag);
    }
}

#[test]
fn event_closure_routes_steps_and_effects_keep_independent_tags() {
    use curve_core::events::*;
    for (reason, encoded) in [
        (ClosureReasonV1::Manual, "00".into()),
        (
            ClosureReasonV1::Depleted {
                definition: [3; 32],
            },
            format!("01{}", "03".repeat(32)),
        ),
        (
            ClosureReasonV1::ScheduledEnd { timestamp: 9 },
            "020900000000000000".into(),
        ),
    ] {
        assert_wire(reason, &encoded);
    }
    for (step, tag, name) in [
        (SettlementStepV1::Prepared, 0, "Prepared"),
        (SettlementStepV1::ReservesWithdrawn, 1, "ReservesWithdrawn"),
        (SettlementStepV1::Payout, 2, "Payout"),
        (SettlementStepV1::Completed, 3, "Completed"),
    ] {
        assert_wire(step, &format!("{tag:02x}"));
        assert_eq!(
            variant("events", "SettlementStepV1", name),
            json!({"tag":tag,"name":name,"fields":[]})
        );
    }
    for (route, tag, name) in [
        (SettlementRouteV1::Manual, 0, "Manual"),
        (SettlementRouteV1::Graduation, 1, "Graduation"),
    ] {
        assert_wire(route, &format!("{tag:02x}"));
        assert_eq!(
            variant("events", "SettlementRouteV1", name),
            json!({"tag":tag,"name":name,"fields":[]})
        );
    }
    for (purpose, tag, name) in [
        (PayoutPurposeV1::Reserve, 0, "Reserve"),
        (PayoutPurposeV1::CreatorProceeds, 1, "CreatorProceeds"),
        (PayoutPurposeV1::CreatorLaunch, 2, "CreatorLaunch"),
        (PayoutPurposeV1::AdapterLaunch, 3, "AdapterLaunch"),
        (PayoutPurposeV1::AdapterCollateral, 4, "AdapterCollateral"),
    ] {
        assert_wire(purpose, &format!("{tag:02x}"));
        assert_eq!(
            variant("events", "PayoutPurposeV1", name),
            json!({"tag":tag,"name":name,"fields":[]})
        );
    }
    assert_wire(
        SettlementEffectV1::Burn {
            definition: [3; 32],
            amount: 5,
        },
        &format!("00{}05000000000000000000000000000000", "03".repeat(32)),
    );
    let effect = SettlementEffectV1::Payout {
        definition: [3; 32],
        recipient: [4; 32],
        purpose: PayoutPurposeV1::CreatorLaunch,
        liability_discharged: 5,
        tokens_transferred: 0,
    };
    let encoded = format!(
        "01{}{}020500000000000000000000000000000000000000000000000000000000000000",
        "03".repeat(32),
        "04".repeat(32)
    );
    assert_wire(effect, &encoded);
    assert_wire(
        SaleEventKindV1::Settlement {
            step: SettlementStepV1::Payout,
            route: Some(SettlementRouteV1::Graduation),
            effects: vec![effect],
        },
        &format!("0802010101000000{encoded}"),
    );
    assert_wire(TradeFormV1::ExactInput, "00");
    assert_wire(TradeFormV1::ExactOutput, "01");
    assert_wire(AllocationReleaseV1::AfterSetup, "00");
    assert_wire(AllocationReleaseV1::AfterEnd, "01");
}

#[test]
fn legacy_weighted_event_payloads_remain_in_the_events_schema() {
    use curve_core::events::{WeightedCheckpoint, WeightedPauseChanged};
    assert_eq!(WeightedCheckpoint::SELECTOR_NAME, "lbp::WeightedCheckpoint");
    assert_eq!(
        WeightedPauseChanged::SELECTOR_NAME,
        "lbp::WeightedPauseChanged"
    );
    assert_wire(
        WeightedCheckpoint {
            time: 9,
            token_weight: 5,
        },
        "090000000000000005000000000000000000000000000000",
    );
    assert_wire(WeightedPauseChanged { paused: true }, "01");
    assert_eq!(
        schema()["modules"]["events"]["WeightedCheckpoint"]["fields"],
        json!([
            {"name":"time","type":"u64"},{"name":"token_weight","type":"u128"}
        ])
    );
    assert_eq!(
        schema()["modules"]["events"]["WeightedPauseChanged"]["fields"],
        json!([{"name":"paused","type":"bool"}])
    );
}

#[test]
fn generated_event_metadata_binds_selectors_to_concrete_payload_types() {
    assert_eq!(
        schema()["events"],
        json!({
            "distribution_adapter::DistributionEvent": {"name":"distribution_adapter::DistributionV1","selector":"4ea9de7d9d1e2e6d","payload":"distribution_adapter::DistributionEvent"},
            "events::SaleEventV1": {"name":"launchpad::SaleEventV1","selector":"e1961d35ac82206c","payload":"events::SaleEventV1"},
            "events::WeightedCheckpoint": {"name":"lbp::WeightedCheckpoint","selector":"ae2f6d457fb379cf","payload":"events::WeightedCheckpoint"},
            "events::WeightedPauseChanged": {"name":"lbp::WeightedPauseChanged","selector":"6d8c1803e82b3a9d","payload":"events::WeightedPauseChanged"}
        })
    );
}

#[test]
fn shared_trade_modes_have_independent_wire_tags() {
    for (mode, tag, name) in [
        (factory_core::TradeMode::BuyOnly, 0, "BuyOnly"),
        (factory_core::TradeMode::BuyAndSell, 1, "BuyAndSell"),
    ] {
        assert_wire(mode, &format!("{tag:02x}"));
        assert_eq!(
            variant("terms", "TradeMode", name),
            json!({"tag":tag,"name":name,"fields":[]})
        );
    }
}

#[test]
fn lbp_sells_append_tags_without_membership_or_weighted_caller_time() {
    for (message, name, tag, timed) in [
        (
            lbp_core::Message::SellExactInput {
                amount_in: 25,
                min_amount_out: 10,
                now: 9,
            },
            "SellExactInput",
            18u8,
            true,
        ),
        (
            lbp_core::Message::SellWeightedExactInput {
                amount_in: 25,
                min_amount_out: 10,
            },
            "SellWeightedExactInput",
            19u8,
            false,
        ),
    ] {
        let payload = format!(
            "190000000000000000000000000000000a000000000000000000000000000000{}",
            if timed { "0900000000000000" } else { "" }
        );
        let mut fields = vec![
            json!({"name":"amount_in","type":"u128"}),
            json!({"name":"min_amount_out","type":"u128"}),
        ];
        if timed {
            fields.push(json!({"name":"now","type":"u64"}));
        }
        assert_eq!(
            variant("lbp", "Message", name),
            json!({"tag":tag,"name":name,"fields":fields})
        );
        assert_wire(
            lbp_core::Message::Execute {
                pool: AccountId::new([3; 32]),
                instruction: Box::new(message),
            },
            &format!("06{}{tag:02x}{payload}", "03".repeat(32)),
        );
    }
}

#[test]
fn retained_sell_fee_and_initial_invariant_have_explicit_schema_slots() {
    let schema = schema();
    let fields = schema["modules"]["factory"]["FactoryState"]["fields"]
        .as_array()
        .unwrap();
    let index = fields
        .iter()
        .position(|field| field["name"] == "trade_mode")
        .unwrap();
    assert_eq!(
        fields[index + 1],
        json!({"name":"sell_fee_bps","type":"u16"})
    );
    let fields = schema["modules"]["pool"]["Pool"]["fields"]
        .as_array()
        .unwrap();
    let index = fields
        .iter()
        .position(|field| field["name"] == "k")
        .unwrap();
    assert_eq!(fields[index - 1], json!({"name":"initial_k","type":"u128"}));
    let trade = variant("events", "SaleEventKindV1", "Trade");
    let fields = trade["fields"].as_array().unwrap();
    let index = fields
        .iter()
        .position(|field| field["name"] == "fee_bps")
        .unwrap();
    assert_eq!(
        fields[index + 1],
        json!({"name":"retained_fee","type":"u128"})
    );
    assert_eq!(
        fields[index + 2],
        json!({"name":"sell_fee_bps","type":"u16"})
    );
    assert_eq!(fields[index + 3]["name"], "pricing_time");
}

#[test]
fn shared_graduation_policy_and_payouts_have_explicit_wire_fields() {
    assert_wire(
        launchpad_client::GraduationPolicy {
            launch_bps: 2500,
            collateral_bps: 7500,
        },
        "c4094c1d",
    );
    assert_eq!(
        schema()["modules"]["settlement"]["GraduationPolicy"]["fields"],
        json!([
            {"name":"launch_bps","type":"u16"}, {"name":"collateral_bps","type":"u16"}
        ])
    );
    let payouts = launchpad_client::SettlementPayouts {
        creator: factory_core::SettlementAmounts {
            launch_tokens: 225,
            collateral: 28,
        },
        adapter: factory_core::SettlementAmounts {
            launch_tokens: 75,
            collateral: 84,
        },
    };
    let expected = [225u128, 28, 75, 84]
        .into_iter()
        .flat_map(u128::to_le_bytes)
        .collect::<Vec<_>>();
    assert_wire(payouts, &hex::encode(expected));
    assert_eq!(
        schema()["modules"]["settlement"]["SettlementPayouts"]["fields"],
        json!([
            {"name":"creator","type":"SettlementAmounts"}, {"name":"adapter","type":"SettlementAmounts"}
        ])
    );
    assert_eq!(
        schema()["modules"]["lbp_settlement"]["SettlementReceipt"]["fields"],
        json!([
            {"name":"factory","type":"Actor"}, {"name":"pool","type":"Actor"},
            {"name":"route","type":"SettlementRoute"}, {"name":"creator","type":"AccountId"},
            {"name":"adapter","type":"Option<Actor>"}, {"name":"eligible","type":"SettlementAmounts"},
            {"name":"payouts","type":"SettlementPayouts"}, {"name":"progress","type":"SettlementProgress"}
        ])
    );
}
