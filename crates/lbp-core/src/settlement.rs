use super::*;
use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response},
};

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SettlementReceipt {
    pub factory: Actor,
    pub pool: Actor,
    pub route: SettlementRoute,
    pub creator: AccountId,
    pub adapter: Option<Actor>,
    pub eligible: SettlementAmounts,
    pub payouts: SettlementPayouts,
    pub progress: SettlementProgress,
}

impl SettlementReceipt {
    pub fn hash(&self) -> [u8; 32] {
        use risc0_zkvm::sha::{Impl, Sha256 as _};
        Impl::hash_bytes(&borsh::to_vec(self).unwrap())
            .as_bytes()
            .try_into()
            .unwrap()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[expect(
    clippy::large_enum_variant,
    reason = "Keep the settlement wire receipt inline"
)]
pub enum SettlementReply {
    Applied {
        receipt: SettlementReceipt,
    },
    Completed {
        receipt_hash: [u8; 32],
        progress: SettlementProgress,
    },
}

#[derive(BorshSerialize, BorshDeserialize)]
struct AwaitFactoryFinalize {
    tag: [u8; 8],
    state: PoolAccount,
    receipt: SettlementReceipt,
    receipt_hash: [u8; 32],
    expiry: Option<u64>,
}

pub(super) struct CustodyPlan {
    pub fees: u128,
    pub eligible: SettlementAmounts,
    pub payouts: SettlementPayouts,
    pub progress: SettlementProgress,
}

pub(super) fn plan(state: &PoolAccount, route: SettlementRoute) -> Result<CustodyPlan, Error> {
    let eligible = SettlementAmounts {
        launch_tokens: state.remaining_inventory,
        collateral: state.recorded_collateral,
    };
    let payouts = state
        .graduation_policy
        .payouts(eligible, route, state.graduation_adapter.is_some())
        .map_err(|_| Error::InvalidTerms)?;
    state
        .settlement_progress
        .ensure_available(route)
        .map_err(|_| Error::InvalidState)?;
    let mut progress = state.settlement_progress;
    match route {
        SettlementRoute::Manual => {
            progress
                .record_manual_payout(eligible)
                .map_err(|_| Error::InvalidState)?;
            progress.finish_manual().map_err(|_| Error::InvalidState)?;
        }
        SettlementRoute::Graduation => progress
            .finish_graduation()
            .map_err(|_| Error::InvalidState)?,
    }
    Ok(CustodyPlan {
        fees: state.accrued_fees,
        eligible,
        payouts,
        progress,
    })
}

pub(super) fn validate_saved_route(
    state: &PoolAccount,
    route: SettlementRoute,
) -> Result<(), Error> {
    state
        .graduation_policy
        .validate_route(route, state.graduation_adapter.is_some())
        .map(|_| ())
        .map_err(|_| Error::InvalidTerms)
}

pub(super) fn validate_route(
    state: &PoolAccount,
    route: SettlementRoute,
    creator: AccountId,
    adapter: Option<Actor>,
) -> Result<(), Error> {
    validate_saved_route(state, route)?;
    if creator == AccountId::default() {
        return Err(Error::Unauthorized);
    }
    match (route, adapter) {
        (SettlementRoute::Manual, None) => {}
        (SettlementRoute::Graduation, Some(actor)) if Some(actor) == state.graduation_adapter => {
            if actor.account_id == AccountId::default()
                || actor.program_account_id == AccountId::default()
                || actor.account_id == state.factory_owner
                || actor.account_id == creator
            {
                return Err(Error::InvalidTerms);
            }
        }
        _ => return Err(Error::InvalidTerms),
    }
    state
        .settlement_progress
        .ensure_available(route)
        .map_err(|_| Error::InvalidState)
}

pub(super) fn apply(
    mut state: PoolAccount,
    receiver: Actor,
    request: (SettlementRoute, AccountId, Option<Actor>),
    treasury: AccountId,
    balances: [u128; 3],
    expiry: Option<u64>,
) -> Result<Response, Error> {
    let (route, creator, adapter) = request;
    validate_route(&state, route, creator, adapter)?;
    let plan = plan(&state, route)?;
    let liability = state
        .recorded_collateral
        .checked_add(plan.fees)
        .ok_or(Error::Arithmetic)?;
    if balances[0] < liability || balances[1] < plan.eligible.launch_tokens {
        return Err(Error::Underfunded);
    }
    if adapter.is_some_and(|a| a.account_id == receiver.account_id)
        || creator == receiver.account_id
    {
        return Err(Error::InvalidTerms);
    }
    let factory = Actor::new(state.factory_owner, state.factory_program_id);
    let receipt = SettlementReceipt {
        factory,
        pool: receiver,
        route,
        creator,
        adapter,
        eligible: plan.eligible,
        payouts: plan.payouts,
        progress: plan.progress,
    };
    let closure_event = state.observe_closure(receiver)?;
    let pending = AwaitFactoryFinalize {
        tag: *b"LZLBFINL",
        state,
        receipt: receipt.clone(),
        receipt_hash: receipt.hash(),
        expiry,
    };
    let response = curve_core::with_time_bounds(
        Response::set_state(borsh::to_vec(&pending).unwrap()),
        expiry,
        None,
    )
    .map_err(|_| Error::InvalidTerms)?;
    let response = if let Some(event) = closure_event {
        response.event(event)
    } else {
        response
    };
    let response = crate::trading::custody_effects(
        response,
        &pending.state,
        receiver,
        creator,
        adapter,
        treasury,
        &plan,
    );
    Ok(response.send_sync(factory, &SettlementReply::Applied { receipt }))
}

pub(super) fn resume(input: &ReceiveInput) -> Option<Result<Response, Error>> {
    if !input.pre_state.starts_with(b"LZLBFINL") {
        return None;
    }
    Some((|| {
        let pending: AwaitFactoryFinalize =
            borsh::from_slice(&input.pre_state).map_err(|_| Error::InvalidState)?;
        pending.state.validate_address(input.receiver)?;
        let factory = Actor::new(
            pending.state.factory_owner,
            pending.state.factory_program_id,
        );
        if input.from != Some(factory) {
            return Err(Error::Unauthorized);
        }
        let Message::FinalizeSettlement { receipt_hash } =
            borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?
        else {
            return Err(Error::InvalidMessage);
        };
        if receipt_hash != pending.receipt_hash
            || receipt_hash != pending.receipt.hash()
            || pending.receipt.pool != input.receiver
            || pending.receipt.factory != factory
        {
            return Err(Error::InvalidState);
        }
        let mut state = pending.state;
        validate_route(
            &state,
            pending.receipt.route,
            pending.receipt.creator,
            pending.receipt.adapter,
        )?;
        let expected = plan(&state, pending.receipt.route)?;
        if pending.receipt.eligible != expected.eligible
            || pending.receipt.payouts != expected.payouts
            || pending.receipt.progress != expected.progress
            || pending.expiry != state.close_time_bound()?
        {
            return Err(Error::InvalidState);
        }
        let expected_progress = expected.progress;
        state.recorded_collateral = 0;
        state.accrued_fees = 0;
        state.settlement_progress = expected_progress;
        curve_core::with_time_bounds(state.response(), pending.expiry, None)
            .map_err(|_| Error::InvalidTerms)
            .map(|r| {
                r.event(state.event(
                    input.receiver,
                    curve_core::events::SaleEventKindV1::Settlement {
                        step: curve_core::events::SettlementStepV1::Completed,
                        route: Some(crate::events::settlement_route(expected_progress)),
                        effects: vec![],
                    },
                ))
                .send_sync(
                    factory,
                    &SettlementReply::Completed {
                        receipt_hash,
                        progress: expected_progress,
                    },
                )
            })
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuyPolicy, CreatorAllocationRelease, EndingPolicy, FixedPrice, TradeControl};
    use lez_compat_core::token;

    fn id(n: u8) -> AccountId {
        AccountId::new([n; 32])
    }
    fn fixture(weighted: bool) -> (PoolAccount, Actor) {
        let state = PoolAccount {
            buy_policy: BuyPolicy::Open,
            trade_control: TradeControl::Unrestricted,
            ending_policy: if weighted {
                EndingPolicy::Scheduled
            } else {
                EndingPolicy::Either
            },
            trade_mode: crate::TradeMode::BuyOnly,
            sell_fee_bps: 0,
            graduation_adapter: Some(Actor::new(id(70), id(71))),
            graduation_policy: sale_policy::GraduationPolicy::default(),
            mode: if weighted {
                PoolMode::Weighted {
                    token_weight_start: crate::WEIGHT_TOTAL / 2,
                    token_weight_end: crate::WEIGHT_TOTAL / 2,
                    collateral_seed: 100,
                    pricing_collateral: 149,
                    checkpoint_time: 0,
                    checkpoint_token_weight: 0,
                }
            } else {
                PoolMode::FixedPrice
            },
            funded: true,
            closure_observed: false,
            namespace: id(40),
            namespace_program_id: id(6),
            factory_owner: id(7),
            factory_program_id: id(8),
            factory_seed: [3; 32],
            token_definition_id: id(41),
            collateral_definition_id: id(42),
            sale_inventory: 800,
            remaining_inventory: 790,
            creator_allocation: 0,
            price: FixedPrice {
                collateral_units: 3,
                token_units: 2,
            },
            start_timestamp: 100,
            end_timestamp: Some(200),
            creator_allocation_release: CreatorAllocationRelease::AfterSetup,
            recorded_collateral: if weighted { 149 } else { 49 },
            accrued_fees: 2,
            settlement_progress: SettlementProgress::Unselected,
            pending: None,
        };
        let receiver = Actor::new(
            crate::compute_pool_pda(
                state.namespace,
                id(9),
                state.token_definition_id,
                state.collateral_definition_id,
                state.factory_owner,
            ),
            id(9),
        );
        (state, receiver)
    }
    fn applied(response: &Response) -> SettlementReceipt {
        let SettlementReply::Applied { receipt } =
            borsh::from_slice(&response.sync_messages.last().unwrap().message).unwrap()
        else {
            panic!("expected applied receipt")
        };
        receipt
    }
    fn finalize(response: &Response, receiver: Actor, receipt: &SettlementReceipt) -> ReceiveInput {
        ReceiveInput {
            receiver,
            from: Some(receipt.factory),
            is_authorized: false,
            pre_state: response.post_state.clone().unwrap(),
            message: borsh::to_vec(&Message::FinalizeSettlement {
                receipt_hash: receipt.hash(),
            })
            .unwrap(),
        }
    }
    fn token_calls(response: &Response) -> Vec<token::Message> {
        response
            .sync_messages
            .iter()
            .filter(|call| call.to.program_account_id == curve_core::TOKEN_PROGRAM_ACCOUNT_ID)
            .map(|call| borsh::from_slice(&call.message).unwrap())
            .collect()
    }

    #[test]
    fn both_assets_split_including_seed_and_excluding_fees_and_surplus() {
        for weighted in [false, true] {
            for route in [SettlementRoute::Manual, SettlementRoute::Graduation] {
                for policy in [
                    GraduationPolicy::default(),
                    GraduationPolicy {
                        launch_bps: 2500,
                        collateral_bps: 6000,
                    },
                    GraduationPolicy {
                        launch_bps: 10_000,
                        collateral_bps: 10_000,
                    },
                ] {
                    if route == SettlementRoute::Manual && policy != GraduationPolicy::default() {
                        continue;
                    }
                    let (mut state, receiver) = fixture(weighted);
                    state.graduation_adapter = if route == SettlementRoute::Graduation {
                        state.graduation_adapter
                    } else {
                        None
                    };
                    state.graduation_policy = policy;
                    let eligible = SettlementAmounts {
                        launch_tokens: 790,
                        collateral: state.recorded_collateral,
                    };
                    let payouts = policy
                        .payouts(eligible, route, state.graduation_adapter.is_some())
                        .unwrap();
                    let response = apply(
                        state.clone(),
                        receiver,
                        (route, id(50), state.graduation_adapter),
                        id(43),
                        [1000, 900, 0],
                        Some(200),
                    )
                    .unwrap();
                    let receipt = applied(&response);
                    assert_eq!(receipt.eligible, eligible);
                    assert_eq!(receipt.payouts, payouts);
                    let mut expected = vec![token::Message::Transfer {
                        definition: id(42),
                        recipient: id(43),
                        amount: 2,
                    }];
                    for (recipient, amounts) in [
                        (id(50), payouts.creator),
                        (
                            state.graduation_adapter.map_or(id(50), |a| a.account_id),
                            payouts.adapter,
                        ),
                    ] {
                        for (definition, amount) in [
                            (id(41), amounts.launch_tokens),
                            (id(42), amounts.collateral),
                        ] {
                            if amount != 0 {
                                expected.push(token::Message::Transfer {
                                    definition,
                                    recipient,
                                    amount,
                                });
                            }
                        }
                    }
                    assert_eq!(token_calls(&response), expected);
                    let pending: AwaitFactoryFinalize =
                        borsh::from_slice(response.post_state.as_ref().unwrap()).unwrap();
                    assert_eq!(
                        pending.state.settlement_progress,
                        SettlementProgress::Unselected
                    );
                    let done = crate::handle(&finalize(&response, receiver, &receipt)).unwrap();
                    let final_state =
                        PoolAccount::decode(done.post_state.as_ref().unwrap()).unwrap();
                    assert_eq!(final_state.remaining_inventory, 790);
                    assert_eq!(final_state.recorded_collateral, 0);
                    assert_eq!(final_state.accrued_fees, 0);
                    assert_eq!(final_state.settlement_progress, receipt.progress);
                    assert_eq!(done.validity.timestamps.start(), Some(200_000));
                    assert!(plan(&final_state, route).is_err());
                }
            }
        }
    }

    #[test]
    fn finalizer_binds_context_totals_each_share_and_progress() {
        let (mut state, receiver) = fixture(true);
        state.graduation_policy = GraduationPolicy {
            launch_bps: 2500,
            collateral_bps: 6000,
        };
        let response = apply(
            state.clone(),
            receiver,
            (
                SettlementRoute::Graduation,
                id(50),
                state.graduation_adapter,
            ),
            id(43),
            [1000, 900, 0],
            Some(200),
        )
        .unwrap();
        let receipt = applied(&response);
        for case in 0..9 {
            let mut pending: AwaitFactoryFinalize =
                borsh::from_slice(response.post_state.as_ref().unwrap()).unwrap();
            match case {
                0 => pending.receipt.pool.account_id = id(99),
                1 => pending.receipt.factory.account_id = id(99),
                2 => pending.receipt.progress = SettlementProgress::ManualComplete,
                3 => pending.receipt.eligible.launch_tokens += 1,
                4 => pending.receipt.eligible.collateral += 1,
                5 => pending.receipt.payouts.creator.launch_tokens += 1,
                6 => pending.receipt.payouts.creator.collateral += 1,
                7 => pending.receipt.payouts.adapter.launch_tokens += 1,
                _ => pending.receipt.payouts.adapter.collateral += 1,
            }
            pending.receipt_hash = pending.receipt.hash();
            let mut input = finalize(&response, receiver, &pending.receipt);
            input.pre_state = borsh::to_vec(&pending).unwrap().into();
            assert_eq!(
                crate::handle(&input).err(),
                Some(if case == 1 {
                    Error::Unauthorized
                } else {
                    Error::InvalidState
                }),
                "case {case}"
            );
        }
        for from in [
            None,
            state.graduation_adapter,
            Some(Actor::new(state.factory_owner, id(99))),
        ] {
            let mut input = finalize(&response, receiver, &receipt);
            input.from = from;
            assert_eq!(crate::handle(&input).err(), Some(Error::Unauthorized));
        }
    }

    #[test]
    fn zero_balances_complete_once_and_manual_cannot_bypass_saved_adapter() {
        for weighted in [false, true] {
            let (mut state, receiver) = fixture(weighted);
            state.remaining_inventory = 0;
            state.recorded_collateral = 0;
            state.accrued_fees = 0;
            let response = apply(
                state.clone(),
                receiver,
                (
                    SettlementRoute::Graduation,
                    id(50),
                    state.graduation_adapter,
                ),
                id(43),
                [0, 0, 0],
                state.close_time_bound().unwrap(),
            )
            .unwrap();
            assert!(token_calls(&response).is_empty());
            let receipt = applied(&response);
            let done = crate::handle(&finalize(&response, receiver, &receipt)).unwrap();
            let state = PoolAccount::decode(done.post_state.as_ref().unwrap()).unwrap();
            assert_eq!(state.settlement_progress, SettlementProgress::Graduated);
            assert_eq!(
                plan(&state, SettlementRoute::Graduation).err(),
                Some(Error::InvalidState)
            );
            assert_eq!(
                plan(&state, SettlementRoute::Manual).err(),
                Some(Error::InvalidTerms)
            );
            let mut input = ReceiveInput {
                receiver,
                from: Some(receipt.factory),
                is_authorized: false,
                pre_state: done.post_state.unwrap(),
                message: vec![],
            };
            for message in [
                Message::SettleFixedPrice {
                    creator: id(50),
                    now: 200,
                },
                Message::SettleWeighted { creator: id(50) },
                Message::Settle {
                    route: SettlementRoute::Manual,
                    creator: id(50),
                    adapter: None,
                },
            ] {
                if (weighted && matches!(message, Message::SettleFixedPrice { .. }))
                    || (!weighted && matches!(message, Message::SettleWeighted { .. }))
                {
                    continue;
                }
                input.message = borsh::to_vec(&message).unwrap();
                assert_eq!(crate::handle(&input).err(), Some(Error::InvalidTerms));
            }
        }
    }

    #[test]
    fn underfunding_and_wrong_adapter_fail_before_payouts() {
        let (state, receiver) = fixture(true);
        for balances in [[150, 900, 0], [1000, 789, 0]] {
            assert_eq!(
                apply(
                    state.clone(),
                    receiver,
                    (
                        SettlementRoute::Graduation,
                        id(50),
                        state.graduation_adapter
                    ),
                    id(43),
                    balances,
                    Some(200)
                )
                .err(),
                Some(Error::Underfunded)
            );
        }
        for adapter in [
            None,
            Some(Actor::new(id(99), id(71))),
            Some(Actor::new(id(70), id(99))),
        ] {
            assert_eq!(
                apply(
                    state.clone(),
                    receiver,
                    (SettlementRoute::Graduation, id(50), adapter),
                    id(43),
                    [1000, 900, 0],
                    Some(200)
                )
                .err(),
                Some(Error::InvalidTerms)
            );
        }
    }
    #[test]
    fn saved_adapter_blocks_every_manual_entry_before_queries_or_completed_noops() {
        for weighted in [false, true] {
            for progress in [
                SettlementProgress::Unselected,
                SettlementProgress::ManualComplete,
                SettlementProgress::Graduated,
            ] {
                for policy in [
                    GraduationPolicy::default(),
                    GraduationPolicy {
                        launch_bps: 10_000,
                        collateral_bps: 10_000,
                    },
                ] {
                    let (mut state, receiver) = fixture(weighted);
                    state.settlement_progress = progress;
                    state.graduation_policy = policy;
                    let legacy = if weighted {
                        Message::SettleWeighted { creator: id(50) }
                    } else {
                        Message::SettleFixedPrice {
                            creator: id(50),
                            now: 200,
                        }
                    };
                    for message in [
                        legacy,
                        Message::Settle {
                            route: SettlementRoute::Manual,
                            creator: id(50),
                            adapter: None,
                        },
                    ] {
                        let input = ReceiveInput {
                            receiver,
                            from: Some(Actor::new(state.factory_owner, state.factory_program_id)),
                            is_authorized: false,
                            pre_state: borsh::to_vec(&state).unwrap().into(),
                            message: borsh::to_vec(&message).unwrap(),
                        };
                        assert_eq!(crate::handle(&input).err(), Some(Error::InvalidTerms));
                    }
                    assert_eq!(
                        plan(&state, SettlementRoute::Manual).err(),
                        Some(Error::InvalidTerms)
                    );
                }
            }
        }
    }
}
