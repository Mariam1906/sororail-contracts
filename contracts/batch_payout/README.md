# sororail-batch-payout

## Purpose

Batch payout routes one transaction to many recipients: the payroll primitive.
It holds no storage and no funds — there is no `init`, and therefore no
`storage.rs` in this crate, unlike its siblings. Each call pulls straight from
the funder to each recipient and leaves nothing behind, so there is no position
to archive and no TTL to extend.

**All-or-nothing.** A single failed transfer aborts the whole invocation and
the host reverts every transfer that preceded it. Nobody is paid unless
everybody is. The contract relies on that host guarantee rather than attempting
to unwind by hand, and deliberately never catches a transfer error. Batches are
capped at `MAX_RECIPIENTS` (40) so the check fits the wasm budget.

## Entry points

| Entry point | Caller | Effect |
| --- | --- | --- |
| `execute` | funder | Pays a list of `Payment { to, amount }` entries from one token atomically. |
| `execute_equal` | funder | Pays the same `amount_each` of one token to every recipient atomically. |
| `execute_many_tokens` | funder | Pays a list of `MultiTokenPayment { token, to, amount }` entries atomically. |
| `preview` | anyone | Validates a single-token batch and returns its total without transferring. |
| `max_recipients` | anyone | Returns the compile-time recipient cap. |

`execute`, `execute_equal`, `execute_many_tokens` and `preview` take an
`only_eoa` flag. When it is `true`, a recipient that is a smart-contract address
is rejected with `BatchRecipientIsContract` (103) before any token moves.

## Example flow

```text
1. client.preview(&payments, &only_eoa) validates size, amounts, total and
   recipient types without moving funds.
2. client.execute(&funder, &token, &payments, &only_eoa) requires the funder's
   signature and transfers each payment in order.
   Or: client.execute_equal(&funder, &token, &recipients, &amount_each,
   &only_eoa).
   Or: client.execute_many_tokens(&funder, &payments, &only_eoa) pays each line
   from its own token.
3. If any transfer fails (for example, insufficient balance), the host reverts
   the whole call: no recipient keeps a payment and no receipt is emitted.
```

For `execute_many_tokens`, the receipt `total` is the arithmetic sum of raw
amounts across tokens with different denominations; treat it as informational
only.

Full CLI commands are in
[docs/CONTRACT_INVOKE.md](../../docs/CONTRACT_INVOKE.md), and the token movement
diagram is in [docs/fund-flows.md](../../docs/fund-flows.md).

## Errors

Failures use the shared `Error` enum in
[contracts/common/src/errors.rs](../common/src/errors.rs), re-exported as
`sororail_batch_payout::Error`. Batch payout owns codes 100–119; the canonical
table is [docs/ERRORS.md](../../docs/ERRORS.md).

| Variant | Raised by |
| --- | --- |
| `BatchEmpty` | a batch with no recipients |
| `BatchTooLarge` | more recipients than `MAX_RECIPIENTS` |
| `InvalidAmount` | any non-positive amount in the batch |
| `IdenticalParties` | a recipient that is this contract's own address |
| `BatchRecipientIsContract` | a contract-address recipient when `only_eoa` is `true` |
| `Overflow` | the batch total exceeding `i128` |

Token-level failures (for example, insufficient balance) are host errors from
the token contract and revert the entire batch; they are not converted into a
typed `Error` variant.
