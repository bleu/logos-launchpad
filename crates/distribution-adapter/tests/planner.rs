use distribution_adapter::{Configuration, Error, Rule, VERSION};
use lee_core::account::AccountId;
fn config(weights: &[u16]) -> Configuration {
    let rules = weights
        .iter()
        .enumerate()
        .map(|(i, bps)| Rule {
            destination: AccountId::new([i as u8; 32]),
            bps: *bps,
        })
        .collect::<Vec<_>>();
    Configuration {
        version: VERSION,
        launch_rules: rules.clone(),
        collateral_rules: rules,
    }
}
#[test]
fn percentages_use_full_initial_balance_and_last_recipient_takes_dust() {
    let payouts = config(&[5000, 3000, 2000]).plan([101, 0]).unwrap();
    assert_eq!(
        payouts.iter().map(|p| p.amount).collect::<Vec<_>>(),
        [50, 30, 21]
    );
    assert_eq!(
        config(&[5000, 3000, 2000]).plan([1, 0]).unwrap()[0].destination,
        AccountId::new([2; 32])
    );
    assert!(config(&[10000]).plan([0, 0]).unwrap().is_empty());
}
#[test]
fn conservation_for_both_assets_full_u128_and_every_rule_count() {
    for count in 1..=8 {
        let mut weights = vec![10000 / count as u16; count];
        weights[count - 1] += 10000 - weights.iter().sum::<u16>();
        for amount in [
            0,
            1,
            7,
            9999,
            10000,
            10001,
            u64::MAX as u128,
            u128::MAX - 1,
            u128::MAX,
        ] {
            let plan = config(&weights).plan([amount, amount]).unwrap();
            for asset in 0..2 {
                assert_eq!(
                    plan.iter()
                        .filter(|p| p.asset == asset)
                        .try_fold(0u128, |n, p| n.checked_add(p.amount)),
                    Some(amount)
                );
            }
        }
    }
}
#[test]
fn invalid_versions_totals_lengths_and_self_destinations_are_rejected() {
    for weights in [
        vec![],
        vec![9999],
        vec![10001],
        vec![5000, 5001],
        vec![10000; 9],
    ] {
        assert_eq!(
            config(&weights).validate(),
            Err(Error::InvalidConfiguration)
        );
    }
    let mut value = config(&[10000]);
    value.version = 2;
    assert!(value.validate().is_err());
    assert!(
        config(&[10000])
            .validate_custody(AccountId::new([0; 32]))
            .is_err()
    );
    assert!(Configuration::decode(&[255; 1000]).is_err());
    assert!(Configuration::decode(&[1, 255, 255, 255, 255]).is_err());
}
#[test]
fn configuration_wire_is_independent_of_borsh_serializer() {
    let value = config(&[10000]);
    let mut bytes = vec![1];
    for _ in 0..2 {
        bytes.extend(1u32.to_le_bytes());
        bytes.extend([0; 32]);
        bytes.extend(10000u16.to_le_bytes());
    }
    assert_eq!(borsh::to_vec(&value).unwrap(), bytes);
    assert_eq!(Configuration::decode(&bytes).unwrap(), value);
    assert_eq!(
        borsh::to_vec(&distribution_adapter::Message::DistributeRemaining).unwrap(),
        [3]
    );
    assert_eq!(
        borsh::to_vec(&distribution_adapter::Message::ReadDistributionState).unwrap(),
        [4]
    );
}
