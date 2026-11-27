use serde_json::json;

use super::{assert_wire, schema, variant};

#[test]
fn sale_event_envelope_has_a_versioned_selector_and_independent_wire_bytes() {
    use curve_core::events::{EventSubjectV1, PricingEngineV1, SaleEventKindV1, SaleEventV1};
    let event = SaleEventV1::new(
        [2; 32],
        PricingEngineV1::Weighted,
        EventSubjectV1::Launch {
            factory: [3; 32],
            pool: [4; 32],
        },
        SaleEventKindV1::PauseChanged { paused: true },
    );
    let encoded = format!(
        "01{}0200{}{}0401",
        "02".repeat(32),
        "03".repeat(32),
        "04".repeat(32)
    );
    assert_wire(event.clone(), &encoded);
    assert_eq!(SaleEventV1::VERSION, 1);
    assert_eq!(SaleEventV1::SELECTOR_NAME, "launchpad::SaleEventV1");
    let runtime = curve_core::sale_event(event);
    assert_eq!(
        runtime.selector,
        [0xe1, 0x96, 0x1d, 0x35, 0xac, 0x82, 0x20, 0x6c]
    );
    assert_eq!(runtime.data, hex::decode(encoded).unwrap());
    assert_eq!(
        schema()["modules"]["events"]["SaleEventV1"]["fields"],
        json!([
            {"name":"version","type":"u8"}, {"name":"namespace","type":"[u8;32]"},
            {"name":"engine","type":"PricingEngineV1"}, {"name":"subject","type":"EventSubjectV1"},
            {"name":"kind","type":"SaleEventKindV1"}
        ])
    );
    assert!(
        schema()["imports"]["sale_policy"]
            .as_array()
            .unwrap()
            .contains(&json!("events"))
    );
}

#[test]
fn event_engines_subjects_and_policy_fields_have_fixed_wire_order() {
    use curve_core::events::{EventPoliciesV1, EventSubjectV1, PricingEngineV1};
    for (engine, tag, name) in [
        (PricingEngineV1::Curve, 0, "Curve"),
        (PricingEngineV1::FixedPrice, 1, "FixedPrice"),
        (PricingEngineV1::Weighted, 2, "Weighted"),
    ] {
        assert_wire(engine, &format!("{tag:02x}"));
        assert_eq!(
            variant("events", "PricingEngineV1", name),
            json!({"tag":tag,"name":name,"fields":[]})
        );
    }
    assert_wire(
        EventSubjectV1::Launch {
            factory: [3; 32],
            pool: [4; 32],
        },
        &format!("00{}{}", "03".repeat(32), "04".repeat(32)),
    );
    assert_wire(
        EventSubjectV1::Pool { pool: [4; 32] },
        &format!("01{}", "04".repeat(32)),
    );
    assert_eq!(
        variant("events", "EventSubjectV1", "Launch"),
        json!({"tag":0,"name":"Launch","fields":[
            {"name":"factory","type":"[u8;32]"},{"name":"pool","type":"[u8;32]"}
        ]})
    );
    assert_eq!(
        variant("events", "EventSubjectV1", "Pool"),
        json!({"tag":1,"name":"Pool","fields":[{"name":"pool","type":"[u8;32]"}]})
    );
    assert_wire(
        EventPoliciesV1 {
            ending: curve_core::EndingPolicy::Scheduled,
            pause: curve_core::PausePolicy::AuthorityControlled,
            sell_fee_bps: 0x0304,
        },
        "01010403",
    );
    assert_eq!(
        schema()["modules"]["events"]["EventPoliciesV1"]["fields"],
        json!([
            {"name":"ending","type":"EndingPolicy"},{"name":"pause","type":"PausePolicy"},
            {"name":"sell_fee_bps","type":"u16"}
        ])
    );
}

#[test]
fn event_kinds_keep_creation_trade_and_settlement_amounts_distinct_on_wire() {
    use curve_core::events::*;
    let policies = EventPoliciesV1 {
        ending: curve_core::EndingPolicy::Scheduled,
        pause: curve_core::PausePolicy::AuthorityControlled,
        sell_fee_bps: 0x0304,
    };
    let a = "01".repeat(32);
    let b = "02".repeat(32);
    let c = "03".repeat(32);
    let d = "04".repeat(32);
    let n = |value: u128| hex::encode(value.to_le_bytes());
    for (kind, tag, name, payload) in [
        (
            SaleEventKindV1::Created {
                creator: [1; 32],
                creator_role: [2; 32],
                token_definition: [3; 32],
                collateral_definition: [4; 32],
                graduation_reserve: 0,
                sale_inventory: 5,
                creator_allocation: 6,
                graduation_policy: launchpad_client::GraduationPolicy {
                    launch_bps: 2500,
                    collateral_bps: 7500,
                },
                collateral_seed: 8,
                start_timestamp: None,
                end_timestamp: Some(9),
                policies,
            },
            0,
            "Created",
            format!(
                "{a}{b}{c}{d}{}{}{}{}{}0001090000000000000001010403",
                n(5),
                n(0),
                n(6),
                "c4094c1d",
                n(8)
            ),
        ),
        (
            SaleEventKindV1::Prepared {
                owner: [1; 32],
                token0_definition: [2; 32],
                token1_definition: [3; 32],
                reserve0: 5,
                reserve1: 6,
                policies,
            },
            1,
            "Prepared",
            format!("{a}{b}{c}{}{}01010403", n(5), n(6)),
        ),
        (
            SaleEventKindV1::Activated {
                reserve0: 5,
                reserve1: 6,
                collateral_seed: 7,
            },
            2,
            "Activated",
            format!("{}{}{}", n(5), n(6), n(7)),
        ),
        (
            SaleEventKindV1::Trade {
                trader: [1; 32],
                form: TradeFormV1::ExactOutput,
                token_in: [2; 32],
                token_out: [3; 32],
                amount_in: 5,
                pricing_input: 6,
                gross_output: 7,
                amount_out: 8,
                fee_definition: [4; 32],
                fee_amount: 9,
                fee_bps: 0x0102,
                retained_fee: 11,
                sell_fee_bps: 0x0304,
                pricing_time: Some(10),
            },
            3,
            "Trade",
            format!(
                "{a}01{b}{c}{}{}{}{}{d}{}0201{}0403010a00000000000000",
                n(5),
                n(6),
                n(7),
                n(8),
                n(9),
                n(11)
            ),
        ),
        (
            SaleEventKindV1::PauseChanged { paused: true },
            4,
            "PauseChanged",
            "01".into(),
        ),
        (
            SaleEventKindV1::FeesCollected {
                definition: [1; 32],
                treasury: [2; 32],
                liability_discharged: 5,
                tokens_transferred: 0,
            },
            5,
            "FeesCollected",
            format!("{a}{b}{}{}", n(5), n(0)),
        ),
        (
            SaleEventKindV1::AllocationReleased {
                definition: [1; 32],
                recipient: [2; 32],
                amount: 5,
                release: AllocationReleaseV1::AfterEnd,
            },
            6,
            "AllocationReleased",
            format!("{a}{b}{}01", n(5)),
        ),
        (
            SaleEventKindV1::Closed {
                reason: ClosureReasonV1::ScheduledEnd { timestamp: 9 },
                inclusion_bounds: EventTimeBoundsV1 {
                    at_least: Some(9),
                    before: None,
                },
            },
            7,
            "Closed",
            "02090000000000000001090000000000000000".into(),
        ),
        (
            SaleEventKindV1::Settlement {
                step: SettlementStepV1::Completed,
                route: Some(SettlementRouteV1::Manual),
                effects: vec![],
            },
            8,
            "Settlement",
            "03010000000000".into(),
        ),
        (
            SaleEventKindV1::Graduated {
                adapter_program: [1; 32],
                adapter_custody: [2; 32],
                launch_tokens: 5,
                collateral: 6,
            },
            9,
            "Graduated",
            format!("{a}{b}{}{}", n(5), n(6)),
        ),
    ] {
        assert_wire(kind, &format!("{tag:02x}{payload}"));
        assert_eq!(variant("events", "SaleEventKindV1", name)["tag"], tag);
    }
}

#[test]
fn event_closure_routes_steps_and_effects_keep_independent_tags() {
    use curve_core::events::*;
    for (reason, encoded) in [
        (ClosureReasonV1::Manual, "00".into()),
        (
            ClosureReasonV1::Depleted {
                definition: [3; 32],
            },
            format!("01{}", "03".repeat(32)),
        ),
        (
            ClosureReasonV1::ScheduledEnd { timestamp: 9 },
            "020900000000000000".into(),
        ),
    ] {
        assert_wire(reason, &encoded);
    }
    for (step, tag, name) in [
        (SettlementStepV1::Prepared, 0, "Prepared"),
        (SettlementStepV1::ReservesWithdrawn, 1, "ReservesWithdrawn"),
        (SettlementStepV1::Payout, 2, "Payout"),
        (SettlementStepV1::Completed, 3, "Completed"),
    ] {
        assert_wire(step, &format!("{tag:02x}"));
        assert_eq!(
            variant("events", "SettlementStepV1", name),
            json!({"tag":tag,"name":name,"fields":[]})
        );
    }
    for (route, tag, name) in [
        (SettlementRouteV1::Manual, 0, "Manual"),
        (SettlementRouteV1::Graduation, 1, "Graduation"),
    ] {
        assert_wire(route, &format!("{tag:02x}"));
        assert_eq!(
            variant("events", "SettlementRouteV1", name),
            json!({"tag":tag,"name":name,"fields":[]})
        );
    }
    for (purpose, tag, name) in [
        (PayoutPurposeV1::Reserve, 0, "Reserve"),
        (PayoutPurposeV1::CreatorProceeds, 1, "CreatorProceeds"),
        (PayoutPurposeV1::CreatorLaunch, 2, "CreatorLaunch"),
        (PayoutPurposeV1::AdapterLaunch, 3, "AdapterLaunch"),
        (PayoutPurposeV1::AdapterCollateral, 4, "AdapterCollateral"),
    ] {
        assert_wire(purpose, &format!("{tag:02x}"));
        assert_eq!(
            variant("events", "PayoutPurposeV1", name),
            json!({"tag":tag,"name":name,"fields":[]})
        );
    }
    assert_wire(
        SettlementEffectV1::Burn {
            definition: [3; 32],
            amount: 5,
        },
        &format!("00{}05000000000000000000000000000000", "03".repeat(32)),
    );
    let effect = SettlementEffectV1::Payout {
        definition: [3; 32],
        recipient: [4; 32],
        purpose: PayoutPurposeV1::CreatorLaunch,
        liability_discharged: 5,
        tokens_transferred: 0,
    };
    let encoded = format!(
        "01{}{}020500000000000000000000000000000000000000000000000000000000000000",
        "03".repeat(32),
        "04".repeat(32)
    );
    assert_wire(effect, &encoded);
    assert_wire(
        SaleEventKindV1::Settlement {
            step: SettlementStepV1::Payout,
            route: Some(SettlementRouteV1::Graduation),
            effects: vec![effect],
        },
        &format!("0802010101000000{encoded}"),
    );
    assert_wire(TradeFormV1::ExactInput, "00");
    assert_wire(TradeFormV1::ExactOutput, "01");
    assert_wire(AllocationReleaseV1::AfterSetup, "00");
    assert_wire(AllocationReleaseV1::AfterEnd, "01");
}

#[test]
fn legacy_weighted_event_payloads_remain_in_the_events_schema() {
    use curve_core::events::{WeightedCheckpoint, WeightedPauseChanged};
    assert_eq!(WeightedCheckpoint::SELECTOR_NAME, "lbp::WeightedCheckpoint");
    assert_eq!(
        WeightedPauseChanged::SELECTOR_NAME,
        "lbp::WeightedPauseChanged"
    );
    assert_wire(
        WeightedCheckpoint {
            time: 9,
            token_weight: 5,
        },
        "090000000000000005000000000000000000000000000000",
    );
    assert_wire(WeightedPauseChanged { paused: true }, "01");
    assert_eq!(
        schema()["modules"]["events"]["WeightedCheckpoint"]["fields"],
        json!([
            {"name":"time","type":"u64"},{"name":"token_weight","type":"u128"}
        ])
    );
    assert_eq!(
        schema()["modules"]["events"]["WeightedPauseChanged"]["fields"],
        json!([{"name":"paused","type":"bool"}])
    );
}

#[test]
fn generated_event_metadata_binds_selectors_to_concrete_payload_types() {
    assert_eq!(
        schema()["events"],
        json!({
            "distribution_adapter::DistributionEvent": {"name":"distribution_adapter::DistributionV1","selector":"4ea9de7d9d1e2e6d","payload":"distribution_adapter::DistributionEvent"},
            "events::SaleEventV1": {"name":"launchpad::SaleEventV1","selector":"e1961d35ac82206c","payload":"events::SaleEventV1"},
            "events::WeightedCheckpoint": {"name":"lbp::WeightedCheckpoint","selector":"ae2f6d457fb379cf","payload":"events::WeightedCheckpoint"},
            "events::WeightedPauseChanged": {"name":"lbp::WeightedPauseChanged","selector":"6d8c1803e82b3a9d","payload":"events::WeightedPauseChanged"}
        })
    );
}
