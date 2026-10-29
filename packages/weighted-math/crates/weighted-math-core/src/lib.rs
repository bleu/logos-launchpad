//! Unaudited pricing for weighted-sale pools, tested against mpmath fixtures.
//!
//! This `no_std` arithmetic package has no dependencies. Raw functions retain
//! documented panics. Checked quotes validate the domain and arithmetic loss.
#![no_std]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod checked;
mod checked_uint;
mod fixed;
mod interpolate;
mod pow;
mod weighted;
pub(crate) mod wide;

pub use checked::{QuoteError, try_calc_in_given_out, try_calc_out_given_in};
pub use fixed::{Fixed, ONE, Repr, SCALE};
pub use interpolate::interpolate_weight;
pub use pow::{exp, expm1, ln, pow, pow_down, pow_up};
pub use weighted::{calc_in_given_out, calc_out_given_in};
