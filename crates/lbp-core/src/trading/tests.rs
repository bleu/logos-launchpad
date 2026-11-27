use super::{Pending, settlement_payout};
use crate::{
    CreatorAllocationRelease, Error, FixedPrice, Message, PoolAccount, PoolMode, TradeControl,
};
use curve_core::Config;
use lee_core::{
    account::{AccountId, Actor},
    program::ReceiveInput,
};

fn fixture() -> (PoolAccount, Config) {
    let id = |n| AccountId::new([n; 32]);
    let namespace = id(40);
    let namespace_program = id(6);
    let namespace_input = ReceiveInput {
        receiver: Actor::new(
            curve_core::compute_config_pda(namespace, namespace_program),
            namespace_program,
        ),
        from: Some(Actor::new(namespace, namespace_program)),
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&curve_core::Message::InitializeNamespace {
            namespace,
            protocol_fee_bps: 0,
            treasury: id(43),
        })
        .unwrap(),
    };
    let config = Config::decode(
        curve_core::handle(&namespace_input)
            .unwrap()
            .post_state
            .as_ref()
            .unwrap(),
    )
    .unwrap();
    let state = PoolAccount {
        block_allocation: crate::BlockAllocation::default(),
        buy_policy: crate::BuyPolicy::Open,
        trade_control: TradeControl::Paused,
        ending_policy: crate::EndingPolicy::Either,
        trade_mode: crate::TradeMode::BuyOnly,
        sell_fee_bps: 0,
        graduation_adapter: None,
        graduation_policy: sale_policy::GraduationPolicy::default(),
        mode: PoolMode::FixedPrice,
        funded: true,
        closure_observed: false,
        namespace,
        namespace_program_id: namespace_program,
        factory_owner: id(7),
        factory_program_id: id(8),
        factory_seed: [3; 32],
        token_definition_id: id(41),
        collateral_definition_id: id(42),
        sale_inventory: 800,
        remaining_inventory: 800,
        creator_allocation: 0,
        price: FixedPrice {
            collateral_units: 3,
            token_units: 2,
        },
        start_timestamp: 100,
        end_timestamp: Some(200),
        creator_allocation_release: CreatorAllocationRelease::AfterSetup,
        recorded_collateral: 0,
        accrued_fees: 0,
        settlement_progress: crate::SettlementProgress::Unselected,
        pending: None,
    };
    (state, config)
}

#[test]
fn final_buy_boundary_checks_common_pause_for_every_buy_form() {
    let id = |n| AccountId::new([n; 32]);
    let (state, config) = fixture();
    let weighted = PoolMode::Weighted {
        token_weight_start: crate::WEIGHT_TOTAL / 2,
        token_weight_end: crate::WEIGHT_TOTAL / 2,
        collateral_seed: 100,
        pricing_collateral: 100,
        checkpoint_time: 0,
        checkpoint_token_weight: 0,
    };
    for (mode, message) in [
        (
            PoolMode::FixedPrice,
            Message::BuyExactInput {
                membership: None,
                amount_in: 5,
                min_amount_out: 1,
                now: 100,
            },
        ),
        (
            PoolMode::FixedPrice,
            Message::BuyExactOutput {
                membership: None,
                amount_out: 3,
                max_amount_in: 6,
                now: 100,
            },
        ),
        (
            weighted,
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 5,
                min_amount_out: 1,
            },
        ),
        (
            weighted,
            Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 3,
                max_amount_in: 6,
            },
        ),
    ] {
        let mut state = state.clone();
        state.mode = mode;
        if matches!(mode, PoolMode::Weighted { .. }) {
            state.recorded_collateral = 100;
            state.ending_policy = crate::EndingPolicy::Scheduled;
        }
        let pending = Pending {
            tag: *b"LZLBPEND",
            state,
            message,
            buyer: id(50),
            step: 4,
            pricing_time: Some(100),
            allocation_block: None,
            config: Some(config.clone()),
            balances: [100, 800, 100],
        };
        assert_eq!(
            pending.finish(Actor::new(id(9), id(8))).err(),
            Some(Error::Paused)
        );
    }
}

#[test]
fn saved_trade_and_settlement_replies_reject_malformed_policies_before_effects() {
    let (base, _) = fixture();
    for weighted in [false, true] {
        let mut state = base.clone();
        state.trade_control = crate::TradeControl::Unrestricted;
        if weighted {
            state.mode = PoolMode::Weighted {
                token_weight_start: crate::WEIGHT_TOTAL / 2,
                token_weight_end: crate::WEIGHT_TOTAL / 2,
                collateral_seed: 100,
                pricing_collateral: 100,
                checkpoint_time: 0,
                checkpoint_token_weight: 0,
            };
            state.ending_policy = crate::EndingPolicy::Scheduled;
        }
        let receiver = Actor::new(
            crate::compute_pool_pda(
                state.namespace,
                AccountId::new([8; 32]),
                state.token_definition_id,
                state.collateral_definition_id,
                state.factory_owner,
            ),
            AccountId::new([8; 32]),
        );
        let buys = if weighted {
            [
                Message::BuyWeightedExactInput {
                    membership: None,
                    amount_in: 5,
                    min_amount_out: 0,
                },
                Message::BuyWeightedExactOutput {
                    membership: None,
                    amount_out: 3,
                    max_amount_in: 6,
                },
            ]
        } else {
            [
                Message::BuyExactInput {
                    membership: None,
                    amount_in: 5,
                    min_amount_out: 0,
                    now: 100,
                },
                Message::BuyExactOutput {
                    membership: None,
                    amount_out: 3,
                    max_amount_in: 6,
                    now: 100,
                },
            ]
        };
        let settle = if weighted {
            Message::SettleWeighted {
                creator: AccountId::new([50; 32]),
            }
        } else {
            Message::SettleFixedPrice {
                creator: AccountId::new([50; 32]),
                now: 200,
            }
        };
        for message in [
            buys[0].clone(),
            buys[1].clone(),
            Message::CollectFees,
            settle,
        ] {
            for step in [0, 4] {
                for case in 0..4 {
                    let mut malformed = state.clone();
                    match case {
                        0 => malformed.ending_policy = crate::EndingPolicy::Manual,
                        1 => malformed.ending_policy = crate::EndingPolicy::Depletion,
                        2 => malformed.start_timestamp = u64::MAX,
                        _ => malformed.end_timestamp = Some(100),
                    }
                    let expected = if case == 0 || (case == 1 && weighted) {
                        Error::InvalidState
                    } else {
                        Error::InvalidTerms
                    };
                    let pending = Pending {
                        tag: *b"LZLBPEND",
                        state: malformed,
                        message: message.clone(),
                        buyer: AccountId::new([50; 32]),
                        step,
                        pricing_time: Some(100),
                        allocation_block: None,
                        config: None,
                        balances: [0; 3],
                    };
                    let input = ReceiveInput {
                        receiver,
                        from: None,
                        is_authorized: false,
                        pre_state: borsh::to_vec(&pending).unwrap().into(),
                        message: vec![],
                    };
                    assert_eq!(crate::handle(&input).err(), Some(expected));
                    assert_eq!(pending.finish(receiver).err(), Some(expected));
                }
            }
        }
    }
}

#[test]
fn shared_manual_completion_errors_map_to_invalid_state() {
    let (state, _) = fixture();
    let receiver = Actor::new(AccountId::new([9; 32]), AccountId::new([8; 32]));
    for progress in [
        crate::SettlementProgress::ManualComplete,
        crate::SettlementProgress::Graduated,
    ] {
        let mut completed = state.clone();
        completed.settlement_progress = progress;
        assert_eq!(
            settlement_payout(
                completed,
                receiver,
                AccountId::new([50; 32]),
                AccountId::new([43; 32]),
                [0, 800, 0],
                Some(200)
            )
            .err(),
            Some(Error::InvalidState)
        );
    }
}
