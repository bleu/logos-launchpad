use sale_policy::{CollateralSplit, FeeError, FeeRate};

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
