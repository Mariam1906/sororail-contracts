//! Batch payout events.
//!
//! Follows the org convention in `sororail_common::events`.

use soroban_sdk::{contractevent, Address, BytesN};

/// A batch completed. Emitted once per batch, not once per recipient, by
/// `execute`, `execute_equal`, `execute_equal_split` and `execute_weighted`.
///
/// Indexers must correlate the token contract's `transfer` events in the same
/// transaction to recover each recipient and amount. This event intentionally
/// does not duplicate those transfers, keeping large payrolls compact.
#[contractevent(topics = ["batch_payout", "executed"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Executed {
    #[topic]
    pub funder: Address,
    pub token: Address,
    pub count: u32,
    pub total: i128,
    /// The caller's per-batch reference (#428), or `None`. Carried in the
    /// event data rather than as a topic so that the topic layout indexers
    /// already filter on is the same whether or not a memo is supplied.
    pub memo: Option<BytesN<32>>,
}

/// A multi-token batch completed. Emitted once per `execute_many_tokens` call.
///
/// Unlike `Executed` there is no `token` or `total` field because a
/// multi-token batch may touch several tokens whose amounts are
/// incommensurable. Indexers should correlate the per-token `transfer` events
/// in the same transaction to recover the full picture.
#[contractevent(topics = ["batch_payout", "executed_many"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutedMany {
    #[topic]
    pub funder: Address,
    pub count: u32,
}
