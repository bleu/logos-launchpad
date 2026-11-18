use borsh::{BorshDeserialize, BorshSerialize};
use sale_policy::events::{
    EventPoliciesV1, EventSubjectV1, EventTimeBoundsV1, PricingEngineV1, SaleEventKindV1,
    SaleEventV1, TradeFormV1, WeightedCheckpoint, WeightedPauseChanged,
};
use sale_policy::{EndingPolicy, GraduationPolicy, PausePolicy};

fn policies() -> EventPoliciesV1 {
    EventPoliciesV1 {
        ending: EndingPolicy::Depletion,
        pause: PausePolicy::AuthorityControlled,
        sell_fee_bps: 200,
    }
}

fn roundtrip<T>(value: &T)
where
    T: BorshSerialize + BorshDeserialize + std::fmt::Debug + PartialEq,
{
    let bytes = borsh::to_vec(value).unwrap();
    let decoded: T = borsh::from_slice(&bytes).unwrap();
    assert_eq!(&decoded, value);
    let mut trailing = bytes;
    trailing.push(0);
    assert!(borsh::from_slice::<T>(&trailing).is_err());
}

#[test]
fn constructor_fixes_version_and_preserves_identity() {
    let subject = EventSubjectV1::Launch {
        factory: [2; 32],
        pool: [3; 32],
    };
    let event = SaleEventV1::new(
        [1; 32],
        PricingEngineV1::FixedPrice,
        subject,
        SaleEventKindV1::PauseChanged { paused: true },
    );
    assert_eq!(event.version, 1);
    assert_eq!(event.version, SaleEventV1::VERSION);
    assert_eq!(event.namespace, [1; 32]);
    assert_eq!(event.engine, PricingEngineV1::FixedPrice);
    assert_eq!(event.subject, subject);
    assert_eq!(SaleEventV1::SELECTOR_NAME, "launchpad::SaleEventV1");
    roundtrip(&event);
}

#[test]
fn envelope_has_a_fixed_borsh_field_order() {
    let event = SaleEventV1::new(
        [1; 32],
        PricingEngineV1::Weighted,
        EventSubjectV1::Pool { pool: [3; 32] },
        SaleEventKindV1::PauseChanged { paused: true },
    );
    let mut expected = vec![1];
    expected.extend_from_slice(&[1; 32]);
    expected.extend_from_slice(&[2, 1]);
    expected.extend_from_slice(&[3; 32]);
    expected.extend_from_slice(&[4, 1]);
    assert_eq!(borsh::to_vec(&event).unwrap(), expected);
}

#[test]
fn retained_fee_fields_have_distinct_wire_slots() {
    assert_eq!(
        borsh::to_vec(&policies()).unwrap(),
        borsh::to_vec(&(2u8, 1u8, 200u16)).unwrap()
    );
    let trade = SaleEventKindV1::Trade {
        trader: [1; 32],
        form: TradeFormV1::ExactInput,
        token_in: [2; 32],
        token_out: [3; 32],
        amount_in: 17,
        pricing_input: 17,
        gross_output: 101,
        amount_out: 96,
        fee_definition: [3; 32],
        fee_amount: 2,
        fee_bps: 100,
        retained_fee: 3,
        sell_fee_bps: 200,
        pricing_time: Some(400),
    };
    let mut expected = vec![3];
    expected.extend(borsh::to_vec(&([1u8; 32], 0u8, [2u8; 32], [3u8; 32])).unwrap());
    expected.extend(borsh::to_vec(&(17u128, 17u128, 101u128, 96u128)).unwrap());
    expected
        .extend(borsh::to_vec(&([3u8; 32], 2u128, 100u16, 3u128, 200u16, Some(400u64))).unwrap());
    assert_eq!(borsh::to_vec(&trade).unwrap(), expected);
    roundtrip(&trade);
}

#[test]
fn time_bounds_encode_seconds_and_optional_end() {
    let bounds = EventTimeBoundsV1 {
        at_least: Some(200),
        before: None,
    };
    let mut expected = vec![1];
    expected.extend_from_slice(&200_u64.to_le_bytes());
    expected.push(0);
    assert_eq!(borsh::to_vec(&bounds).unwrap(), expected);
    roundtrip(&bounds);
    roundtrip(&EventTimeBoundsV1 {
        at_least: Some(100),
        before: Some(200),
    });
}

#[test]
fn legacy_pause_payload_is_exactly_a_borsh_bool() {
    assert_eq!(
        WeightedPauseChanged::SELECTOR_NAME,
        "lbp::WeightedPauseChanged"
    );
    for paused in [false, true] {
        let payload = WeightedPauseChanged { paused };
        assert_eq!(
            borsh::to_vec(&payload).unwrap(),
            borsh::to_vec(&paused).unwrap()
        );
        assert_eq!(borsh::to_vec(&payload).unwrap(), vec![u8::from(paused)]);
        roundtrip(&payload);
    }
}

#[test]
fn legacy_checkpoint_payload_is_exactly_the_existing_tuple() {
    assert_eq!(WeightedCheckpoint::SELECTOR_NAME, "lbp::WeightedCheckpoint");
    for (time, token_weight) in [
        (0, 0),
        (100, 1_000_000_000_000_000_000),
        (u64::MAX, u128::MAX),
    ] {
        let payload = WeightedCheckpoint { time, token_weight };
        let bytes = borsh::to_vec(&payload).unwrap();
        assert_eq!(bytes, borsh::to_vec(&(time, token_weight)).unwrap());
        let mut expected = Vec::new();
        expected.extend_from_slice(&time.to_le_bytes());
        expected.extend_from_slice(&token_weight.to_le_bytes());
        assert_eq!(bytes, expected);
        assert_eq!(bytes.len(), 24);
        roundtrip(&payload);
    }
}

#[test]
fn created_reports_both_saved_graduation_shares() {
    let event = SaleEventKindV1::Created {
        creator: [1; 32],
        creator_role: [2; 32],
        token_definition: [3; 32],
        collateral_definition: [4; 32],
        graduation_reserve: 0,
        sale_inventory: 100,
        creator_allocation: 20,
        graduation_policy: GraduationPolicy {
            launch_bps: 2_500,
            collateral_bps: 7_500,
        },
        collateral_seed: 50,
        start_timestamp: Some(100),
        end_timestamp: Some(200),
        policies: policies(),
    };
    let mut expected = vec![0];
    expected.extend(borsh::to_vec(&([1u8; 32], [2u8; 32], [3u8; 32], [4u8; 32])).unwrap());
    expected.extend(borsh::to_vec(&(100u128, 0u128, 20u128, 2_500u16, 7_500u16, 50u128)).unwrap());
    expected.extend(borsh::to_vec(&(Some(100u64), Some(200u64), policies())).unwrap());
    assert_eq!(borsh::to_vec(&event).unwrap(), expected);
    roundtrip(&event);
}

#[test]
fn settlement_reports_creator_and_adapter_payouts_for_both_assets() {
    use sale_policy::events::{
        PayoutPurposeV1, SettlementEffectV1, SettlementRouteV1, SettlementStepV1,
    };
    let effects = [
        ([3; 32], [1; 32], PayoutPurposeV1::CreatorLaunch, 75),
        ([4; 32], [1; 32], PayoutPurposeV1::CreatorProceeds, 25),
        ([3; 32], [2; 32], PayoutPurposeV1::AdapterLaunch, 25),
        ([4; 32], [2; 32], PayoutPurposeV1::AdapterCollateral, 75),
    ]
    .into_iter()
    .map(
        |(definition, recipient, purpose, amount)| SettlementEffectV1::Payout {
            definition,
            recipient,
            purpose,
            liability_discharged: amount,
            tokens_transferred: amount,
        },
    )
    .collect();
    roundtrip(&SaleEventKindV1::Settlement {
        step: SettlementStepV1::Completed,
        route: Some(SettlementRouteV1::Graduation),
        effects,
    });
}

#[test]
fn settlement_steps_and_payout_purposes_have_explicit_wire_tags() {
    use sale_policy::events::{PayoutPurposeV1, SettlementStepV1};
    for (tag, step) in [
        SettlementStepV1::Prepared,
        SettlementStepV1::ReservesWithdrawn,
        SettlementStepV1::Payout,
        SettlementStepV1::Completed,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(borsh::to_vec(&step).unwrap(), vec![tag as u8]);
        roundtrip(&step);
    }
    for (tag, purpose) in [
        PayoutPurposeV1::Reserve,
        PayoutPurposeV1::CreatorProceeds,
        PayoutPurposeV1::CreatorLaunch,
        PayoutPurposeV1::AdapterLaunch,
        PayoutPurposeV1::AdapterCollateral,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(borsh::to_vec(&purpose).unwrap(), vec![tag as u8]);
        roundtrip(&purpose);
    }
}
