use std::process::Command;

#[test]
fn application_state_schema_includes_private_pending_types_and_parent_imports() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let output = Command::new(env!("CARGO_BIN_EXE_lez-compat-tools"))
        .current_dir(root)
        .args([
            "idl-source",
            "factory=crates/factory-core/src/lib.rs",
            "fixed_price=crates/factory-core/src/fixed_price.rs",
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
        idl["imports"]["fixed_price"],
        serde_json::json!(["factory"])
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
