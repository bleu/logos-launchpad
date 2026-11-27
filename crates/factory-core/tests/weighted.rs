#[path = "support/creation.rs"]
mod creation_fixture;
#[path = "support/lbp_pause_forwarding.rs"]
mod lbp_pause_forwarding;

use creation_fixture::Creation;
use factory_core::events::*;
use factory_core::{
    CreatorAllocationRelease, Error, LbpCreationStage, LbpLaunchState, LbpSetupMode, Message,
    WeightedInputs, handle,
};
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response, StateReply},
};
use lez_compat_core::token::{TokenKind, TokenState};

#[path = "weighted/allocation.rs"]
mod allocation;
#[path = "weighted/controls.rs"]
mod controls;
#[path = "weighted/fixture.rs"]
mod fixture;
#[path = "weighted/settlement.rs"]
mod settlement;
#[path = "weighted/setup.rs"]
mod setup;

use fixture::*;
