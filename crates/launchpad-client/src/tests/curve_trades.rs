use lee::AccountId;
use lee_core::account::Actor;

use crate::curve::factory_pool_addresses;
use crate::{
    ClientError, ExactInputTradeTerms, ExactOutputTradeTerms, build_buy_invocation,
    build_buy_with_collateral_invocation, build_sell_invocation, prepare_buy_invocation,
    prepare_buy_with_collateral_invocation, prepare_sell_invocation, quote_buy,
    quote_buy_with_collateral, quote_sell,
};

use super::fixtures::{CURVE_PROGRAM_ID, pool_snapshot, sale};

#[test]
fn exact_output_purchase_preparation_keeps_membership_and_price_limits() {
    let (pool, config) = pool_snapshot();
    let membership = curve_core::MembershipPath {
        index: 3,
        salt: [7; 32],
        siblings: [[8; 32]; 17],
    };
    let call = prepare_buy_invocation(
        sale(),
        AccountId::new([9; 32]),
        &pool,
        &config,
        ExactOutputTradeTerms {
            amount_out: 200,
            max_amount_in: 25,
        },
        Some(membership.clone()),
        1,
        None,
    )
    .unwrap();
    let curve_core::Message::Execute { instruction, .. } = call.instruction else {
        panic!("signed purchase");
    };
    assert_eq!(
        *instruction,
        curve_core::Message::SwapExactOutput {
            amount_out: 200,
            max_amount_in: 25,
            token_in: sale().collateral_definition,
            membership: Some(membership),
            now: 1,
        }
    );
}

#[test]
fn exact_output_buy_uses_collateral_input_and_typed_participant_limits() {
    let participant = AccountId::new([9; 32]);
    let call = build_buy_invocation(
        sale(),
        participant,
        ExactOutputTradeTerms {
            amount_out: 25,
            max_amount_in: 100,
        },
        None,
        0,
    );
    assert_eq!(call.destination, Actor::new(participant, CURVE_PROGRAM_ID));
    assert_eq!(call.signer_accounts, [participant]);
    let curve_core::Message::Execute { pool, instruction } = call.instruction else {
        panic!("signed trade envelope");
    };
    assert_eq!(pool, factory_pool_addresses(sale()).pool);
    assert!(
        matches!(*instruction, curve_core::Message::SwapExactOutput { amount_out: 25, max_amount_in: 100, token_in, membership: None, now: 0 } if token_in == sale().collateral_definition)
    );
}

#[test]
fn collateral_buy_uses_collateral_input_and_typed_participant_limits() {
    let participant = AccountId::new([9; 32]);
    let call = build_buy_with_collateral_invocation(
        sale(),
        participant,
        ExactInputTradeTerms {
            amount_in: 25,
            min_amount_out: 100,
        },
        None,
        0,
    );
    assert_eq!(call.destination, Actor::new(participant, CURVE_PROGRAM_ID));
    assert_eq!(call.signer_accounts, [participant]);
    let curve_core::Message::Execute { pool, instruction } = call.instruction else {
        panic!("signed trade envelope");
    };
    assert_eq!(pool, factory_pool_addresses(sale()).pool);
    assert!(
        matches!(*instruction, curve_core::Message::SwapExactInput { amount_in: 25, min_amount_out: 100, token_in, membership: None, now: 0 } if token_in == sale().collateral_definition)
    );
}

#[test]
fn sell_uses_launch_token_input_and_typed_participant_limits() {
    let participant = AccountId::new([9; 32]);
    let call = build_sell_invocation(
        sale(),
        participant,
        ExactInputTradeTerms {
            amount_in: 25,
            min_amount_out: 10,
        },
        0,
    );
    assert_eq!(call.destination, Actor::new(participant, CURVE_PROGRAM_ID));
    assert_eq!(call.signer_accounts, [participant]);
    let curve_core::Message::Execute { pool, instruction } = call.instruction else {
        panic!("signed trade envelope");
    };
    assert_eq!(pool, factory_pool_addresses(sale()).pool);
    assert!(
        matches!(*instruction, curve_core::Message::SwapExactInput { amount_in: 25, min_amount_out: 10, token_in, membership: None, now: 0 } if token_in == factory_pool_addresses(sale()).token_definition)
    );
}

#[test]
fn collateral_purchase_preparation_keeps_membership_and_price_limits() {
    let (pool, config) = pool_snapshot();
    let membership = curve_core::MembershipPath {
        index: 0x01020304,
        salt: [7; 32],
        siblings: [[8; 32]; 17],
    };
    let call = prepare_buy_with_collateral_invocation(
        sale(),
        AccountId::new([9; 32]),
        &pool,
        &config,
        ExactInputTradeTerms {
            amount_in: 25,
            min_amount_out: 200,
        },
        Some(membership.clone()),
        1,
        None,
    )
    .unwrap();
    let curve_core::Message::Execute { instruction, .. } = call.instruction else {
        panic!("signed purchase");
    };
    assert_eq!(
        *instruction,
        curve_core::Message::SwapExactInput {
            amount_in: 25,
            min_amount_out: 200,
            token_in: sale().collateral_definition,
            membership: Some(membership),
            now: 1,
        }
    );
}

#[test]
fn collateral_purchase_preparation_rejects_output_below_participant_floor() {
    let (pool, config) = pool_snapshot();
    assert!(matches!(
        prepare_buy_with_collateral_invocation(
            sale(),
            AccountId::new([9; 32]),
            &pool,
            &config,
            ExactInputTradeTerms {
                amount_in: 25,
                min_amount_out: 201
            },
            None,
            1,
            None,
        ),
        Err(ClientError::QuoteBuyWithCollateral(
            pool::SwapError::SlippageExceeded
        ))
    ));
}

#[test]
fn sale_preparation_rejects_output_below_participant_floor() {
    let (pool, config) = pool_snapshot();
    assert!(matches!(
        prepare_sell_invocation(
            sale(),
            AccountId::new([9; 32]),
            &pool,
            &config,
            ExactInputTradeTerms {
                amount_in: 250,
                min_amount_out: 21
            },
            1
        ),
        Err(ClientError::QuoteSell(pool::SwapError::SlippageExceeded))
    ));
}

#[test]
fn buy_only_quotes_reject_selling_and_keep_both_buy_forms() {
    let (mut pool, config) = pool_snapshot();
    pool.allowed_input_token = Some(pool::TokenSide::Token1);
    assert!(matches!(
        quote_sell(&pool, &config, 250, 1),
        Err(ClientError::InputTokenRestricted)
    ));
    assert_eq!(
        quote_buy(&pool, &config, 200, 1, None).unwrap().amount_in,
        25
    );
    assert_eq!(
        quote_buy_with_collateral(&pool, &config, 25, 1, None)
            .unwrap()
            .amount_out,
        200
    );
}

#[test]
fn token0_only_pool_quotes_reject_both_collateral_buy_forms() {
    let (mut pool, config) = pool_snapshot();
    pool.allowed_input_token = Some(pool::TokenSide::Token0);
    let quotes = [
        quote_buy(&pool, &config, 200, 1, None),
        quote_buy_with_collateral(&pool, &config, 25, 1, None),
    ];
    for (form, quote) in ["exact-output", "exact-input"].into_iter().zip(quotes) {
        assert!(
            matches!(quote, Err(ClientError::InputTokenRestricted)),
            "{form} buy returned {quote:?}"
        );
    }
}

#[test]
fn quote_builders_match_pool_snapshot_rules_and_map_errors() {
    let (pool, config) = pool_snapshot();
    let exact_output = quote_buy(&pool, &config, 200, 1, None).unwrap();
    let exact_input = quote_buy_with_collateral(&pool, &config, 25, 1, None).unwrap();
    let sell = quote_sell(&pool, &config, 250, 1).unwrap();

    assert_eq!((exact_output.amount_in, exact_output.amount_out), (25, 200));
    assert_eq!((exact_input.amount_in, exact_input.amount_out), (25, 200));
    assert_eq!((sell.amount_in, sell.amount_out), (250, 20));
    assert_eq!(pool.pool.k, 100_000);
    assert_eq!(pool.pool.real_reserve0, 800);
    assert_eq!(pool.pool.real_reserve1, 100);
    assert!(matches!(
        quote_buy(&pool, &config, 801, 1, None),
        Err(ClientError::QuoteBuy(
            pool::SwapError::InsufficientRealOutputReserve
        ))
    ));
    assert!(matches!(
        quote_sell(&pool, &config, 0, 1),
        Err(ClientError::QuoteSell(pool::SwapError::ZeroAmount))
    ));
    assert!(matches!(
        quote_buy_with_collateral(&pool, &config, 0, 1, None),
        Err(ClientError::QuoteBuyWithCollateral(
            pool::SwapError::ZeroAmount
        ))
    ));
    let mut pending = pool;
    pending.funded = false;
    assert!(matches!(
        quote_buy(&pending, &config, 1, 1, None),
        Err(ClientError::PoolFundingPending)
    ));
}

#[test]
fn paused_curve_quotes_reject_both_buy_forms_and_sells() {
    let (mut pool, config) = pool_snapshot();
    pool.trade_control = curve_core::TradeControl::Paused;
    for result in [
        quote_buy(&pool, &config, 1, 1, None),
        quote_buy_with_collateral(&pool, &config, 1, 1, None),
        quote_sell(&pool, &config, 1, 1),
    ] {
        assert!(matches!(result, Err(ClientError::PoolPaused)));
    }
}

#[test]
fn malformed_policy_quotes_keep_the_quote_error_label_and_source() {
    use std::error::Error;
    let (original, config) = pool_snapshot();
    for change in 0..4 {
        let mut pool = original.clone();
        match change {
            0 => pool.pool.ending_policy = curve_core::EndingPolicy::Either,
            1 => pool.pool.ending_policy = curve_core::EndingPolicy::Scheduled,
            2 => pool.pool.ending_policy = curve_core::EndingPolicy::Depletion,
            3 => pool.pool.close_timestamp = Some(100),
            _ => unreachable!(),
        }
        let before = pool.clone();
        for (error, label) in [
            (
                quote_buy(&pool, &config, 1, 0, None).unwrap_err(),
                "quote_buy",
            ),
            (quote_sell(&pool, &config, 1, 0).unwrap_err(), "quote_sell"),
            (
                quote_buy_with_collateral(&pool, &config, 1, 0, None).unwrap_err(),
                "quote_buy_with_collateral",
            ),
        ] {
            let actual: &'static str = (&error).into();
            assert_eq!(actual, label);
            assert!(matches!(
                error.source().unwrap().downcast_ref::<pool::SwapError>(),
                Some(pool::SwapError::InvalidPolicy)
            ));
        }
        assert_eq!(pool, before);
    }
}

#[test]
fn curve_quotes_expose_retained_fee_without_changing_buy_quotes() {
    let (mut pool, mut config) = pool_snapshot();
    config.protocol_fee_bps = 1_000;
    let zero_sell = quote_sell(&pool, &config, 250, 1).unwrap();
    let zero_buy = quote_buy_with_collateral(&pool, &config, 25, 1, None).unwrap();
    assert_eq!(zero_sell.retained_fee, 0);
    pool.pool.sell_fee_bps = 500;
    let sell = quote_sell(&pool, &config, 250, 1).unwrap();
    assert_eq!(
        (
            sell.raw_amount_out,
            sell.protocol_fee,
            sell.retained_fee,
            sell.amount_out
        ),
        (20, 2, 1, 17)
    );
    assert_eq!(
        quote_buy_with_collateral(&pool, &config, 25, 1, None).unwrap(),
        zero_buy
    );
    assert_eq!(pool.pool.k, pool.pool.initial_k);
    config.protocol_fee_bps = 9_500;
    assert!(quote_sell(&pool, &config, 250, 1).is_err());
    assert!(quote_buy_with_collateral(&pool, &config, 100, 1, None).is_ok());
}

#[test]
fn capped_curve_quotes_use_remaining_allowance_for_both_buy_forms() {
    let (mut pool, config) = pool_snapshot();
    pool.block_allocation = curve_core::BlockAllocation {
        limit: Some(250),
        observed_block: Some(7),
        tokens_sold: 100,
    };
    assert!(matches!(
        quote_buy(&pool, &config, 200, 1, None),
        Err(ClientError::BlockIdRequired)
    ));
    for quote in [
        quote_buy(&pool, &config, 200, 1, Some(7)),
        quote_buy_with_collateral(&pool, &config, 25, 1, Some(7)),
    ] {
        assert!(matches!(
            quote,
            Err(ClientError::CurveBlockAllocationExceeded)
        ));
    }
    assert_eq!(
        quote_buy(&pool, &config, 200, 1, Some(8))
            .unwrap()
            .amount_out,
        200
    );
}
