# Stream cancel: worked examples

When a cancellable stream is cancelled, the contract splits what is left
between the two parties in one step:

- **Settled to the recipient** is everything that has accrued but not yet been
  withdrawn, as of the cancel timestamp.
- **Refunded to the sender** is the deposit that has not accrued, which is
  `deposited - accrued_at(cancel)`. Withdrawals do not change the refund,
  because they are already counted in the accrual.

Accrual stops at the earlier of `stop` and the cancel timestamp, so a cancel
after `stop` refunds nothing. A cancel before `start` refunds the whole
deposit.

The stream used below is `rate_per_second = 100` over `start = 1_000_000` and
`stop = 1_001_000`, so `deposited = 100 * 1_000 = 100_000`.

Each example has a matching test in `contracts/stream/src/test.rs`. The numbers
in this document and in those tests are the same.

## Example 1: cancel mid-stream, no prior withdrawals

At `start + 400`, `100 * 400 = 40_000` has accrued and nothing has been
withdrawn.

| | Amount |
|---|---|
| Settled to recipient | 40_000 |
| Refunded to sender | 100_000 - 40_000 = 60_000 |

Test: `cancel_worked_example_1_mid_stream_without_withdrawals`.

## Example 2: cancel after a partial withdrawal

At `start + 200`, 20_000 has accrued and the recipient withdraws 10_000. At
`start + 500`, 50_000 has accrued in total.

| | Amount |
|---|---|
| Withdrawn before cancel | 10_000 |
| Settled to recipient by cancel | 50_000 - 10_000 = 40_000 |
| Recipient's total | 50_000 |
| Refunded to sender | 100_000 - 50_000 = 50_000 |

The `Cancelled` event reports `settled_to_recipient = 40_000` and
`refunded_to_sender = 50_000`. The event does not include the earlier
withdrawal. Read `withdrawn` from `get()` for the running total.

Test: `cancel_worked_example_2_after_a_partial_withdrawal`.

## Example 3: cancel after `stop`, with a prior withdrawal

At `start + 300`, 30_000 has accrued and the recipient withdraws 10_000. The
cancel happens at `stop + 100`. Accrual has halted at `stop`, so the full
100_000 has accrued.

| | Amount |
|---|---|
| Withdrawn before cancel | 10_000 |
| Settled to recipient by cancel | 100_000 - 10_000 = 90_000 |
| Recipient's total | 100_000 |
| Refunded to sender | 0 |

The sender's balance does not change, and the contract holds nothing after the
cancel.

Test: `cancel_worked_example_3_after_stop_refunds_nothing`.

## Invariant

Across every case:

```text
withdrawn + refunded + remaining == deposited
```

After a cancel, `remaining` is `0`. Each test calls the conservation check
`assert_conserved` to verify this.
