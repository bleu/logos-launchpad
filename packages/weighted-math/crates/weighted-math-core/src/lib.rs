//! Unaudited pricing for weighted-sale pools, tested against mpmath fixtures.
//!
//! This `no_std` arithmetic kernel has no dependencies. Domain violations
//! use documented panics instead of typed errors to keep that boundary.
#![no_std]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod fixed;
mod pow;
mod weighted;
pub(crate) mod wide;

pub use fixed::{Fixed, ONE, Repr, SCALE};
pub use pow::{exp, expm1, ln, pow, pow_down, pow_up};
pub use weighted::{calc_in_given_out, calc_out_given_in};
