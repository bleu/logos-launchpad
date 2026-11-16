use borsh::{BorshDeserialize, BorshSerialize};
use sale_policy::events::{
    EventSubjectV1, EventTimeBoundsV1, PricingEngineV1, SaleEventKindV1, SaleEventV1,
    WeightedCheckpoint, WeightedPauseChanged,
};

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
