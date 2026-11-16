use std::process::Command;

#[test]
fn application_state_schema_includes_private_pending_types_and_parent_imports() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let output = Command::new(env!("CARGO_BIN_EXE_lez-compat-tools"))
        .current_dir(root)
        .args([
            "idl-source",
            "factory=crates/factory-core/src/lib.rs",
            "lbp_setup=crates/factory-core/src/lbp_setup.rs",
            "lbp=crates/lbp-core/src/lib.rs",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let idl: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let fields = |module: &str, name: &str| idl["modules"][module][name]["fields"].clone();
    assert_eq!(
        fields("factory", "Pending"),
        serde_json::json!([
            {"name": "command", "type": "Message"},
            {"name": "proof", "type": "bool"},
            {"name": "reads", "type": "Vec<Read>"},
            {"name": "values", "type": "Vec<Vec<u8>>"}
        ])
    );
    assert_eq!(
        fields("factory", "Read"),
        serde_json::json!([
            {"name": "actor", "type": "Actor"},
            {"name": "message", "type": "Vec<u8>"}
        ])
    );
    assert_eq!(
        fields("lbp", "Pending"),
        serde_json::json!([
            {"name": "activate", "type": "bool"},
            {"name": "step", "type": "u8"}
        ])
    );
    assert_eq!(
        idl["imports"]["lbp_setup"],
        serde_json::json!(["factory", "lbp"])
    );
}

#[test]
fn source_idl_describes_the_public_message_fields_and_borsh_tags() {
    let source = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/messages.rs");
    let output = Command::new(env!("CARGO_BIN_EXE_lez-compat-tools"))
        .args(["idl-source", &format!("sale={source}")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let idl: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(idl["encoding"], "borsh");
    assert_eq!(
        idl["modules"]["sale"]["Message"]["variants"][1],
        serde_json::json!({
            "tag": 1,
            "name": "Start",
            "fields": [{"name": "start_timestamp", "type": "u64"}]
        })
    );
}

#[test]
fn shared_policy_source_resolves_imports_for_each_sale_engine() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let temporary = std::env::temp_dir().join(format!("policy-idl-{}", std::process::id()));
    std::fs::create_dir_all(&temporary).unwrap();
    let mut arguments = vec![
        "idl-source".to_string(),
        format!(
            "eligibility_policy={}",
            root.join("crates/eligibility-policy/src/lib.rs").display()
        ),
    ];
    for name in ["curve", "factory", "lbp"] {
        let path = temporary.join(format!("{name}.rs"));
        std::fs::write(&path, "pub use eligibility_policy::{BuyPolicy, MembershipPath};\n#[derive(BorshSerialize)]\npub struct Terms { pub policy: BuyPolicy, pub membership: Option<MembershipPath> }").unwrap();
        arguments.push(format!("{name}={}", path.display()));
    }
    let output = Command::new(env!("CARGO_BIN_EXE_lez-compat-tools"))
        .args(arguments)
        .output()
        .unwrap();
    std::fs::remove_dir_all(temporary).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let idl: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for name in ["curve", "factory", "lbp"] {
        assert_eq!(
            idl["imports"][name],
            serde_json::json!(["eligibility_policy"])
        );
    }
    assert_eq!(
        idl["modules"]["eligibility_policy"]["MembershipPath"]["fields"],
        serde_json::json!([
            {"name":"index", "type":"u32"},
            {"name":"salt", "type":"[u8;32]"},
            {"name":"siblings", "type":"[[u8;32];17]"}
        ])
    );
}

#[test]
fn qualified_crate_types_use_registered_module_names() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let temporary = std::env::temp_dir().join(format!("qualified-idl-{}", std::process::id()));
    std::fs::create_dir_all(&temporary).unwrap();
    let source = temporary.join("factory.rs");
    std::fs::write(&source, "#[derive(BorshSerialize)]\npub struct Terms { pub policy: curve_core::BuyPolicy, pub paths: Option<Vec<eligibility_policy::MembershipPath>> }").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lez-compat-tools"))
        .args([
            "idl-source".to_owned(),
            format!("factory={}", source.display()),
            format!(
                "curve={}",
                root.join("crates/curve-core/src/lib.rs").display()
            ),
            format!(
                "eligibility_policy={}",
                root.join("crates/eligibility-policy/src/lib.rs").display()
            ),
        ])
        .output()
        .unwrap();
    std::fs::remove_dir_all(temporary).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let idl: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        idl["modules"]["factory"]["Terms"]["fields"],
        serde_json::json!([
            {"name":"policy","type":"curve::BuyPolicy"},
            {"name":"paths","type":"Option<Vec<eligibility_policy::MembershipPath>>"}
        ])
    );
    assert_eq!(
        idl["imports"]["factory"],
        serde_json::json!(["curve", "eligibility_policy"])
    );
}

#[test]
fn source_idl_does_not_assign_wire_tags_to_unencoded_errors() {
    let source = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/messages.rs");
    let output = Command::new(env!("CARGO_BIN_EXE_lez-compat-tools"))
        .args(["idl-source", &format!("sale={source}")])
        .output()
        .unwrap();
    assert!(output.status.success());
    let idl: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(idl["modules"]["sale"].get("Error").is_none());
}

#[test]
fn application_settlement_schema_links_pool_receipts_and_shared_rules() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let output = Command::new(env!("CARGO_BIN_EXE_lez-compat-tools"))
        .current_dir(root)
        .args([
            "idl-source",
            "eligibility_policy=crates/eligibility-policy/src/lib.rs",
            "sale_policy=crates/sale-policy/src/lib.rs",
            "controls=crates/sale-policy/src/controls.rs",
            "settlement=crates/sale-policy/src/settlement.rs",
            "terms=crates/sale-policy/src/terms.rs",
            "curve=crates/curve-core/src/lib.rs",
            "pool=crates/pool/src/lib.rs",
            "factory=crates/factory-core/src/lib.rs",
            "lbp_setup=crates/factory-core/src/lbp_setup.rs",
            "hook=crates/factory-core/src/hook.rs",
            "lbp=crates/lbp-core/src/lib.rs",
            "lbp_settlement=crates/lbp-core/src/settlement.rs",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let idl: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for (module, dependency) in [
        ("factory", "lbp"),
        ("lbp_setup", "factory"),
        ("lbp_setup", "lbp"),
        ("lbp", "lbp_settlement"),
        ("lbp", "sale_policy"),
        ("sale_policy", "settlement"),
    ] {
        assert!(
            idl["imports"][module]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!(dependency)),
            "missing schema dependency {module} -> {dependency}"
        );
    }
    assert_eq!(idl["imports"]["lbp_settlement"], serde_json::json!(["lbp"]));
    assert!(
        !idl["imports"]["lbp"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("settlement"))
    );
    for name in [
        "SettlementReceipt",
        "SettlementReply",
        "AwaitFactoryFinalize",
    ] {
        assert!(idl["modules"]["lbp_settlement"].get(name).is_some());
        assert!(idl["modules"]["settlement"].get(name).is_none());
    }
    assert!(
        idl["modules"]["settlement"]
            .get("SettlementAmounts")
            .is_some()
    );
    assert!(idl["modules"]["lbp"].get("PoolAccount").is_some());
    for (name, field, field_type) in [
        ("SettlementReceipt", "amounts", "SettlementAmounts"),
        ("SettlementReceipt", "route", "SettlementRoute"),
        ("SettlementReceipt", "progress", "SettlementProgress"),
        ("AwaitFactoryFinalize", "state", "PoolAccount"),
        ("AwaitFactoryFinalize", "receipt", "SettlementReceipt"),
    ] {
        assert!(
            idl["modules"]["lbp_settlement"][name]["fields"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!({"name": field, "type": field_type}))
        );
    }
}

#[test]
fn compatibility_idl_exposes_resolution_without_private_role_state() {
    let output = Command::new(env!("CARGO_BIN_EXE_lez-compat-tools"))
        .arg("idl")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let idl: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let authority = &idl["modules"]["authority"];
    let variants = authority["Message"]["variants"].as_array().unwrap();
    assert_eq!(variants.len(), 14);
    assert_eq!(variants[12]["name"], "BootstrapTransferOnly");
    assert_eq!(variants[12]["tag"], 12);
    assert_eq!(
        variants[13],
        serde_json::json!({"tag": 13, "name": "Resolve", "fields": []})
    );
    assert_eq!(
        authority["CurrentAuthority"]["fields"],
        serde_json::json!([
            {"name": "role", "type": "AccountId"},
            {"name": "authority", "type": "AccountId"}
        ])
    );
    assert!(authority.get("Role").is_none());
}
