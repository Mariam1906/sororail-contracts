// Unsafe code is denied, not forbidden: soroban-sdk's `#[contractimpl]` emits
// an `#[allow(unsafe_code)]` constructor-registration shim whenever its
// `testutils` feature is enabled (unit tests, and any test crate depending on
// this one), and `forbid` cannot be overridden (E0453). `deny` still rejects
// any hand-written `unsafe`.
#![deny(unsafe_code)]
#![no_std]
#![doc = include_str!("../README.md")]

#[cfg(test)]
extern crate std;

soroban_sdk::contractmeta!(key = "version", val = env!("CARGO_PKG_VERSION"));
soroban_sdk::contractmeta!(
    key = "source_repo",
    val = "https://github.com/Sororail/sororail-contracts"
);
soroban_sdk::contractmeta!(key = "git_commit", val = env!("SORORAIL_GIT_COMMIT"));

pub mod contract;
pub mod errors;
pub mod events;
pub mod storage;
pub mod types;

pub use contract::{StreamContract, StreamContractClient};
pub use errors::Error;
pub use types::Stream;

#[cfg(test)]
mod test;
