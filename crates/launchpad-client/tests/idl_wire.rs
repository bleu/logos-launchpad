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
