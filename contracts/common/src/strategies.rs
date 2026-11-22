//! Proptest strategies shared across SoroRail contract test suites.
//!
//! Gated behind the `testutils` feature so it is never compiled into
//! production or WASM builds. Each contract's `[dev-dependencies]` section
//! must include:
//!
//! ```toml
//! sororail-common = { workspace = true, features = ["testutils"] }
//! ```
//!
//! # What is here
//!
//! Ready-made [`proptest`] strategies for the primitive types that appear in
//! every SoroRail contract test:
//!
//! * [`arb_bps`] — a valid basis-points value in `[0, 10_000]`
//! * [`arb_amount`] — a non-negative `i128` amount in a Soroban-safe range
//! * [`arb_positive_amount`] — same, but strictly positive (`>= 1`)
//!
//! These strategies free individual test suites from repeating
//! `0u32..=10_000u32` or hardcoding the balance ceiling, and make it easy
//! to add further strategies as the shared type surface grows.

// Strategy combinators use standard integer arithmetic; the checked-math rule
// is for contract code, not test harness scaffolding.
#![allow(clippy::arithmetic_side_effects)]

use proptest::prelude::*;

use crate::math::MAX_BPS;

/// Any valid basis-points value: `0 ..= 10_000`.
pub fn arb_bps() -> impl Strategy<Value = u32> {
    0u32..=MAX_BPS
}

/// Any non-negative `i128` amount in `[0, 1_000_000_000_000]`.
///
/// The upper bound fits safely in `i128` and is large enough to exercise
/// the `i128`-widened intermediate product inside [`crate::math::mul_bps`]
/// without overflowing it.
pub fn arb_amount() -> impl Strategy<Value = i128> {
    0i128..=1_000_000_000_000i128
}

/// Any strictly positive `i128` amount in `[1, 1_000_000_000_000]`.
pub fn arb_positive_amount() -> impl Strategy<Value = i128> {
    1i128..=1_000_000_000_000i128
}
