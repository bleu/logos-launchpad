use borsh::{BorshDeserialize, BorshSerialize};
use sale_policy::{SettlementAmounts, SettlementError, SettlementProgress, SettlementRoute};
use serde::{Deserialize, Serialize};

const PROGRESS: [SettlementProgress; 4] = [
    SettlementProgress::Unselected,
    SettlementProgress::ManualStarted,
    SettlementProgress::ManualComplete,
    SettlementProgress::Graduated,
];

#[test]
fn route_and_completion_matrix() {
    let expected = [
        (None, false),
        (Some(SettlementRoute::Manual), false),
        (Some(SettlementRoute::Manual), true),
        (Some(SettlementRoute::Graduation), true),
    ];
    for (progress, (route, complete)) in PROGRESS.into_iter().zip(expected) {
        assert_eq!(progress.route(), route, "{progress:?}");
        assert_eq!(progress.is_complete(), complete, "{progress:?}");
    }
}

#[test]
fn availability_matrix() {
    use SettlementError::{AlreadySettled, ManualPayoutStarted};
    let expected = [
        (Ok(()), Ok(())),
        (Ok(()), Err(ManualPayoutStarted)),
        (Err(AlreadySettled), Err(ManualPayoutStarted)),
        (Err(AlreadySettled), Err(AlreadySettled)),
    ];
    for (progress, (manual, graduation)) in PROGRESS.into_iter().zip(expected) {
        assert_eq!(
            progress.ensure_available(SettlementRoute::Manual),
            manual,
            "{progress:?} manual"
        );
        assert_eq!(
            progress.ensure_available(SettlementRoute::Graduation),
            graduation,
            "{progress:?} graduation"
        );
    }
}

#[test]
fn manual_payout_amount_matrix_preserves_zero_and_checks_each_asset_without_summing() {
    for initial in PROGRESS {
        for launch_tokens in [0, 1, u128::MAX] {
            for collateral in [0, 1, u128::MAX] {
                let mut progress = initial;
                let amounts = SettlementAmounts {
                    launch_tokens,
                    collateral,
                };
                let expected = match initial {
                    SettlementProgress::Unselected | SettlementProgress::ManualStarted => Ok(()),
                    _ => Err(SettlementError::AlreadySettled),
                };
                assert_eq!(progress.record_manual_payout(amounts), expected);
                let expected_progress =
                    if expected.is_ok() && (launch_tokens != 0 || collateral != 0) {
                        SettlementProgress::ManualStarted
                    } else {
                        initial
                    };
                assert_eq!(progress, expected_progress, "{initial:?}, {amounts:?}");
            }
        }
    }
}

#[test]
fn manual_completion_matrix_and_errors_leave_state_unchanged() {
    let expected = [
        (Ok(()), SettlementProgress::ManualComplete),
        (Ok(()), SettlementProgress::ManualComplete),
        (
            Err(SettlementError::AlreadySettled),
            SettlementProgress::ManualComplete,
        ),
        (
            Err(SettlementError::AlreadySettled),
            SettlementProgress::Graduated,
        ),
    ];
    for (initial, (result, final_progress)) in PROGRESS.into_iter().zip(expected) {
        let mut progress = initial;
        assert_eq!(progress.finish_manual(), result, "{initial:?}");
        assert_eq!(progress, final_progress, "{initial:?}");
    }
}

#[test]
fn graduation_completion_matrix_and_errors_leave_state_unchanged() {
    use SettlementError::{AlreadySettled, ManualPayoutStarted};
    let expected = [
        Ok(()),
        Err(ManualPayoutStarted),
        Err(ManualPayoutStarted),
        Err(AlreadySettled),
    ];
    for (initial, result) in PROGRESS.into_iter().zip(expected) {
        let mut progress = initial;
        assert_eq!(progress.finish_graduation(), result, "{initial:?}");
        assert_eq!(
            progress,
            if result.is_ok() {
                SettlementProgress::Graduated
            } else {
                initial
            },
            "{initial:?}"
        );
    }
}

#[test]
fn zero_manual_completion_blocks_hooks_and_repeated_manual_payouts() {
    let mut progress = SettlementProgress::Unselected;
    progress
        .record_manual_payout(SettlementAmounts {
            launch_tokens: 0,
            collateral: 0,
        })
        .unwrap();
    assert_eq!(progress.route(), None);
    progress.finish_manual().unwrap();
    assert_eq!(progress, SettlementProgress::ManualComplete);
    assert_eq!(progress.route(), Some(SettlementRoute::Manual));
    assert!(progress.is_complete());
    assert_eq!(
        progress.record_manual_payout(SettlementAmounts {
            launch_tokens: 0,
            collateral: 0,
        }),
        Err(SettlementError::AlreadySettled)
    );
    assert_eq!(
        progress.finish_graduation(),
        Err(SettlementError::ManualPayoutStarted)
    );
    assert_eq!(
        progress.finish_manual(),
        Err(SettlementError::AlreadySettled)
    );
    assert_eq!(progress, SettlementProgress::ManualComplete);
}

#[test]
fn zero_manual_payout_does_not_clear_an_existing_manual_route() {
    let mut progress = SettlementProgress::ManualStarted;
    progress
        .record_manual_payout(SettlementAmounts {
            launch_tokens: 0,
            collateral: 0,
        })
        .unwrap();
    assert_eq!(progress, SettlementProgress::ManualStarted);
    progress.finish_manual().unwrap();
    assert_eq!(progress, SettlementProgress::ManualComplete);
}

#[test]
fn empty_hook_selects_graduation_without_a_manual_payout() {
    let mut progress = SettlementProgress::Unselected;
    progress.finish_graduation().unwrap();
    assert_eq!(progress.route(), Some(SettlementRoute::Graduation));
    assert!(progress.is_complete());
    assert_eq!(
        progress.finish_manual(),
        Err(SettlementError::AlreadySettled)
    );
}

#[test]
fn persisted_types_have_borsh_and_serde_traits() {
    fn assert_traits<T>()
    where
        T: BorshSerialize + BorshDeserialize + Serialize + for<'de> Deserialize<'de>,
    {
    }
    assert_traits::<SettlementRoute>();
    assert_traits::<SettlementProgress>();
    assert_traits::<SettlementAmounts>();
}

#[test]
fn borsh_round_trip_and_enum_tags() {
    for (progress, tag) in PROGRESS.into_iter().zip(0u8..4) {
        let bytes = borsh::to_vec(&progress).unwrap();
        assert_eq!(bytes, vec![tag]);
        assert_eq!(
            borsh::from_slice::<SettlementProgress>(&bytes).unwrap(),
            progress
        );
    }
    for (route, tag) in [SettlementRoute::Manual, SettlementRoute::Graduation]
        .into_iter()
        .zip(0u8..2)
    {
        let bytes = borsh::to_vec(&route).unwrap();
        assert_eq!(bytes, vec![tag]);
        assert_eq!(borsh::from_slice::<SettlementRoute>(&bytes).unwrap(), route);
    }
    for launch_tokens in [0u128, 1, u128::MAX] {
        for collateral in [0u128, 1, u128::MAX] {
            let amounts = SettlementAmounts {
                launch_tokens,
                collateral,
            };
            let bytes = borsh::to_vec(&amounts).unwrap();
            let expected = [launch_tokens.to_le_bytes(), collateral.to_le_bytes()].concat();
            assert_eq!(bytes, expected);
            assert_eq!(
                borsh::from_slice::<SettlementAmounts>(&bytes).unwrap(),
                amounts
            );
        }
    }
}

#[test]
fn borsh_rejects_unknown_tags_truncated_amounts_and_trailing_bytes() {
    assert!(borsh::from_slice::<SettlementRoute>(&[2]).is_err());
    assert!(borsh::from_slice::<SettlementProgress>(&[4]).is_err());
    assert!(borsh::from_slice::<SettlementProgress>(&[0, 0]).is_err());
    assert!(borsh::from_slice::<SettlementAmounts>(&[0; 31]).is_err());
    assert!(borsh::from_slice::<SettlementAmounts>(&[0; 33]).is_err());
}
