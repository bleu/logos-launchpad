use serde_json::{Value, json};

#[path = "idl_wire/access_control.rs"]
mod access_control;
#[path = "idl_wire/curve.rs"]
mod curve;
#[path = "idl_wire/events.rs"]
mod events;
#[path = "idl_wire/lbp.rs"]
mod lbp;
#[path = "idl_wire/sale_policy.rs"]
mod sale_policy;
#[path = "idl_wire/settlement.rs"]
mod settlement;

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
