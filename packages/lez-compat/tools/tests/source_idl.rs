use std::process::Command;

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
