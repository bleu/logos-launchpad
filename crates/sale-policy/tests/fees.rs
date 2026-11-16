use sale_policy::{CollateralSplit, FeeError, FeeRate, SellCollateralSplit, SellFeeRate};

#[test]
fn sell_fees_round_independently_and_preserve_collateral() {
    for retained_bps in [0, 1, 17, 100, 9_998, 9_999] {
        let rate = SellFeeRate::new(retained_bps).unwrap();
        for namespace_bps in [0, 1, 37, 100, 9_998, 9_999, 10_000, u16::MAX] {
            for gross in [0, 1, 2, 3, 97, 100, 10_001, 1_000_000] {
                let result = rate.split(gross, namespace_bps);
                if u32::from(retained_bps) + u32::from(namespace_bps) >= 10_000 {
                    assert_eq!(result, Err(FeeError::RateAboveDenominator));
                    continue;
                }
                let protocol_fee = (gross * u128::from(namespace_bps)).div_ceil(10_000);
                let retained_fee = (gross * u128::from(retained_bps)).div_ceil(10_000);
                let total = protocol_fee + retained_fee;
                if total >= gross {
                    assert_eq!(result, Err(FeeError::InputConsumedByFees));
                } else {
                    assert_eq!(
                        result,
                        Ok(SellCollateralSplit {
                            gross,
                            net: gross - total,
                            protocol_fee,
                            retained_fee,
                        })
                    );
                }
            }
        }
    }
}

#[test]
fn stored_sell_rates_are_valid_without_a_live_namespace_rate() {
    for bps in 0..10_000 {
        assert!(SellFeeRate::new(bps).is_ok());
    }
    for bps in 10_000..=u16::MAX {
        assert_eq!(SellFeeRate::new(bps), Err(FeeError::RateAboveDenominator));
    }
    let rate = SellFeeRate::new(200).unwrap();
    assert!(rate.split(10_000, 100).is_ok());
    assert_eq!(
        rate.split(10_000, 9_800),
        Err(FeeError::RateAboveDenominator)
    );
    assert!(rate.split(10_000, 0).is_ok());
}

#[test]
fn zero_retention_preserves_namespace_sell_amounts() {
    let rate = SellFeeRate::new(0).unwrap();
    for bps in 0..10_000 {
        for gross in [2, 101, 10_001, u128::MAX] {
            let prior = FeeRate::new(bps).unwrap().split(gross).unwrap();
            let result = rate.split(gross, bps);
            if prior.net == 0 {
                assert_eq!(result, Err(FeeError::InputConsumedByFees));
            } else {
                let split = result.unwrap();
                assert_eq!(split.gross, prior.gross);
                assert_eq!(split.net, prior.net);
                assert_eq!(split.protocol_fee, prior.fee);
                assert_eq!(split.retained_fee, 0);
            }
        }
    }
}

#[test]
fn retained_sell_fees_support_full_width_collateral() {
    let split = SellFeeRate::new(333)
        .unwrap()
        .split(u128::MAX, 777)
        .unwrap();
    assert_eq!(
        split.protocol_fee,
        FeeRate::new(777).unwrap().split(u128::MAX).unwrap().fee
    );
    assert_eq!(
        split.retained_fee,
        FeeRate::new(333).unwrap().split(u128::MAX).unwrap().fee
    );
    assert_eq!(
        split.net + split.protocol_fee + split.retained_fee,
        u128::MAX
    );
}

#[test]
fn splits_use_ceiling_fees_and_preserve_collateral() {
    for bps in 0..=10_000 {
        let rate = FeeRate::new(bps).unwrap();
        for gross in [0, 1, 2, 9, 100, 10_001] {
            let split = rate.split(gross).unwrap();
            let fee = (gross * u128::from(bps)).div_ceil(10_000);
            assert_eq!(
                split,
                CollateralSplit {
                    gross,
                    net: gross - fee,
                    fee
                }
            );
        }
    }
}

#[test]
fn gross_up_is_the_smallest_payment_that_covers_net_collateral() {
    for bps in 0..10_000 {
        let rate = FeeRate::new(bps).unwrap();
        for net in [0, 1, 2, 9, 100, 10_001] {
            let split = rate.gross_up(net).unwrap();
            assert!(split.net >= net);
            assert_eq!(split.gross, split.net + split.fee);
            if split.gross > 0 {
                assert!(rate.split(split.gross - 1).unwrap().net < net);
            }
        }
    }
}

#[test]
fn full_width_collateral_does_not_need_a_full_width_product() {
    assert_eq!(
        FeeRate::new(0).unwrap().gross_up(u128::MAX),
        Ok(CollateralSplit {
            gross: u128::MAX,
            net: u128::MAX,
            fee: 0
        })
    );
    assert_eq!(
        FeeRate::new(10_000).unwrap().split(u128::MAX),
        Ok(CollateralSplit {
            gross: u128::MAX,
            net: 0,
            fee: u128::MAX
        })
    );
    let split = FeeRate::new(1).unwrap().split(u128::MAX).unwrap();
    assert_eq!(split.fee, u128::MAX.div_ceil(10_000));
    assert_eq!(split.net + split.fee, u128::MAX);
    assert_eq!(
        FeeRate::new(1).unwrap().gross_up(u128::MAX),
        Err(FeeError::Arithmetic)
    );
}

#[test]
fn invalid_rates_and_fee_consumed_inputs_remain_distinct() {
    assert_eq!(FeeRate::new(10_001), Err(FeeError::RateAboveDenominator));
    let rate = FeeRate::new(10_000).unwrap();
    assert_eq!(rate.gross_up(0), Err(FeeError::InputConsumedByFees));
    assert_eq!(rate.gross_up(1), Err(FeeError::InputConsumedByFees));
    assert_eq!(
        rate.split(1),
        Ok(CollateralSplit {
            gross: 1,
            net: 0,
            fee: 1
        })
    );
}
