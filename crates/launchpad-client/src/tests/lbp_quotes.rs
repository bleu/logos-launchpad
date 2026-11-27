use lee::AccountId;
use lee_core::account::Actor;

use crate::lbp::lbp_addresses;
use crate::{
    ClientError, EndingPolicy, GraduationPolicy, LbpQuoteRequest, TradeMode, quote_lbp_trade,
};

use super::fixtures::{lbp_sale, pool_snapshot};

fn lbp_pool_snapshot(weighted: bool) -> (lbp_core::PoolAccount, curve_core::Config) {
    let sale = lbp_sale();
    let a = lbp_addresses(sale);
    let mode = if weighted {
        lbp_core::PoolMode::Weighted {
            token_weight_start: lbp_core::WEIGHT_TOTAL / 2,
            token_weight_end: lbp_core::WEIGHT_TOTAL / 2,
            collateral_seed: 100,
            pricing_collateral: 1_000,
            checkpoint_time: 9,
            checkpoint_token_weight: lbp_core::WEIGHT_TOTAL / 2,
        }
    } else {
        lbp_core::PoolMode::FixedPrice
    };
    let mut bytes = borsh::to_vec(&(
        (None::<u128>, None::<u64>, 0u128),
        curve_core::BuyPolicy::Open,
        curve_core::TradeControl::Unrestricted,
        curve_core::EndingPolicy::Scheduled,
        TradeMode::BuyAndSell,
        0u16,
        None::<Actor>,
        GraduationPolicy::default(),
        mode,
        (true, false),
        (
            sale.namespace,
            sale.namespace_program_id,
            a.factory,
            sale.factory_program_id,
            *factory_core::compute_factory_seed(sale.namespace, sale.launch_salt).as_bytes(),
            a.token_definition,
            sale.collateral_definition,
        ),
        (800u128, 700u128, 200u128),
        lbp_core::FixedPrice {
            collateral_units: 2,
            token_units: 1,
        },
        (
            9u64,
            Some(100u64),
            lbp_core::CreatorAllocationRelease::AfterEnd,
        ),
        (1_000u128, 0u128, lbp_core::SettlementProgress::Unselected),
    ))
    .unwrap();
    bytes.push(0);
    let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    let (_, mut config) = pool_snapshot();
    config.protocol_fee_bps = 1_000;
    (pool, config)
}

#[test]
fn lbp_quotes_reuse_core_results_for_each_engine_and_trade_form() {
    use crate::LbpQuoteRequest::*;
    for weighted in [false, true] {
        let (pool, config) = lbp_pool_snapshot(weighted);
        for request in [
            BuyExactInput {
                collateral_in: 20,
                min_tokens_out: 1,
            },
            BuyExactOutput {
                tokens_out: 5,
                max_collateral_in: 100,
            },
            SellExactInput {
                tokens_in: 10,
                min_collateral_out: 1,
            },
        ] {
            let expected =
                lbp_core::quote_trade(&pool, request, config.protocol_fee_bps, 50).unwrap();
            let before = pool.clone();
            assert_eq!(
                quote_lbp_trade(lbp_sale(), &pool, &config, request, 50, None).unwrap(),
                expected
            );
            assert_eq!(pool, before);
        }
    }
}

#[test]
fn capped_lbp_buy_quotes_check_the_observed_block_allowance() {
    use crate::LbpQuoteRequest::*;
    let buy = BuyExactOutput {
        tokens_out: 5,
        max_collateral_in: 100,
    };
    let sell = SellExactInput {
        tokens_in: 10,
        min_collateral_out: 1,
    };
    for weighted in [false, true] {
        let (mut pool, config) = lbp_pool_snapshot(weighted);
        pool.block_allocation = lbp_core::BlockAllocation {
            limit: Some(8),
            observed_block: Some(7),
            tokens_sold: 4,
        };
        let quote =
            |request, block| quote_lbp_trade(lbp_sale(), &pool, &config, request, 50, block);
        assert!(matches!(
            quote(buy, None),
            Err(ClientError::BlockIdRequired)
        ));
        assert!(matches!(
            quote(buy, Some(7)),
            Err(ClientError::QuoteLbpTrade(
                lbp_core::Error::BlockAllocationExceeded
            ))
        ));
        assert_eq!(
            quote(buy, Some(8)).unwrap(),
            lbp_core::quote_trade(&pool, buy, config.protocol_fee_bps, 50).unwrap()
        );
        // Sells do not use the allowance or the block ID.
        assert!(quote(sell, None).is_ok());
        pool.block_allocation.tokens_sold = 3;
        let quote =
            |request, block| quote_lbp_trade(lbp_sale(), &pool, &config, request, 50, block);
        assert_eq!(quote(buy, Some(7)).unwrap().amount_out, 5);
    }
}

#[test]
fn lbp_quotes_reject_each_pool_identity_mismatch_and_wrong_namespace_config() {
    let (pool, config) = lbp_pool_snapshot(false);
    let request = LbpQuoteRequest::SellExactInput {
        tokens_in: 10,
        min_collateral_out: 0,
    };
    for field in 0..7 {
        let mut wrong = pool.clone();
        let other = AccountId::new([99; 32]);
        match field {
            0 => wrong.namespace = other,
            1 => wrong.namespace_program_id = other,
            2 => wrong.factory_owner = other,
            3 => wrong.factory_program_id = other,
            4 => wrong.factory_seed = [99; 32],
            5 => wrong.token_definition_id = other,
            6 => wrong.collateral_definition_id = other,
            _ => unreachable!(),
        }
        assert!(matches!(
            quote_lbp_trade(lbp_sale(), &wrong, &config, request, 50, None),
            Err(ClientError::FactorySaleMismatch)
        ));
    }
    let mut wrong = config;
    wrong.namespace = AccountId::new([99; 32]);
    assert!(matches!(
        quote_lbp_trade(lbp_sale(), &pool, &wrong, request, 50, None),
        Err(ClientError::PoolSelectionMismatch)
    ));
}

#[test]
fn lbp_quotes_check_live_freeze_and_preserve_core_errors() {
    let request = LbpQuoteRequest::SellExactInput {
        tokens_in: 10,
        min_collateral_out: 0,
    };
    for weighted in [false, true] {
        let (mut pool, mut config) = lbp_pool_snapshot(weighted);
        config.emergency.is_frozen = true;
        assert!(matches!(
            quote_lbp_trade(lbp_sale(), &pool, &config, request, 50, None),
            Err(ClientError::NamespaceFrozen)
        ));
        config.emergency.is_frozen = false;
        pool.trade_mode = TradeMode::BuyOnly;
        assert!(matches!(
            quote_lbp_trade(lbp_sale(), &pool, &config, request, 50, None),
            Err(ClientError::QuoteLbpTrade(
                lbp_core::Error::TradeDirectionUnsupported
            ))
        ));
        pool.trade_mode = TradeMode::BuyAndSell;
        pool.remaining_inventory = 0;
        pool.ending_policy = EndingPolicy::Either;
        assert!(matches!(
            quote_lbp_trade(lbp_sale(), &pool, &config, request, 50, None),
            Err(ClientError::QuoteLbpTrade(lbp_core::Error::Closed))
        ));
    }
}

#[test]
fn lbp_sell_quotes_apply_net_floor_and_protect_weighted_seed() {
    let (pool, config) = lbp_pool_snapshot(false);
    let request = LbpQuoteRequest::SellExactInput {
        tokens_in: 10,
        min_collateral_out: 18,
    };
    let quote = quote_lbp_trade(lbp_sale(), &pool, &config, request, 50, None).unwrap();
    assert_eq!(
        (
            quote.gross_collateral,
            quote.net_collateral,
            quote.protocol_fee,
            quote.amount_out
        ),
        (20, 18, 2, 18)
    );
    let too_high = LbpQuoteRequest::SellExactInput {
        tokens_in: 10,
        min_collateral_out: 19,
    };
    assert!(quote_lbp_trade(lbp_sale(), &pool, &config, too_high, 50, None).is_err());
    let (mut pool, config) = lbp_pool_snapshot(true);
    pool.recorded_collateral = 100;
    if let lbp_core::PoolMode::Weighted {
        pricing_collateral, ..
    } = &mut pool.mode
    {
        *pricing_collateral = 100;
    }
    assert!(quote_lbp_trade(lbp_sale(), &pool, &config, request, 50, None).is_err());
}

#[test]
fn lbp_quotes_expose_independent_retained_fees_and_net_slippage() {
    for weighted in [false, true] {
        let (mut pool, config) = lbp_pool_snapshot(weighted);
        let request = LbpQuoteRequest::SellExactInput {
            tokens_in: 10,
            min_collateral_out: 0,
        };
        let zero = quote_lbp_trade(lbp_sale(), &pool, &config, request, 50, None).unwrap();
        assert_eq!(zero.retained_fee, 0);
        let buy = LbpQuoteRequest::BuyExactInput {
            collateral_in: 25,
            min_tokens_out: 0,
        };
        let zero_buy = quote_lbp_trade(lbp_sale(), &pool, &config, buy, 50, None).unwrap();
        pool.sell_fee_bps = 500;
        let quote = quote_lbp_trade(lbp_sale(), &pool, &config, request, 50, None).unwrap();
        assert_eq!(quote.gross_collateral, zero.gross_collateral);
        assert_eq!(quote.protocol_fee, zero.protocol_fee);
        assert!(quote.retained_fee > 0);
        assert_eq!(
            quote.amount_out + quote.protocol_fee + quote.retained_fee,
            quote.gross_collateral
        );
        assert_eq!(quote.net_collateral, quote.amount_out);
        assert_eq!(
            quote_lbp_trade(lbp_sale(), &pool, &config, buy, 50, None).unwrap(),
            zero_buy
        );
        let too_high = LbpQuoteRequest::SellExactInput {
            tokens_in: 10,
            min_collateral_out: quote.amount_out + 1,
        };
        let core_error =
            lbp_core::quote_trade(&pool, too_high, config.protocol_fee_bps, 50).unwrap_err();
        let Err(ClientError::QuoteLbpTrade(error)) =
            quote_lbp_trade(lbp_sale(), &pool, &config, too_high, 50, None)
        else {
            panic!("expected core slippage error")
        };
        assert_eq!(error, core_error);
    }
}
