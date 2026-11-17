use borsh::{BorshDeserialize, BorshSerialize};
use sale_policy::{
    GraduationPolicy, SettlementAmounts, SettlementError, SettlementPayouts, SettlementProgress,
    SettlementRoute,
};
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
    assert_traits::<GraduationPolicy>();
    assert_traits::<SettlementPayouts>();
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

#[test]
fn independent_shares_split_each_asset_and_preserve_every_unit() {
    for launch_tokens in [0, 1, 3, 9_999, 10_000, 10_001, u128::MAX - 1, u128::MAX] {
        for collateral in [0, 1, 3, 9_999, 10_000, 10_001, u128::MAX - 1, u128::MAX] {
            for launch_bps in [0, 1, 3_333, 5_000, 9_999, 10_000] {
                for collateral_bps in [0, 1, 3_333, 5_000, 9_999, 10_000] {
                    let policy = GraduationPolicy {
                        launch_bps,
                        collateral_bps,
                    };
                    let amounts = SettlementAmounts {
                        launch_tokens,
                        collateral,
                    };
                    let payouts = policy
                        .payouts(amounts, SettlementRoute::Graduation, true)
                        .unwrap();
                    // Divide first to keep this independent check within u128.
                    let expected = |amount: u128, bps: u16| {
                        amount / 10_000 * u128::from(bps)
                            + (amount % 10_000 * u128::from(bps)) / 10_000
                    };
                    assert_eq!(
                        payouts.adapter.launch_tokens,
                        expected(launch_tokens, launch_bps)
                    );
                    assert_eq!(
                        payouts.adapter.collateral,
                        expected(collateral, collateral_bps)
                    );
                    assert_eq!(
                        payouts
                            .creator
                            .launch_tokens
                            .checked_add(payouts.adapter.launch_tokens),
                        Some(launch_tokens)
                    );
                    assert_eq!(
                        payouts
                            .creator
                            .collateral
                            .checked_add(payouts.adapter.collateral),
                        Some(collateral)
                    );
                }
            }
        }
    }
}

#[test]
fn adapter_rounds_down_and_creator_receives_the_remainder() {
    let payouts = GraduationPolicy {
        launch_bps: 3_333,
        collateral_bps: 6_667,
    }
    .payouts(
        SettlementAmounts {
            launch_tokens: 7,
            collateral: 7,
        },
        SettlementRoute::Graduation,
        true,
    )
    .unwrap();
    assert_eq!(
        payouts.adapter,
        SettlementAmounts {
            launch_tokens: 2,
            collateral: 4
        }
    );
    assert_eq!(
        payouts.creator,
        SettlementAmounts {
            launch_tokens: 5,
            collateral: 3
        }
    );
}

#[test]
fn invalid_percentages_and_nonzero_shares_without_adapter_are_rejected() {
    for policy in [
        GraduationPolicy {
            launch_bps: 10_001,
            collateral_bps: 0,
        },
        GraduationPolicy {
            launch_bps: 0,
            collateral_bps: 10_001,
        },
        GraduationPolicy {
            launch_bps: u16::MAX,
            collateral_bps: u16::MAX,
        },
    ] {
        for has_adapter in [false, true] {
            assert_eq!(
                policy.validate(has_adapter),
                Err(SettlementError::InvalidGraduationPolicy)
            );
            for route in [SettlementRoute::Manual, SettlementRoute::Graduation] {
                assert_eq!(
                    policy.payouts(SettlementAmounts::default(), route, has_adapter),
                    Err(SettlementError::InvalidGraduationPolicy)
                );
            }
        }
    }
    for policy in [
        GraduationPolicy {
            launch_bps: 1,
            collateral_bps: 0,
        },
        GraduationPolicy {
            launch_bps: 0,
            collateral_bps: 1,
        },
        GraduationPolicy {
            launch_bps: 10_000,
            collateral_bps: 10_000,
        },
    ] {
        assert_eq!(
            policy.validate(false),
            Err(SettlementError::AdapterRequired)
        );
        assert_eq!(policy.validate(true), Ok(()));
        assert_eq!(
            policy.payouts(SettlementAmounts::default(), SettlementRoute::Manual, false),
            Err(SettlementError::AdapterRequired)
        );
    }
}

#[test]
fn saved_adapter_requires_graduation_even_for_empty_balances_and_zero_shares() {
    for policy in [
        GraduationPolicy::default(),
        GraduationPolicy {
            launch_bps: 10_000,
            collateral_bps: 0,
        },
        GraduationPolicy {
            launch_bps: 0,
            collateral_bps: 10_000,
        },
    ] {
        for amounts in [
            SettlementAmounts::default(),
            SettlementAmounts {
                launch_tokens: 7,
                collateral: 11,
            },
        ] {
            assert_eq!(
                policy.payouts(amounts, SettlementRoute::Manual, true),
                Err(SettlementError::RouteMismatch)
            );
            assert!(
                policy
                    .payouts(amounts, SettlementRoute::Graduation, true)
                    .is_ok()
            );
        }
    }
}

#[test]
fn no_adapter_allows_only_manual_payout_of_both_assets_to_creator() {
    for amounts in [
        SettlementAmounts::default(),
        SettlementAmounts {
            launch_tokens: u128::MAX,
            collateral: u128::MAX,
        },
    ] {
        let policy = GraduationPolicy::default();
        assert_eq!(policy.validate(false), Ok(()));
        assert_eq!(
            policy.payouts(amounts, SettlementRoute::Manual, false),
            Ok(SettlementPayouts {
                creator: amounts,
                adapter: SettlementAmounts::default()
            })
        );
        assert_eq!(
            policy.payouts(amounts, SettlementRoute::Graduation, false),
            Err(SettlementError::RouteMismatch)
        );
    }
}

#[test]
fn payout_policy_and_plan_have_fixed_borsh_field_order() {
    let policy = GraduationPolicy {
        launch_bps: 1_234,
        collateral_bps: 5_678,
    };
    let bytes = borsh::to_vec(&policy).unwrap();
    assert_eq!(
        bytes,
        [1_234u16.to_le_bytes(), 5_678u16.to_le_bytes()].concat()
    );
    assert_eq!(
        borsh::from_slice::<GraduationPolicy>(&bytes).unwrap(),
        policy
    );
    let payouts = policy
        .payouts(
            SettlementAmounts {
                launch_tokens: 10_000,
                collateral: 10_000,
            },
            SettlementRoute::Graduation,
            true,
        )
        .unwrap();
    let bytes = borsh::to_vec(&payouts).unwrap();
    assert_eq!(
        bytes,
        [
            8_766u128.to_le_bytes(),
            4_322u128.to_le_bytes(),
            1_234u128.to_le_bytes(),
            5_678u128.to_le_bytes()
        ]
        .concat()
    );
    assert_eq!(
        borsh::from_slice::<SettlementPayouts>(&bytes).unwrap(),
        payouts
    );
}

#[test]
fn route_validation_requires_the_saved_adapter_choice_before_planning() {
    for launch_bps in [0, 1, 10_000] {
        for collateral_bps in [0, 1, 10_000] {
            let policy = GraduationPolicy {
                launch_bps,
                collateral_bps,
            };
            assert_eq!(
                policy.validate_route(SettlementRoute::Manual, true),
                Err(SettlementError::RouteMismatch)
            );
            assert_eq!(
                policy.validate_route(SettlementRoute::Graduation, true),
                Ok(())
            );
            if launch_bps == 0 && collateral_bps == 0 {
                assert_eq!(
                    policy.validate_route(SettlementRoute::Manual, false),
                    Ok(())
                );
                assert_eq!(
                    policy.validate_route(SettlementRoute::Graduation, false),
                    Err(SettlementError::RouteMismatch)
                );
            } else {
                for route in [SettlementRoute::Manual, SettlementRoute::Graduation] {
                    assert_eq!(
                        policy.validate_route(route, false),
                        Err(SettlementError::AdapterRequired)
                    );
                }
            }
        }
    }
}
