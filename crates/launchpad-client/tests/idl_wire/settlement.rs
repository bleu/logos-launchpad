use lee_core::account::AccountId;
use serde_json::json;

use super::{assert_wire, schema, variant};

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
