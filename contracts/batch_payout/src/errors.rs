//! Batch payout error surface.
//!
//! Errors are shared org-wide -- see `sororail_common::errors`, where
//! batch_payout owns the numeric range **100–119**. This module re-exports the
//! enum and documents which variants this contract can return:
//!
//! | Variant | Raised by |
//! |---|---|
//! | [`Error::BatchEmpty`] | a batch with no recipients |
//! | [`Error::BatchTooLarge`] | more recipients than [`crate::types::MAX_RECIPIENTS`] |
//! | [`Error::InvalidAmount`] | any non-positive amount in the batch |
//! | [`Error::IdenticalParties`] | a recipient that is this contract's own address |
//! | [`Error::Overflow`] | the batch total exceeding `i128` |
//! | [`Error::InvalidBasisPoints`] | `execute_weighted` shares not summing to exactly `10_000` |
//!
//! # What is absent, and why
//!
//! Number 102 was `BatchDuplicateRecipient`, removed before release: detecting
//! duplicates on-chain costs a quadratic scan, and paying one address twice in
//! a batch is legitimate. That check belongs in the client's CSV import.
//! A recipient equal to the funder is also permitted (including repeated
//! funder entries): it is a no-op token transfer, but the receipt still counts
//! the submitted lines and their amounts. Only an address equal to this
//! contract itself triggers `IdenticalParties`.
//!
//! `AlreadyInitialized` and `NotInitialized` do not appear: this contract is
//! stateless, with no `init` and no `storage.rs`.
//!
//! `Unauthorized` does not appear: `execute` and `execute_equal` act on the
//! single `funder` passed in and call `require_auth` on it directly, so a wrong
//! signer fails at authorization rather than at a membership check.
//!
//! The following shared variants are never returned by this contract either:
//!
//! - `InsufficientBalance` -- the contract never holds funds. A funder who
//!   cannot cover the batch makes a token `transfer` fail with the token's own
//!   error, which reverts the whole batch.
//! - `InvalidState`, `InvalidTimeRange`, `InvalidDuration`,
//!   `DeadlineNotReached`, `DeadlinePassed` -- there is no lifecycle and no
//!   time dependence; every call is a one-shot.
//! - `Underflow`, `DivisionByZero` -- arithmetic is summing positive amounts,
//!   one multiplication in `execute_equal`, a division by a non-zero recipient
//!   count in `execute_equal_split` / `preview_equal`, and `mul_bps` plus a
//!   subtraction that cannot go negative (shares sum to `10_000`) in
//!   `execute_weighted`.

pub use sororail_common::Error;
