#[path = "support/creation.rs"]
mod creation_fixture;

use creation_fixture::Creation;
use factory_core::events::*;
use factory_core::*;
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response, StateReply},
};
use lez_compat_core::{
    authority,
    token::{self, TokenKind, TokenState},
};
#[path = "curve/allocation.rs"]
mod allocation;
#[path = "curve/controls.rs"]
mod controls;
#[path = "curve/creation.rs"]
mod creation;
#[path = "curve/creator_authority.rs"]
mod creator_authority;
#[path = "curve/fixture.rs"]
mod fixture;
#[path = "curve/graduation.rs"]
mod graduation;
#[path = "curve/settlement.rs"]
mod settlement;

use fixture::*;
