use soroban_sdk::{contracttype, Address, BytesN};

/// Maximum recipients permitted in one batch.
///
/// # Where this number comes from
///
/// Measured, not guessed, as SPEC.md requires. `report_batch_ceiling` in
/// `test.rs` ramps batch size until execution fails; against soroban-sdk
/// 27.0.6 on 2026-09-07 it found **40 recipients execute and 45 exceed the
/// budget** (`Error(Budget, ExceededLimit)`). An initial guess of 100 was
/// wrong by more than a factor of two, which is the argument for measuring.
///
/// `max_recipients_is_within_what_actually_executes` re-measures on every CI
/// run and fails if this constant drifts above what the code can actually do,
/// so added logic that raises the per-recipient cost cannot silently make the
/// documented cap a lie.
///
/// # It is still not the on-chain ceiling
///
/// The local test environment models the CPU/memory budget but **not** the
/// transaction size limit a real network applies to the submitted envelope,
/// and its `mock_all_auths` builds a separate authorization entry per transfer
/// where a real submission signs one tree. The real figure could be higher or
/// lower. Re-run the ramp against testnet before `v0.2` and set this from that
/// measurement.
///
/// Erring low is deliberate: a refused batch is an inconvenience, a batch that
/// exhausts the ledger budget is an outage.
pub const MAX_RECIPIENTS: u32 = 40;

/// One line of a batch: who is paid, and how much.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payment {
    /// The recipient's address. Must be a valid Stellar address.
    pub to: Address,
    /// The amount to transfer, in the token's smallest unit (e.g., stroops for
    /// XLM, or the base unit defined by the token's decimals). Must be
    /// non-negative; negative amounts are rejected.
    pub amount: i128,
}

/// One line of a multi-token batch: which token to use, who is paid, and how
/// much.
///
/// Used by `execute_many_tokens` to describe a payment in a batch that may
/// draw from more than one token at once.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiTokenPayment {
    /// The token contract to draw the payment from.
    pub token: Address,
    /// The recipient's address.
    pub to: Address,
    /// The amount to transfer, in the token's smallest unit. Must be positive.
    pub amount: i128,
}

/// The outcome of a batch, returned to the caller and emitted as an event.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Receipt {
    /// How many recipients were paid.
    pub count: u32,
    /// The sum actually moved.
    ///
    /// For `execute_many_tokens` this is the sum of raw amounts across all
    /// tokens; the values are incommensurable when tokens have different
    /// denominators, so treat this field as informational rather than
    /// financially significant.
    pub total: i128,
    /// The caller's per-batch reference, echoed back unchanged (#428).
    ///
    /// An opaque 32 bytes the contract never interprets -- typically a hash or
    /// id that ties this on-chain batch to an off-chain pay run. It is an
    /// argument of the entry point, so it is covered by the funder's
    /// authorization. `None` when the caller supplied none, and always `None`
    /// from `preview` and `execute_many_tokens`, which take no memo.
    pub memo: Option<BytesN<32>>,
}

/// The outcome of an equal split of a `total`, from `execute_equal_split` and
/// `preview_equal`.
///
/// Both entry points return this one shape, computed by the same helper, so a
/// preview that succeeds reports exactly what the execution will pay.
///
/// # The dust rule
///
/// `amount_each = total / count` (integer division, truncating) and
/// `dust = total - amount_each * count`, so `0 <= dust < count`. Every
/// recipient receives `amount_each`; the **last** recipient in the list
/// additionally receives `dust`. The sum of all payouts is therefore exactly
/// `total`: nothing is rounded away and nothing stays with the funder.
///
/// When `total < count`, `amount_each` is `0` and the whole `total` is dust
/// paid to the last recipient. Zero-amount lines move no tokens but still
/// count in `count`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EqualSplitReceipt {
    /// How many recipient lines the batch contained.
    pub count: u32,
    /// The sum moved: always exactly the `total` the caller asked to split.
    pub total: i128,
    /// What every recipient receives, before dust.
    pub amount_each: i128,
    /// The remainder of `total / count`, paid on top to the last recipient.
    pub dust: i128,
    /// The caller's per-batch reference; see [`Receipt::memo`]. Always `None`
    /// from `preview_equal`.
    pub memo: Option<BytesN<32>>,
}

// There was a `pub type Payments = Vec<Payment>` alias here, used in the
// `execute` and `preview` signatures. Do not reintroduce it.
//
// `#[contractimpl]` reads argument types syntactically and cannot resolve an
// alias, so it emitted a contract spec referring to a user-defined type called
// `Payments` that does not exist. The contract deployed and the Rust tests all
// passed, but every client reading the spec broke: `stellar contract invoke`
// failed with `Missing Entry Payments` on *any* function of the contract,
// including argument-less ones, because the whole interface failed to load.
//
// Spell soroban types out in full in entry-point signatures.
