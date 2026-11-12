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
    pub amounts: SettlementAmounts,
    pub seed_payment: u128,
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

impl PoolAccount {
    pub fn outstanding_seed(&self) -> Result<u128, Error> {
        match self.mode {
            PoolMode::FixedPrice if !self.seed_returned => Err(Error::InvalidState),
            PoolMode::FixedPrice => Ok(0),
            PoolMode::Weighted {
                collateral_seed, ..
            } => Ok(if self.seed_returned {
                0
            } else {
                collateral_seed
            }),
        }
    }

    pub fn buyer_proceeds(&self) -> Result<u128, Error> {
        self.recorded_collateral
            .checked_sub(self.outstanding_seed()?)
            .ok_or(Error::InvalidState)
    }
}

pub(super) struct CustodyPlan {
    pub fees: u128,
    pub burn: u128,
    pub seed: u128,
    pub amounts: SettlementAmounts,
    pub progress: SettlementProgress,
}

pub(super) fn plan(state: &PoolAccount, route: SettlementRoute) -> Result<CustodyPlan, Error> {
    state
        .settlement_progress
        .ensure_available(route)
        .map_err(|_| Error::InvalidState)?;
    let seed = state.outstanding_seed()?;
    let collateral = state.buyer_proceeds()?;
    let amounts = SettlementAmounts {
        launch_tokens: 0,
        collateral,
    };
    let mut progress = state.settlement_progress;
    match route {
        SettlementRoute::Manual => {
            progress
                .record_manual_payout(amounts)
                .map_err(|_| Error::InvalidState)?;
            progress.finish_manual().map_err(|_| Error::InvalidState)?;
        }
        SettlementRoute::Graduation => progress
            .finish_graduation()
            .map_err(|_| Error::InvalidState)?,
    }
    Ok(CustodyPlan {
        fees: state.accrued_fees,
        burn: state.remaining_inventory,
        seed,
        amounts,
        progress,
    })
}

pub(super) fn validate_route(
    state: &PoolAccount,
    route: SettlementRoute,
    creator: AccountId,
    adapter: Option<Actor>,
) -> Result<(), Error> {
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
    if balances[0] < liability || balances[1] < plan.burn {
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
        amounts: plan.amounts,
        seed_payment: plan.seed,
        progress: plan.progress,
    };
    let closure_event = state.observe_closure(receiver)?;
    state.recorded_collateral = 0;
    state.accrued_fees = 0;
    state.seed_returned = true;
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
        if state.recorded_collateral != 0
            || state.accrued_fees != 0
            || !state.seed_returned
            || pending.receipt.amounts.launch_tokens != 0
            || pending.expiry != state.close_time_bound()?
        {
            return Err(Error::InvalidState);
        }
        let mut expected_progress = state.settlement_progress;
        match pending.receipt.route {
            SettlementRoute::Manual => {
                expected_progress
                    .record_manual_payout(pending.receipt.amounts)
                    .map_err(|_| Error::InvalidState)?;
                expected_progress
                    .finish_manual()
                    .map_err(|_| Error::InvalidState)?;
            }
            SettlementRoute::Graduation => expected_progress
                .finish_graduation()
                .map_err(|_| Error::InvalidState)?,
        }
        if pending.receipt.progress != expected_progress {
            return Err(Error::InvalidState);
        }
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
            graduation_adapter: Some(Actor::new(id(70), id(71))),
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
            seed_returned: !weighted,
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
    fn both_routes_split_seed_and_proceeds_without_spending_surplus() {
        for weighted in [false, true] {
            for route in [SettlementRoute::Manual, SettlementRoute::Graduation] {
                let (state, receiver) = fixture(weighted);
                let adapter = if route == SettlementRoute::Graduation {
                    state.graduation_adapter
                } else {
                    None
                };
                let response = apply(
                    state.clone(),
                    receiver,
                    (route, id(50), adapter),
                    id(43),
                    [1000, 900, 0],
                    Some(200),
                )
                .unwrap();
                let receipt = applied(&response);
                assert_eq!(
                    receipt.amounts,
                    SettlementAmounts {
                        launch_tokens: 0,
                        collateral: 49
                    }
                );
                assert_eq!(receipt.seed_payment, if weighted { 100 } else { 0 });
                assert_eq!(
                    receipt.progress,
                    if route == SettlementRoute::Manual {
                        SettlementProgress::ManualComplete
                    } else {
                        SettlementProgress::Graduated
                    }
                );
                let mut expected = vec![
                    token::Message::Transfer {
                        definition: id(42),
                        recipient: id(43),
                        amount: 2,
                    },
                    token::Message::Burn {
                        definition: id(41),
                        amount: 790,
                    },
                ];
                if let Some(adapter) = adapter {
                    if weighted {
                        expected.push(token::Message::Transfer {
                            definition: id(42),
                            recipient: id(50),
                            amount: 100,
                        });
                    }
                    expected.push(token::Message::Transfer {
                        definition: id(42),
                        recipient: adapter.account_id,
                        amount: 49,
                    });
                } else {
                    expected.push(token::Message::Transfer {
                        definition: id(42),
                        recipient: id(50),
                        amount: state.recorded_collateral,
                    });
                }
                assert_eq!(token_calls(&response), expected);
                let pending: AwaitFactoryFinalize =
                    borsh::from_slice(response.post_state.as_ref().unwrap()).unwrap();
                assert_eq!(
                    pending.state.settlement_progress,
                    SettlementProgress::Unselected
                );
                assert_eq!(pending.state.remaining_inventory, 790);
                assert!(pending.state.seed_returned);
                let done = crate::handle(&finalize(&response, receiver, &receipt)).unwrap();
                let final_state = PoolAccount::decode(done.post_state.as_ref().unwrap()).unwrap();
                assert_eq!(final_state.settlement_progress, receipt.progress);
                assert_eq!(final_state.recorded_collateral, 0);
                assert_eq!(done.validity.timestamps.start(), Some(200_000));
                assert!(
                    matches!(borsh::from_slice::<SettlementReply>(&done.sync_messages[0].message).unwrap(), SettlementReply::Completed { receipt_hash, progress } if receipt_hash == receipt.hash() && progress == receipt.progress)
                );
                assert!(!response.events.is_empty());
            }
        }
    }

    #[test]
    fn guard_rejects_public_reentry_wrong_actor_and_wrong_hash() {
        let (state, receiver) = fixture(true);
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
        let mut input = finalize(&response, receiver, &receipt);
        for from in [
            None,
            Some(Actor::new(state.factory_owner, id(99))),
            state.graduation_adapter,
        ] {
            input.from = from;
            assert_eq!(crate::handle(&input).err(), Some(Error::Unauthorized));
        }
        input.from = Some(receipt.factory);
        for message in [
            Message::ReadState,
            Message::CollectFees,
            Message::SetPaused { paused: false },
            Message::Settle {
                route: SettlementRoute::Manual,
                creator: id(50),
                adapter: None,
            },
        ] {
            input.message = borsh::to_vec(&message).unwrap();
            assert_eq!(crate::handle(&input).err(), Some(Error::InvalidMessage));
        }
        input.message = borsh::to_vec(&Message::FinalizeSettlement {
            receipt_hash: [0; 32],
        })
        .unwrap();
        assert_eq!(crate::handle(&input).err(), Some(Error::InvalidState));
    }

    #[test]
    fn finalize_rejects_receipt_binding_and_progress_forgery() {
        let (state, receiver) = fixture(false);
        let response = apply(
            state,
            receiver,
            (SettlementRoute::Manual, id(50), None),
            id(43),
            [1000, 900, 0],
            Some(200),
        )
        .unwrap();
        let receipt = applied(&response);
        for case in 0..4 {
            let mut pending: AwaitFactoryFinalize =
                borsh::from_slice(response.post_state.as_ref().unwrap()).unwrap();
            match case {
                0 => pending.receipt.pool = Actor::new(id(99), receiver.program_account_id),
                1 => pending.receipt.factory = Actor::new(id(99), id(8)),
                2 => pending.receipt.progress = SettlementProgress::Graduated,
                _ => pending.receipt.amounts.launch_tokens = 1,
            }
            pending.receipt_hash = pending.receipt.hash();
            let mut input = finalize(&response, receiver, &receipt);
            input.message = borsh::to_vec(&Message::FinalizeSettlement {
                receipt_hash: pending.receipt_hash,
            })
            .unwrap();
            input.pre_state = borsh::to_vec(&pending).unwrap().into();
            assert_eq!(crate::handle(&input).err(), Some(Error::InvalidState));
        }
    }

    #[test]
    fn zero_proceeds_complete_the_chosen_route_and_block_both_routes() {
        for weighted in [false, true] {
            for route in [SettlementRoute::Manual, SettlementRoute::Graduation] {
                let (mut state, receiver) = fixture(weighted);
                state.accrued_fees = 0;
                state.recorded_collateral = if weighted { 100 } else { 0 };
                let adapter = if route == SettlementRoute::Graduation {
                    state.graduation_adapter
                } else {
                    None
                };
                let response = apply(
                    state,
                    receiver,
                    (route, id(50), adapter),
                    id(43),
                    [1000, 900, 0],
                    Some(200),
                )
                .unwrap();
                let receipt = applied(&response);
                assert_eq!(
                    receipt.progress,
                    if route == SettlementRoute::Manual {
                        SettlementProgress::ManualComplete
                    } else {
                        SettlementProgress::Graduated
                    }
                );
                assert_eq!(receipt.amounts.collateral, 0);
                assert_eq!(receipt.seed_payment, if weighted { 100 } else { 0 });
                let mut expected = vec![token::Message::Burn {
                    definition: id(41),
                    amount: 790,
                }];
                if weighted {
                    expected.push(token::Message::Transfer {
                        definition: id(42),
                        recipient: id(50),
                        amount: 100,
                    });
                }
                assert_eq!(token_calls(&response), expected);
                let done = crate::handle(&finalize(&response, receiver, &receipt)).unwrap();
                let state = PoolAccount::decode(done.post_state.as_ref().unwrap()).unwrap();
                assert_eq!(state.settlement_progress, receipt.progress);
                assert!(state.seed_returned);
                for retry in [SettlementRoute::Manual, SettlementRoute::Graduation] {
                    assert_eq!(plan(&state, retry).err(), Some(Error::InvalidState));
                }
            }
        }
    }

    #[test]
    fn seed_shortfall_fails_despite_surplus() {
        let (mut state, receiver) = fixture(true);
        state.recorded_collateral = 99;
        assert_eq!(
            apply(
                state.clone(),
                receiver,
                (SettlementRoute::Manual, id(50), None),
                id(43),
                [1000, 900, 0],
                Some(200)
            )
            .err(),
            Some(Error::InvalidState)
        );
    }

    #[test]
    fn new_settle_authenticates_factory_and_binds_immutable_adapter() {
        let (state, receiver) = fixture(false);
        let mut input = ReceiveInput {
            receiver,
            from: None,
            is_authorized: true,
            pre_state: borsh::to_vec(&state).unwrap().into(),
            message: borsh::to_vec(&Message::Settle {
                route: SettlementRoute::Graduation,
                creator: id(50),
                adapter: state.graduation_adapter,
            })
            .unwrap(),
        };
        assert_eq!(crate::handle(&input).err(), Some(Error::Unauthorized));
        input.from = Some(Actor::new(state.factory_owner, state.factory_program_id));
        for adapter in [
            None,
            Some(Actor::new(id(70), id(99))),
            Some(Actor::new(id(99), id(71))),
        ] {
            input.message = borsh::to_vec(&Message::Settle {
                route: SettlementRoute::Graduation,
                creator: id(50),
                adapter,
            })
            .unwrap();
            assert_eq!(crate::handle(&input).err(), Some(Error::InvalidTerms));
        }
        input.message = borsh::to_vec(&Message::Settle {
            route: SettlementRoute::Manual,
            creator: id(50),
            adapter: state.graduation_adapter,
        })
        .unwrap();
        assert_eq!(crate::handle(&input).err(), Some(Error::InvalidTerms));
    }

    #[test]
    fn receipt_hash_is_sha256_of_exact_borsh_bytes() {
        use risc0_zkvm::sha::{Impl, Sha256 as _};
        let (state, receiver) = fixture(false);
        let response = apply(
            state,
            receiver,
            (SettlementRoute::Manual, id(50), None),
            id(43),
            [1000, 900, 0],
            Some(200),
        )
        .unwrap();
        let receipt = applied(&response);
        assert_eq!(
            receipt.hash().as_slice(),
            Impl::hash_bytes(&borsh::to_vec(&receipt).unwrap()).as_bytes()
        );
        let mut changed = receipt.clone();
        changed.seed_payment += 1;
        assert_ne!(receipt.hash(), changed.hash());
    }

    #[test]
    fn graduated_legacy_retry_remains_authenticated_and_scheduled() {
        let (mut state, receiver) = fixture(true);
        state.remaining_inventory = 0;
        state.recorded_collateral = 0;
        state.accrued_fees = 0;
        state.seed_returned = true;
        state.settlement_progress = SettlementProgress::Graduated;
        let mut input = ReceiveInput {
            receiver,
            from: Some(Actor::new(state.factory_owner, state.factory_program_id)),
            is_authorized: false,
            pre_state: borsh::to_vec(&state).unwrap().into(),
            message: borsh::to_vec(&Message::SettleWeighted { creator: id(50) }).unwrap(),
        };
        let response = crate::handle(&input).unwrap();
        assert!(response.sync_messages.is_empty());
        assert_eq!(response.validity.timestamps.start(), Some(200_000));
        input.from = state.graduation_adapter;
        assert_eq!(crate::handle(&input).err(), Some(Error::Unauthorized));
    }
}
