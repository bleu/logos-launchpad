//! Versioned payloads for committed pool and launch changes.
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{controls::PausePolicy, terms::EndingPolicy};

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct SaleEventV1 {
    pub version: u8,
    pub namespace: [u8; 32],
    pub engine: PricingEngineV1,
    pub subject: EventSubjectV1,
    pub kind: SaleEventKindV1,
}

impl SaleEventV1 {
    pub const SELECTOR_NAME: &'static str = "launchpad::SaleEventV1";
    pub const VERSION: u8 = 1;

    pub fn new(
        namespace: [u8; 32],
        engine: PricingEngineV1,
        subject: EventSubjectV1,
        kind: SaleEventKindV1,
    ) -> Self {
        Self {
            version: Self::VERSION,
            namespace,
            engine,
            subject,
            kind,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum PricingEngineV1 {
    Curve,
    FixedPrice,
    Weighted,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum EventSubjectV1 {
    Launch { factory: [u8; 32], pool: [u8; 32] },
    Pool { pool: [u8; 32] },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub struct EventPoliciesV1 {
    pub ending: EndingPolicy,
    pub pause: PausePolicy,
}

/// Unix-second inclusion bounds: inclusive lower bound and exclusive upper bound.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub struct EventTimeBoundsV1 {
    pub at_least: Option<u64>,
    pub before: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub enum SaleEventKindV1 {
    Created {
        creator: [u8; 32],
        creator_role: [u8; 32],
        token_definition: [u8; 32],
        collateral_definition: [u8; 32],
        sale_inventory: u128,
        creator_allocation: u128,
        curve_reserve: u128,
        collateral_seed: u128,
        start_timestamp: Option<u64>,
        end_timestamp: Option<u64>,
        policies: EventPoliciesV1,
    },
    Prepared {
        owner: [u8; 32],
        token0_definition: [u8; 32],
        token1_definition: [u8; 32],
        reserve0: u128,
        reserve1: u128,
        policies: EventPoliciesV1,
    },
    Activated {
        reserve0: u128,
        reserve1: u128,
        collateral_seed: u128,
    },
    Trade {
        trader: [u8; 32],
        form: TradeFormV1,
        token_in: [u8; 32],
        token_out: [u8; 32],
        amount_in: u128,
        pricing_input: u128,
        gross_output: u128,
        amount_out: u128,
        fee_definition: [u8; 32],
        fee_amount: u128,
        fee_bps: u16,
        pricing_time: Option<u64>,
    },
    PauseChanged {
        paused: bool,
    },
    FeesCollected {
        definition: [u8; 32],
        treasury: [u8; 32],
        liability_discharged: u128,
        tokens_transferred: u128,
    },
    AllocationReleased {
        definition: [u8; 32],
        recipient: [u8; 32],
        amount: u128,
        release: AllocationReleaseV1,
    },
    /// First stored pool closure; a factory allocation read is not an observation.
    Closed {
        reason: ClosureReasonV1,
        inclusion_bounds: EventTimeBoundsV1,
    },
    Settlement {
        step: SettlementStepV1,
        route: Option<SettlementRouteV1>,
        effects: Vec<SettlementEffectV1>,
    },
    /// The factory has accepted the configured adapter's authenticated callback.
    Graduated {
        adapter_program: [u8; 32],
        adapter_custody: [u8; 32],
        launch_tokens: u128,
        collateral: u128,
    },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum TradeFormV1 {
    ExactInput,
    ExactOutput,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum AllocationReleaseV1 {
    AfterSetup,
    AfterEnd,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum ClosureReasonV1 {
    Manual,
    Depleted { definition: [u8; 32] },
    ScheduledEnd { timestamp: u64 },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum SettlementStepV1 {
    Prepared,
    ReservesWithdrawn,
    UnsoldBurned,
    SeedReturned,
    Payout,
    Completed,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum SettlementRouteV1 {
    Manual,
    Graduation,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum SettlementEffectV1 {
    Burn {
        definition: [u8; 32],
        amount: u128,
    },
    Payout {
        definition: [u8; 32],
        recipient: [u8; 32],
        purpose: PayoutPurposeV1,
        liability_discharged: u128,
        tokens_transferred: u128,
    },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum PayoutPurposeV1 {
    Reserve,
    CreatorProceeds,
    CreatorSeed,
    CurveReserve,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub struct WeightedPauseChanged {
    pub paused: bool,
}

impl WeightedPauseChanged {
    pub const SELECTOR_NAME: &'static str = "lbp::WeightedPauseChanged";
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub struct WeightedCheckpoint {
    pub time: u64,
    pub token_weight: u128,
}

impl WeightedCheckpoint {
    pub const SELECTOR_NAME: &'static str = "lbp::WeightedCheckpoint";
}
