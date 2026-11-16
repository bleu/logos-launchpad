use sale_policy::{EndingDecision, EndingError, EndingPolicy, EndingReason, TimeBounds};

const ENDINGS: [EndingPolicy; 4] = [
    EndingPolicy::Manual,
    EndingPolicy::Scheduled,
    EndingPolicy::Depletion,
    EndingPolicy::Either,
];

#[test]
fn ending_policy_tags_and_round_trips_are_stable() {
    for (tag, policy) in ENDINGS.into_iter().enumerate() {
        let bytes = borsh::to_vec(&policy).unwrap();
        assert_eq!(bytes, vec![u8::try_from(tag).unwrap()]);
        assert_eq!(borsh::from_slice::<EndingPolicy>(&bytes).unwrap(), policy);
    }
    assert!(borsh::from_slice::<EndingPolicy>(&[4]).is_err());
}

#[test]
fn ending_validation_covers_policy_and_window_combinations() {
    for policy in ENDINGS {
        for start in [None, Some(0), Some(10), Some(u64::MAX)] {
            for end in [None, Some(0), Some(1), Some(10), Some(11), Some(u64::MAX)] {
                let requires_end = matches!(policy, EndingPolicy::Scheduled | EndingPolicy::Either);
                let valid = requires_end == end.is_some()
                    && !matches!((start, end), (Some(start), Some(end)) if start >= end);
                assert_eq!(
                    policy.validate(start, end),
                    if valid {
                        Ok(())
                    } else {
                        Err(EndingError::InvalidWindow)
                    }
                );
                if !valid {
                    for remaining in [0, 1] {
                        assert_eq!(
                            policy.trade_bounds(start, end, remaining),
                            Err(EndingError::InvalidWindow)
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn trade_bounds_preserve_inclusive_start_and_exclusive_end() {
    for policy in [EndingPolicy::Scheduled, EndingPolicy::Either] {
        let bounds = policy.trade_bounds(Some(10), Some(20), 1).unwrap();
        assert_eq!(
            bounds,
            TimeBounds {
                at_least: Some(10),
                before: Some(20)
            }
        );
        assert!(!bounds.contains(9));
        assert!(bounds.contains(10));
        assert!(bounds.contains(19));
        assert!(!bounds.contains(20));
        assert!(!bounds.contains(u64::MAX));
    }
    for policy in [EndingPolicy::Manual, EndingPolicy::Depletion] {
        let bounds = policy.trade_bounds(Some(10), None, 1).unwrap();
        assert!(!bounds.contains(9));
        assert!(bounds.contains(10));
        assert!(bounds.contains(u64::MAX));
    }
}

#[test]
fn only_selected_depletion_stops_trades_on_empty_reserves() {
    for (policy, end) in [
        (EndingPolicy::Depletion, None),
        (EndingPolicy::Either, Some(20)),
    ] {
        assert_eq!(policy.trade_bounds(None, end, 0), Err(EndingError::Ended));
        assert!(policy.trade_bounds(None, end, 1).is_ok());
        let decision = policy.settlement(end, 0).unwrap();
        assert_eq!(decision, EndingDecision::Depleted);
        assert_eq!(decision.reason(), EndingReason::Depleted);
        assert_eq!(
            decision.inclusion_bounds(),
            TimeBounds {
                at_least: None,
                before: None
            }
        );
    }
    assert!(EndingPolicy::Manual.trade_bounds(None, None, 0).is_ok());
    assert!(
        EndingPolicy::Scheduled
            .trade_bounds(None, Some(20), 0)
            .unwrap()
            .contains(19)
    );
}

#[test]
fn scheduled_settlement_keeps_its_bound_even_after_inventory_depletion() {
    for remaining in [0, 1, u128::MAX] {
        let decision = EndingPolicy::Scheduled
            .settlement(Some(20), remaining)
            .unwrap();
        assert_eq!(decision, EndingDecision::ScheduledEnd { timestamp: 20 });
        assert_eq!(decision.reason(), EndingReason::ScheduledEnd);
        let bounds = decision.inclusion_bounds();
        assert_eq!(
            bounds,
            TimeBounds {
                at_least: Some(20),
                before: None
            }
        );
        assert!(!bounds.contains(19));
        assert!(bounds.contains(20));
        assert!(bounds.contains(u64::MAX));
    }
}

#[test]
fn non_depleted_inventory_uses_only_selected_time_ending() {
    for remaining in [1, u128::MAX] {
        assert_eq!(
            EndingPolicy::Depletion.settlement(None, remaining),
            Err(EndingError::NotEnded)
        );
        let decision = EndingPolicy::Either
            .settlement(Some(20), remaining)
            .unwrap();
        assert_eq!(decision, EndingDecision::ScheduledEnd { timestamp: 20 });
        assert_eq!(decision.inclusion_bounds().at_least, Some(20));
    }
}

#[test]
fn manual_closure_requires_engine_state_instead_of_reserve_depletion() {
    for remaining in [0, 1, u128::MAX] {
        assert_eq!(
            EndingPolicy::Manual.settlement(None, remaining),
            Err(EndingError::NotEnded)
        );
    }
    assert_eq!(EndingDecision::Manual.reason(), EndingReason::Manual);
    assert_eq!(
        EndingDecision::Manual.inclusion_bounds(),
        TimeBounds {
            at_least: None,
            before: None
        }
    );
}
