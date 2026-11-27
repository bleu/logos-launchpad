use lee_core::account::AccountId;
use serde_json::{Value, json};

use super::{assert_wire, schema, variant};

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
    let schema: Value = serde_json::from_str(include_str!(
        "../../../../packages/lez-compat/idl/compat.json"
    ))
    .unwrap();
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
    let schema: Value = serde_json::from_str(include_str!(
        "../../../../packages/lez-compat/idl/compat.json"
    ))
    .unwrap();
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
