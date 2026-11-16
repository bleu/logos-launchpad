use crate::{Error, PoolAccount, PoolMode, SettlementProgress};
use curve_core::events::{
    ClosureReasonV1, EventPoliciesV1, EventSubjectV1, EventTimeBoundsV1, PricingEngineV1,
    SaleEventKindV1, SaleEventV1, SettlementRouteV1,
};
use lee_core::{
    account::{AccountId, Actor},
    program::ProgramEvent,
};
use sale_policy::EndingDecision;

impl PoolAccount {
    pub(super) fn event(&self, receiver: Actor, kind: SaleEventKindV1) -> ProgramEvent {
        curve_core::sale_event(SaleEventV1::new(
            *self.namespace.value(),
            match self.mode {
                PoolMode::FixedPrice => PricingEngineV1::FixedPrice,
                PoolMode::Weighted { .. } => PricingEngineV1::Weighted,
            },
            EventSubjectV1::Pool {
                pool: *receiver.account_id.value(),
            },
            kind,
        ))
    }

    pub(super) fn event_policies(&self) -> EventPoliciesV1 {
        EventPoliciesV1 {
            ending: self.ending_policy,
            pause: self.trade_control.policy(),
        }
    }

    pub(super) fn observe_closure(
        &mut self,
        receiver: Actor,
    ) -> Result<Option<ProgramEvent>, Error> {
        if self.closure_observed {
            return Ok(None);
        }
        self.validate_policies()?;
        let decision = self
            .ending_policy
            .settlement(self.end_timestamp, self.remaining_inventory)
            .map_err(crate::ending_error)?;
        let bounds = decision.inclusion_bounds();
        let reason = match decision {
            EndingDecision::Manual => ClosureReasonV1::Manual,
            EndingDecision::Depleted => ClosureReasonV1::Depleted {
                definition: *self.token_definition_id.value(),
            },
            EndingDecision::ScheduledEnd { timestamp } => {
                ClosureReasonV1::ScheduledEnd { timestamp }
            }
        };
        self.closure_observed = true;
        Ok(Some(self.event(
            receiver,
            SaleEventKindV1::Closed {
                reason,
                inclusion_bounds: EventTimeBoundsV1 {
                    at_least: bounds.at_least,
                    before: bounds.before,
                },
            },
        )))
    }

    pub(super) fn fee_event(
        &self,
        receiver: Actor,
        treasury: AccountId,
        amount: u128,
    ) -> Option<ProgramEvent> {
        (amount != 0).then(|| {
            self.event(
                receiver,
                SaleEventKindV1::FeesCollected {
                    definition: *self.collateral_definition_id.value(),
                    treasury: *treasury.value(),
                    liability_discharged: amount,
                    tokens_transferred: transferred(receiver, treasury, amount),
                },
            )
        })
    }
}

pub(super) fn transferred(receiver: Actor, recipient: AccountId, amount: u128) -> u128 {
    if receiver.account_id == recipient {
        0
    } else {
        amount
    }
}

pub(super) fn settlement_route(progress: SettlementProgress) -> SettlementRouteV1 {
    match progress {
        SettlementProgress::Graduated => SettlementRouteV1::Graduation,
        _ => SettlementRouteV1::Manual,
    }
}

pub(super) fn custody_events(
    mut response: lee_core::program::Response,
    state: &PoolAccount,
    receiver: Actor,
    creator: AccountId,
    adapter: Option<Actor>,
    treasury: AccountId,
    plan: &crate::lbp_settlement::CustodyPlan,
) -> lee_core::program::Response {
    use curve_core::events::{PayoutPurposeV1, SettlementEffectV1, SettlementStepV1};
    if let Some(event) = state.fee_event(receiver, treasury, plan.fees) {
        response = response.event(event);
    }
    let route = Some(settlement_route(plan.progress));
    if plan.burn != 0 {
        response = response.event(state.event(
            receiver,
            SaleEventKindV1::Settlement {
                step: SettlementStepV1::UnsoldBurned,
                route: None,
                effects: vec![SettlementEffectV1::Burn {
                    definition: *state.token_definition_id.value(),
                    amount: plan.burn,
                }],
            },
        ));
    }
    for (amount, recipient, purpose, step) in [
        (
            plan.seed,
            creator,
            PayoutPurposeV1::CreatorSeed,
            SettlementStepV1::SeedReturned,
        ),
        (
            plan.amounts.collateral,
            adapter.map_or(creator, |a| a.account_id),
            PayoutPurposeV1::CreatorProceeds,
            SettlementStepV1::Payout,
        ),
    ] {
        if amount != 0 {
            response = response.event(state.event(
                receiver,
                SaleEventKindV1::Settlement {
                    step,
                    route: if step == SettlementStepV1::SeedReturned {
                        None
                    } else {
                        route
                    },
                    effects: vec![SettlementEffectV1::Payout {
                        definition: *state.collateral_definition_id.value(),
                        recipient: *recipient.value(),
                        purpose,
                        liability_discharged: amount,
                        tokens_transferred: transferred(receiver, recipient, amount),
                    }],
                },
            ));
        }
    }
    response
}
