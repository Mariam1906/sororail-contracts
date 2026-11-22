# Recurring payment keepers

Recurring authorizations are pull-based: the contract does not run itself.
The payee, or a keeper acting on its behalf, must submit the charge transaction.
The payer must first approve the recurring contract as a spender of the token.

## Polling and choosing a charge entry point

Poll `is_chargeable` at a cadence appropriate for the subscription, using
`next_chargeable_at` to avoid unnecessary requests. A true result means the
stored schedule is due and the authorization is not cancelled or exhausted;
it does not guarantee that the token transfer will succeed. The payer may
have revoked or used the allowance, and the payer's balance may be too low.

Use `charge` for one payment at a time. It restarts the next due time from the
successful charge timestamp, so periods skipped during keeper downtime are
forfeited. Use `charge_up_to(max_periods)` only when bounded catch-up is
intended. It transfers one payment per elapsed scheduled period, subject to
the explicit per-call limit and the authorization's remaining lifetime cap.
Neither method can charge before the first full period after authorization.

## Retry and fee handling

Treat each invocation as a real transaction, not as a guaranteed payment:

- If simulation or submission fails, inspect the typed contract error and token
  result. Retry transient RPC or network errors with backoff and jitter.
- `RecurringPeriodNotElapsed` means the keeper should wait until the next due
  time rather than retrying immediately.
- `RecurringCancelled` and `RecurringExhausted` are terminal for this
  authorization. An insufficient allowance or balance requires payer action;
  do not spin on retries.
- Submit with an account that can authorize the payee and can cover network
  fees. The keeper pays the transaction fee; the contract does not deduct a
  fee from the recurring payment.
- Re-read the schedule and remaining cap after a successful transaction.
  `Charged` is emitted once per transferred period and carries its period index,
  remaining cap (when capped), and the next due timestamp.

## TypeScript sketch

This sketch assumes a TypeScript client generated from the recurring contract
spec with the Stellar SDK. Configure the generated client with the payee's
signing function and your RPC/network settings:

```ts
async function pollAndCharge(client: RecurringClient, catchUpLimit = 1) {
  const { result: chargeable } = await client.isChargeable({});
  if (!chargeable) return;

  try {
    if (catchUpLimit > 1) {
      await (await client.chargeUpTo({ maxPeriods: catchUpLimit })).signAndSend();
    } else {
      await (await client.charge({})).signAndSend();
    }
  } catch (error) {
    // Classify contract errors as terminal/wait-for-due; retry transient
    // transport or submission failures with bounded exponential backoff.
    throw error;
  }
}
```

In production, ensure the keeper persists its retry state, applies a maximum
retry window, and monitors failed invocations. Do not retry a transaction
whose outcome is uncertain without first checking its status or the contract's
updated state.
