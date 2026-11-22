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
