# sororail-escrow

## Purpose

Escrow holds funds for one agreement per deployed instance. The depositor funds
it; the funds leave only by release to the beneficiary, refund to the depositor,
or an arbiter's split after a dispute. When an arbiter is provided at
initialization it must differ from both depositor and beneficiary; otherwise
`init` returns `EscrowArbiterNotNeutral` (27). Without an arbiter, the original
no-dispute behavior is preserved.

```text
Created ──fund──▶ Funded ──release──▶ Released
                    │
                    ├────refund────▶ Refunded
                    │
                    └───dispute────▶ Disputed ──resolve──▶ Resolved
```

Illegal transitions return typed errors; they never panic.

## Entry points

| Entry point | Caller | Effect |
| --- | --- | --- |
| `init` | depositor | Records the agreement. Requires depositor auth. |
| `fund` | depositor | Pulls `amount` of the token into the contract. |
| `cancel` | depositor | Backs out of a `Created` escrow before funding. |
| `release` | depositor or arbiter | Pays the beneficiary. |
| `refund` | depositor (after deadline) or arbiter | Returns funds to the depositor. |
| `dispute` | depositor or beneficiary | Freezes a funded escrow. Needs an arbiter. |
| `resolve` | arbiter | Splits funds per `split_bps`. Only from `Disputed`. |
| `state`, `get`, `bump` | anyone | Reads the lifecycle state or agreement; extends TTL. |

After the deadline plus a seven-day grace period, anyone may call `refund` on a
still-disputed escrow so an unresponsive arbiter cannot lock funds forever.
See [SECURITY.md](../../SECURITY.md) for the rationale.

## Example flow

```text
1. client.init(&depositor, &beneficiary, &Some(arbiter), &token, &amount,
               &deadline)
2. client.fund() -> the contract pulls the amount from the depositor.
3. Happy path: client.release(&depositor) pays the beneficiary.
   Alternative: client.dispute(&beneficiary) freezes the funds, then
   client.resolve(&arbiter, &split_bps) splits them.
   Recovery: after the deadline, client.refund(&depositor) returns the full
   amount (after the grace period, any caller may be passed as `caller`).
```

Full CLI commands are in
[docs/CONTRACT_INVOKE.md](../../docs/CONTRACT_INVOKE.md), and the token movement
diagram is in [docs/fund-flows.md](../../docs/fund-flows.md).

## Errors

Failures use the shared `Error` enum in
[contracts/common/src/errors.rs](../common/src/errors.rs), re-exported as
`sororail_escrow::Error`. Escrow owns codes 20–39; the canonical table is
[docs/ERRORS.md](../../docs/ERRORS.md).

| Variant | Raised by |
| --- | --- |
| `AlreadyInitialized` | `init` on an initialized instance |
| `NotInitialized` | any entry point before `init` |
| `Unauthorized` | a caller outside the permitted set |
| `InvalidAmount` | `init` with a non-positive amount |
| `InvalidTimeRange` | `init` with a deadline at or before now |
| `InvalidBasisPoints` | `resolve` with `split_bps > 10000` |
| `Overflow` | `resolve` on an amount too large to scale by `split_bps` in `i128` |
| `DeadlineNotReached` | depositor `refund` before the deadline; `refund` of a disputed escrow inside its grace period |
| `DeadlinePassed` | `fund` at or after the deadline |
| `EscrowNotFundable` | `fund` when not in `Created` |
| `EscrowNotFunded` | `release`/`refund`/`dispute` when not in `Funded` |
| `EscrowClosed` | any transition from a terminal state |
| `EscrowNoArbiter` | `dispute` with no arbiter configured |
| `EscrowNotDisputed` | `resolve` when not in `Disputed` |
| `EscrowAlreadyDisputed` | `dispute` when already disputed |
| `EscrowNotCancellable` | `cancel` when not in `Created` |
| `EscrowArbiterNotNeutral` | `init` when the arbiter is either escrow party |
