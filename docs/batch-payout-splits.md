# Batch payout: splitting a total, and where the dust goes

`batch_payout` has three ways to decide who gets how much. Only the two that
split a *total* can produce a remainder ("dust"). Each one has a fixed,
tested rule for where that remainder goes.

| Entry point | Input | Dust |
|---|---|---|
| `execute` | an explicit amount per line | none: every line is paid as given |
| `execute_equal` | `amount_each` | none: total is exactly `amount_each * count` |
| `execute_equal_split` / `preview_equal` | `total` split evenly | `total % count`, paid to the **last** recipient |
| `execute_weighted` | `total` and basis-point `shares` | truncation from `mul_bps`, paid to the **last** recipient |

In every case the recipients together receive exactly the total, and nothing
stays with the funder or the contract.

## Equal split (`execute_equal_split`, `preview_equal`)

```
amount_each = total / count          (integer division, truncating)
dust        = total - amount_each * count   (0 <= dust < count)
recipient[i]       receives amount_each        for i < count - 1
recipient[last]    receives amount_each + dust
```

- **One recipient.** `amount_each = total`, `dust = 0`.
- **`MAX_RECIPIENTS`.** Same rule; at most `MAX_RECIPIENTS - 1` units of dust.
- **`total < count`.** `amount_each = 0` and `dust = total`. Only the last
  recipient is paid. Lines that work out to zero make no transfer but are
  still counted in `count`.

`preview_equal` runs the same helper as `execute_equal_split` and returns the
same `EqualSplitReceipt { count, total, amount_each, dust, memo }` (with
`memo: None`), so a successful preview is exactly what the execution pays.

## Weighted split (`execute_weighted`)

`shares` pairs each recipient with basis points (`10_000` = 100%). They must
sum to exactly `10_000`, otherwise the call fails with `InvalidBasisPoints`.

```
recipient[i]    receives mul_bps(total, bps[i])          for i < count - 1
recipient[last] receives total - sum(previous payouts)
```

`mul_bps` truncates toward zero, so the last recipient absorbs at most
`count - 1` units of dust. Put the recipient who should take the rounding
last. A zero share is allowed and pays nothing.

## Tests

`contracts/batch_payout/src/test.rs` checks these rules against token
balances. The tests are:

- `equal_split_one_recipient_*`, `equal_split_at_max_recipients_*`, and
  `equal_split_total_smaller_than_recipient_count_*`
- `preview_equal_agrees_with_execute_equal_split`, which runs a table of
  totals against a table of recipient counts
- `execute_weighted_payouts_always_sum_to_total`, a seeded property test
