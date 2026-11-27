#[path = "support/creation.rs"]
mod creation_fixture;
#[path = "support/lbp_pause_forwarding.rs"]
mod lbp_pause_forwarding;

use creation_fixture::Creation;
use factory_core::events::*;
use factory_core::{
    CreatorAllocationRelease, Error, FixedPrice, FixedPriceInputs, Message, SaleStart,
    compute_factory_pda, handle,
};
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response},
};
#[path = "fixed_price/allocation.rs"]
mod allocation;
#[path = "fixed_price/controls.rs"]
mod controls;
#[path = "fixed_price/creation.rs"]
mod creation;
#[path = "fixed_price/fixture.rs"]
mod fixture;
#[path = "fixed_price/settlement.rs"]
mod settlement;
#[path = "fixed_price/setup.rs"]
mod setup;
#[path = "fixed_price/setup_policies.rs"]
mod setup_policies;

use fixture::*;
