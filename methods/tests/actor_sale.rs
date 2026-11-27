use curve_core::events::{
    AllocationReleaseV1, ClosureReasonV1, EventSubjectV1, EventTimeBoundsV1, PayoutPurposeV1,
    PricingEngineV1, SaleEventKindV1, SaleEventV1, SettlementEffectV1, SettlementRouteV1,
    SettlementStepV1, TradeFormV1, WeightedPauseChanged,
};
use curve_core::{Message as CurveMessage, PoolAccount, TOKEN_PROGRAM_ACCOUNT_ID};
use lee::{
    AccountId, Actor, PrivateKey, PublicKey, PublicTransaction, V03State,
    program::Program,
    public_transaction::{Message, WitnessSet},
};
use lee_core::account::Account;
use lee_core::program::TransactionEvent;
use lez_compat_core::token::{self, TokenKind, TokenState};
use std::{borrow::Cow, collections::BTreeMap};

const CURVE: AccountId = AccountId::new([7; 32]);
const NAMESPACE: AccountId = AccountId::new([8; 32]);
const OWNER: AccountId = AccountId::new([9; 32]);
const LAUNCH: AccountId = AccountId::new([10; 32]);
const COLLATERAL: AccountId = AccountId::new([11; 32]);
const LBP: AccountId = AccountId::new([14; 32]);
const TREASURY: AccountId = AccountId::new([15; 32]);
const NEXT_TREASURY: AccountId = AccountId::new([16; 32]);

#[path = "actor_sale/allowlist.rs"]
mod allowlist;
#[path = "actor_sale/buys.rs"]
mod buys;
#[path = "actor_sale/controls.rs"]
mod controls;
#[path = "actor_sale/creation.rs"]
mod creation;
#[path = "actor_sale/curve_fixture.rs"]
mod curve_fixture;
#[path = "actor_sale/distribution.rs"]
mod distribution;
#[path = "actor_sale/event_fixture.rs"]
mod event_fixture;
#[path = "actor_sale/events.rs"]
mod events;
#[path = "actor_sale/fixed_factory.rs"]
mod fixed_factory;
#[path = "actor_sale/graduation.rs"]
mod graduation;
#[path = "actor_sale/graduation_fixture.rs"]
mod graduation_fixture;
#[path = "actor_sale/graduation_rollback.rs"]
mod graduation_rollback;
#[path = "actor_sale/lbp_fixture.rs"]
mod lbp_fixture;
#[path = "actor_sale/runtime.rs"]
mod runtime;
#[path = "actor_sale/sells.rs"]
mod sells;
#[path = "actor_sale/settlement.rs"]
mod settlement;
#[path = "actor_sale/weighted_clock.rs"]
mod weighted_clock;
#[path = "actor_sale/weighted_factory.rs"]
mod weighted_factory;

use curve_fixture::*;
use event_fixture::*;
use fixed_factory::*;
use graduation_fixture::*;
use lbp_fixture::*;
use runtime::*;
use weighted_factory::*;
