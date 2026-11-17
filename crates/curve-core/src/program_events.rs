use crate::{EndingPolicy, PoolAccount, TimeBounds};
use lee_core::{
    account::{AccountId, Actor},
    program::ProgramEvent,
};
use sale_policy::events::{
    ClosureReasonV1, EventPoliciesV1, EventSubjectV1, EventTimeBoundsV1, PricingEngineV1,
    SaleEventKindV1, SaleEventV1,
};

pub fn sale_event(event: SaleEventV1) -> ProgramEvent {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let digest = Impl::hash_bytes(SaleEventV1::SELECTOR_NAME.as_bytes());
    ProgramEvent {
        selector: digest.as_bytes()[..8].try_into().unwrap(),
        data: borsh::to_vec(&event).unwrap(),
    }
}

impl PoolAccount {
    pub(crate) fn event(&self, receiver: Actor, kind: SaleEventKindV1) -> ProgramEvent {
        sale_event(SaleEventV1::new(
            *self.namespace.value(),
            PricingEngineV1::Curve,
            EventSubjectV1::Pool {
                pool: *receiver.account_id.value(),
            },
            kind,
        ))
    }

    pub(crate) fn event_policies(&self) -> EventPoliciesV1 {
        EventPoliciesV1 {
            ending: self.pool.ending_policy,
            pause: self.trade_control.policy(),
            sell_fee_bps: self.pool.sell_fee_bps,
        }
    }

    pub(crate) fn depletion_reason(&self) -> Option<ClosureReasonV1> {
        if !matches!(
            self.pool.ending_policy,
            EndingPolicy::Depletion | EndingPolicy::Either
        ) {
            return None;
        }
        let definition = match self.pool.close_on_depletion {
            Some(pool::TokenSide::Token0) if self.pool.real_reserve0 == 0 => {
                self.token0_definition_id
            }
            Some(pool::TokenSide::Token1) if self.pool.real_reserve1 == 0 => {
                self.token1_definition_id
            }
            _ => return None,
        };
        Some(ClosureReasonV1::Depleted {
            definition: *definition.value(),
        })
    }

    // Call before withdrawal clears reserves, with the response's enforced bounds.
    pub(crate) fn observe_closure(
        &mut self,
        receiver: Actor,
        reason: ClosureReasonV1,
        bounds: TimeBounds,
    ) -> Option<ProgramEvent> {
        if self.closure_observed {
            return None;
        }
        self.closure_observed = true;
        Some(self.event(
            receiver,
            SaleEventKindV1::Closed {
                reason,
                inclusion_bounds: EventTimeBoundsV1 {
                    at_least: bounds.at_least,
                    before: bounds.before,
                },
            },
        ))
    }

    pub(crate) fn fees_event(
        &self,
        receiver: Actor,
        treasury: AccountId,
        amount: u128,
    ) -> Option<ProgramEvent> {
        (amount != 0).then(|| {
            self.event(
                receiver,
                SaleEventKindV1::FeesCollected {
                    definition: *self.token1_definition_id.value(),
                    treasury: *treasury.value(),
                    liability_discharged: amount,
                    tokens_transferred: transferred_amount(receiver, treasury, amount),
                },
            )
        })
    }
}

pub(crate) fn transferred_amount(receiver: Actor, recipient: AccountId, amount: u128) -> u128 {
    if recipient == receiver.account_id {
        0
    } else {
        amount
    }
}
