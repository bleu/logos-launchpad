use lee_core::account::AccountId;
use serde_json::json;

use super::{assert_wire, schema, variant};

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
        per_block_token_limit: None,
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
    let mut expected = vec![8, 0, 0, 1, 1, 0, 2, 1, 0, 0, 0, 0, 0];
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
fn fixed_pool_state_tuple_places_policies_after_trade_control() {
    let account = AccountId::new([5; 32]);
    let mut bytes = borsh::to_vec(&(
        (Some(17u128), Some(8u64), 5u128),
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
    assert_eq!(pool.block_allocation.limit, Some(17));
    assert_eq!(pool.block_allocation.observed_block, Some(8));
    assert_eq!(pool.block_allocation.tokens_sold, 5);
    assert_eq!(pool.ending_policy, lbp_core::EndingPolicy::Either);
    assert_eq!(pool.remaining_inventory, 800);
    assert_eq!(pool.graduation_adapter, None);
    assert_eq!(
        pool.settlement_progress,
        lbp_core::SettlementProgress::Unselected
    );
    assert_eq!(&bytes[42..45], &[0, 0, 3]);
    assert_eq!(borsh::to_vec(&pool).unwrap(), bytes);
}

#[test]
fn fixed_pool_terminal_states_use_progress_tags_instead_of_boolean_completion() {
    let account = AccountId::new([5; 32]);
    for (progress, tag) in [
        (lbp_core::SettlementProgress::ManualComplete, 2),
        (lbp_core::SettlementProgress::Graduated, 3),
    ] {
        let mut bytes = borsh::to_vec(&(
            (None::<u128>, None::<u64>, 0u128),
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
fn weighted_pool_binds_the_split_and_keeps_seed_in_recorded_collateral() {
    let account = AccountId::new([5; 32]);
    let adapter = lee_core::account::Actor::new(AccountId::new([6; 32]), AccountId::new([7; 32]));
    let mut bytes = borsh::to_vec(&(
        (None::<u128>, None::<u64>, 0u128),
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
    assert_eq!(&bytes[18..25], &[0, 0, 1, 0, 0, 0, 1]);
    assert_eq!(pool.graduation_adapter, Some(adapter));
    assert_eq!(pool.recorded_collateral, 140);
    assert_eq!(pool.graduation_policy.collateral_bps, 7500);
    assert_eq!(borsh::to_vec(&pool).unwrap(), bytes);
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
