use lee_core::account::AccountId;
use serde_json::json;

use super::{assert_wire, variant};

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
fn direct_pool_creation_keeps_namespace_owner_and_funding_terms_on_wire() {
    assert_wire(
        curve_core::Message::CreatePool {
            buy_policy: curve_core::BuyPolicy::Open,
            pause_policy: curve_core::PausePolicy::Disabled,
            ending_policy: curve_core::EndingPolicy::Either,
            sell_fee_bps: 0x0102,
            per_block_token_limit: Some(1000),
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
            "01e8030000000000000000000000000000",
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
                {"name":"per_block_token_limit","type":"Option<u128>"},
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
            per_block_token_limit: None,
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
            "000000030102010001",
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
                {"name":"per_block_token_limit","type":"Option<u128>"},
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
