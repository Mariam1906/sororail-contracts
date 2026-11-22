# Threat model

This document complements [SECURITY.md](../SECURITY.md). SECURITY.md covers
reporting, supported tokens and known design decisions; this document states
what each contract trusts, what an attacker can do, how each threat is mitigated,
and what remains for the audit to examine.

**Status: unaudited, testnet only.** Do not deploy these contracts to mainnet or
use them with real value. See [SECURITY.md](../SECURITY.md) for the policy.

## Assets and trust boundaries

| Asset | Where it lives | Trust boundary |
| --- | --- | --- |
| Deposited token balances | SAC balances owned by the escrow, stream and vesting contract instances | Token contract and its issuer/admin |
| Authorization records and period counters | One Soroban instance per contract instance | Contract code, callers with valid signatures |
| Lifecycle state (escrow `State`, cancellation/revocation flags) | Single instance storage position | Contract code and authorized parties |
| Allowance for recurring charges | Payer's SAC balance, spent by the recurring contract | Token contract and payer |
| Ledger timestamp and TTL | Soroban host | Validators and the operator calling `bump()` |

Out of scope: compromised caller keys, Stellar protocol or `soroban-sdk`
vulnerabilities, fee-on-transfer/rebasing/callback tokens, and mainnet use (see
[SECURITY.md § Scope](../SECURITY.md#scope)).

## Shared trust assumptions

| Assumption | Why it matters | Mitigation / evidence |
| --- | --- | --- |
| Tokens are well-behaved SEP-41 contracts: transfers move exactly the requested amount, do not rebase, and do not call back into the contract. | Conservation accounting assumes the contract's balance changes only through its own calls. | Documented in [SECURITY.md § Supported token assumptions](../SECURITY.md#supported-token-assumptions); state is saved before every token call in [escrow/contract.rs](../contracts/escrow/src/contract.rs) and [stream/contract.rs](../contracts/stream/src/contract.rs). |
| Ledger time is supplied by validators and does not jump backwards within a transaction. | Accrual, cliffs, deadlines and charge scheduling use `env.ledger().timestamp()`. | Pure time math in [stream/types.rs](../contracts/stream/src/types.rs) and [vesting/types.rs](../contracts/vesting/src/types.rs); rationale in [FAQ § 9](faq.md#9-how-does-the-streaming-payment-clock-work). |
| Callers are identified by Soroban `require_auth`; a wrong signer is rejected by the host. | All privileged operations act on stored addresses or a two-party allow-list. | Shared guards in [common/auth.rs](../contracts/common/src/auth.rs), with tests in [common/test.rs](../contracts/common/src/test.rs). |
| Contract code is immutable after deployment. | There is no emergency upgrade path; fixes require a new deployment and migration. | [README § Contract Immutability and Migration](../README.md#contract-immutability-and-migration) and [docs/reproducible-builds.md](reproducible-builds.md). |
| Instance TTL is maintained by an operator calling `bump()` before expiry. | Archived instances cannot be read or mutated until restored. | Public `bump()` in every stateful contract; schedule in [TTL_STRATEGY.md](TTL_STRATEGY.md). |
| A host error reverts all writes made in the same invocation. | Batch payout relies on this instead of unwinding transfers; other contracts rely on it when a token transfer fails after state was saved. | [batch_payout/contract.rs](../contracts/batch_payout/src/contract.rs) and its atomic-failure tests in [batch_payout/test.rs](../contracts/batch_payout/src/test.rs). |
| Issuer flags may change while a position is open. | `AUTH_REQUIRED`/`AUTH_REVOCABLE` can freeze a recipient's balance; clawback can remove a contract balance outright. | Tested for streams in [stream/test.rs](../contracts/stream/src/test.rs) (`withdraw_fails_without_state_change_when_recipient_is_deauthorized`); recovery is documented below. |

## Attacker capabilities

1. **Any account** can call read-only views and `bump()`, observe events and
   ledger time, and submit any entry point with its own address as `caller`.
   Membership and signature checks decide what it can change.
2. **A party to an agreement** (depositor, beneficiary, sender, recipient,
   grantor, payer or payee) can sign for itself and try to exceed its rights:
   withdraw more than accrued, release escrow without the right role, revoke a
   non-revocable grant, or charge early.
3. **A third party with a valid signature** can try to act as an arbiter or
   cancel someone else's authorization; neutrality and allow-list checks reject
   it.
4. **The token issuer/admin** can change flags, authorize or deauthorize
   balances, and claw back funds when the asset permits it. This is a trusted
   party for the token, not for the payment contract.
5. **An unresponsive arbiter** can leave an escrow disputed; the recovery path
   must not depend on that key.
6. **An operator who lets TTL expire** can turn a live position into an archived
   one until restoration; `bump()` is deliberately permissionless to reduce
   this risk.

## Per-contract mitigations

### Escrow

| Threat | Mitigation | Evidence |
| --- | --- | --- |
| Depositor funds an escrow with an arbiter who is one of the parties. | `init` rejects a non-neutral arbiter with `EscrowArbiterNotNeutral`. | [escrow/contract.rs](../contracts/escrow/src/contract.rs); [escrow/test/init.rs](../contracts/escrow/src/test/init.rs). |
| Unauthorized release, refund or dispute. | `release`, `refund` and `dispute` take a `caller` checked against the permitted set before `require_auth`; single-party entry points call `require_auth` directly. | [common/auth.rs](../contracts/common/src/auth.rs); [escrow/test/release.rs](../contracts/escrow/src/test/release.rs), [escrow/test/dispute.rs](../contracts/escrow/src/test/dispute.rs). |
| Funding after the deadline or from the wrong state. | Explicit state and time guards return `EscrowNotFundable` / `DeadlinePassed` before any token movement. | [escrow/contract.rs](../contracts/escrow/src/contract.rs); [escrow/test/fund.rs](../contracts/escrow/src/test/fund.rs). |
| Arbiter disappears after a dispute. | After `deadline + 7 days`, anyone may refund the full amount to the depositor. | [escrow/contract.rs](../contracts/escrow/src/contract.rs); [SECURITY.md § Escrow dispute timeout](../SECURITY.md#escrow-dispute-timeout). |
| Callback token re-enters during a payout. | State is saved before each `transfer`; illegal transitions then fail against the persisted state. | [escrow/contract.rs](../contracts/escrow/src/contract.rs); [escrow/test/resolve.rs](../contracts/escrow/src/test/resolve.rs). |
| Arbiter takes more than the escrow amount via `resolve`. | `split_bps` is capped at 10,000 and the remainder is derived by subtraction. | [common/math.rs](../contracts/common/src/math.rs); [escrow/test/resolve.rs](../contracts/escrow/src/test/resolve.rs). |

### Stream

| Threat | Mitigation | Evidence |
| --- | --- | --- |
| Recipient withdraws more than has accrued. | `withdraw` computes availability from ledger time and rejects larger requests with `StreamInsufficientAccrued`. | [stream/contract.rs](../contracts/stream/src/contract.rs); [stream/test.rs](../contracts/stream/src/test.rs). |
| Sender withdraws funds that have already accrued via `cancel`. | Cancellation freezes accrual at `now`, pays the recipient that amount and refunds only the unaccrued remainder. | [stream/contract.rs](../contracts/stream/src/contract.rs); [stream/test.rs](../contracts/stream/src/test.rs). |
| Sender or recipient acts without authorization. | Each mutating entry point calls `require_auth` on its single stored party; a negative-auth test asserts the failure. | [stream/contract.rs](../contracts/stream/src/contract.rs); [stream/test.rs](../contracts/stream/src/test.rs). |
| Top-up or extension breaks the per-second rate / overflows `i128`. | Amounts must be exact multiples of the rate; all funding math is checked. | [common/math.rs](../contracts/common/src/math.rs); [stream/test.rs](../contracts/stream/src/test.rs). |
| Recipient's token balance is deauthorized mid-stream. | The failing transfer returns a host error; the host rolls back the saved state, and re-authorizing the recipient restores the withdrawable amount. | [stream/test.rs](../contracts/stream/src/test.rs); recovery path below. |
| Contract instance expires mid-stream. | Permissionless `bump()` extends TTL; mutation extends it automatically. | [stream/contract.rs](../contracts/stream/src/contract.rs); [TTL_STRATEGY.md](TTL_STRATEGY.md). |

### Vesting

| Threat | Mitigation | Evidence |
| --- | --- | --- |
| Beneficiary claims before the cliff. | `claim` checks `start + cliff` and returns `VestingCliffNotReached`. | [vesting/contract.rs](../contracts/vesting/src/contract.rs); [vesting/test/claim.rs](../contracts/vesting/src/test/claim.rs). |
| Grantor revokes a non-revocable grant or revokes twice. | `revocable` flag and `revoked_at` guard return `VestingNotRevocable` / `VestingRevoked`. | [vesting/contract.rs](../contracts/vesting/src/contract.rs); [vesting/test/revoke.rs](../contracts/vesting/src/test/revoke.rs). |
| Revocation takes vested funds from the beneficiary. | Vesting is frozen at `revoke` time; only `total - vested` returns to the grantor, and the vested remainder stays claimable. | [vesting/types.rs](../contracts/vesting/src/types.rs); [vesting/test/conservation.rs](../contracts/vesting/src/test/conservation.rs). |
| Malformed schedule (`cliff > duration`, zero duration, overflowing `start + duration`). | Validation in `create`; checked arithmetic throughout. | [vesting/contract.rs](../contracts/vesting/src/contract.rs); [vesting/test/create.rs](../contracts/vesting/src/test/create.rs). |
| Accounting drift over many claims and revocations. | Invariant `claimed + returned + remaining == total` is asserted after lifecycle operations and by property tests. | [vesting/test/conservation.rs](../contracts/vesting/src/test/conservation.rs); [vesting/test/property.rs](../contracts/vesting/src/test/property.rs). |

### Recurring

| Threat | Mitigation | Evidence |
| --- | --- | --- |
| Payee charges too early or after the cap. | `next_chargeable_at` and `max_periods` gates return `RecurringPeriodNotElapsed` / `RecurringExhausted`. | [recurring/contract.rs](../contracts/recurring/src/contract.rs); [recurring/test.rs](../contracts/recurring/src/test.rs). |
| Payee banks missed periods and takes several payments at once without asking. | `charge` forfeits skipped periods by scheduling from the charge time; `charge_up_to(max_periods)` is an explicit, caller-bounded catch-up option. | [recurring/types.rs](../contracts/recurring/src/types.rs); [recurring/test.rs](../contracts/recurring/src/test.rs); [docs/recurring-keepers.md](recurring-keepers.md). |
| Payer cannot stop a subscription. | Either party can `cancel`; revoking the token allowance also stops charges immediately. | [recurring/contract.rs](../contracts/recurring/src/contract.rs); [recurring/test.rs](../contracts/recurring/src/test.rs). |
| Third party cancels someone else's authorization. | `cancel` takes a `caller` checked against `{payer, payee}` before `require_auth`. | [common/auth.rs](../contracts/common/src/auth.rs); [recurring/test.rs](../contracts/recurring/src/test.rs). |
| Contract charges more than the payer approved. | The contract never holds funds; the SAC allowance is the real cap and `transfer_from` enforces it. | [recurring/src/lib.rs](../contracts/recurring/src/lib.rs); [recurring/test.rs](../contracts/recurring/src/test.rs). |

### Batch payout

| Threat | Mitigation | Evidence |
| --- | --- | --- |
| Funder pays an oversized or empty batch to exhaust resources. | `MAX_RECIPIENTS` is checked before any transfer; empty batches return `BatchEmpty`. | [batch_payout/types.rs](../contracts/batch_payout/src/types.rs); [batch_payout/test.rs](../contracts/batch_payout/src/test.rs). |
| One bad recipient leaves earlier recipients paid. | Host revert on any transfer error makes the batch all-or-nothing; the contract never catches transfer errors. | [batch_payout/contract.rs](../contracts/batch_payout/src/contract.rs); [batch_payout/test.rs](../contracts/batch_payout/src/test.rs). |
| Funded amount overflows `i128`. | Every batch path sums with checked math and `execute_equal` multiplies with checked math. | [common/math.rs](../contracts/common/src/math.rs); [batch_payout/test.rs](../contracts/batch_payout/src/test.rs). |
| Attacker pays the contract's own address to trap funds. | Recipients equal to the contract address are rejected with `IdenticalParties`. | [batch_payout/contract.rs](../contracts/batch_payout/src/contract.rs); [batch_payout/test.rs](../contracts/batch_payout/src/test.rs). |
| Caller unintentionally pays a contract address that cannot receive the asset. | `only_eoa` rejects contract recipients with `BatchRecipientIsContract` before validation completes or funds move. | [batch_payout/contract.rs](../contracts/batch_payout/src/contract.rs); [batch_payout/test.rs](../contracts/batch_payout/src/test.rs). |
| A third party spends the funder's balance. | `execute`, `execute_equal` and `execute_many_tokens` call `require_auth` on the funder directly. | [batch_payout/contract.rs](../contracts/batch_payout/src/contract.rs); [batch_payout/test.rs](../contracts/batch_payout/src/test.rs). |

## Recovery: token authorization revoked mid-stream

With `AUTH_REQUIRED` and `AUTH_REVOCABLE`, an issuer/admin can deauthorize the
recipient after a stream is funded. Transfers to that recipient then fail at the
token level with `BalanceDeauthorizedError`. The stream contract saves its state
before calling the token, and the host reverts that write when the call fails,
so the failed `withdraw` leaves the stream, the contract balance and every party
balance unchanged; the accrued amount is still withdrawable.

The recovery path is:

1. The token admin calls `StellarAssetClient::set_authorized(recipient, true)`.
2. The recipient calls `withdraw` again; the same accrued amount pays out.
3. If authorization cannot be restored, the sender can call `cancel`, which
   settles accrued funds to the recipient and refunds the unaccrued remainder.

This is covered end-to-end by
`withdraw_fails_without_state_change_when_recipient_is_deauthorized` in
[stream/test.rs](../contracts/stream/src/test.rs). A clawback-enabled asset can
also remove a contract balance without any contract-level authorization check;
that capability belongs to the trusted token issuer and is an explicit
assumption, not a bug in the payment contracts.

## Residual risks for the audit

1. **Escrow post-grace refund branch.** The "anyone may refund after
   `deadline + 7 days`" path is implemented in
   [escrow/contract.rs](../contracts/escrow/src/contract.rs) but is not yet
   covered by a dedicated test. Audit the boundary condition and add coverage.
2. **Multi-entry-point re-entry.** State is saved before token calls (see
   [FAQ § 7](faq.md#7-why-does-the-contract-store-state-before-calling-transfer)),
   but callback tokens are out of scope and cross-entry-point re-entry has not
   been exhaustively tested.
3. **`MAX_RECIPIENTS = 40`.** The bound in
   [batch_payout/types.rs](../contracts/batch_payout/src/types.rs) was measured
   locally; the audit should re-measure the release WASM against the 256 KiB
   budget enforced by [check_wasm_sizes.sh](../scripts/check_wasm_sizes.sh).
4. **Clawback-enabled assets.** An issuer can claw back funds held by escrow,
   stream or vesting, breaking the conservation invariant from outside the
   contracts. Integrators must choose issuers accordingly.
5. **Time and TTL.** Validators supply the timestamp and operators must call
   `bump()`; see [TTL_STRATEGY.md](TTL_STRATEGY.md) and
   [FAQ § 9](faq.md#9-how-does-the-streaming-payment-clock-work).
6. **Deployment provenance.** The `source_repo`, `version` and `git_commit`
   contract metadata in each WASM is only useful when paired with a verifiable
   build; see [reproducible-builds.md](reproducible-builds.md).
