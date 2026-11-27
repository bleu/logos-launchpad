use lee_core::account::AccountId;
use serde_json::json;

use super::{assert_wire, schema, variant};

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
