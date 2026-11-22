# sororail-vesting

## Purpose

Vesting releases tokens against a schedule with a cliff. One grant is stored per
deployed instance and is fully funded up front. Nothing vests before the cliff;
after it, vesting is linear until fully vested at `start + duration`. A
revocable grant lets the grantor reclaim the unvested remainder while the vested
portion stays claimable by the beneficiary.

`cliff` and `duration` are spans in seconds from `start`, not absolute
timestamps. The exactness invariant is:

```text
claimed + returned + remaining == total
```

## Entry points

| Entry point | Caller | Effect |
| --- | --- | --- |
| `create` | grantor | Records the schedule and pulls the full `total` amount in. |
| `claim` | beneficiary | Pays out what has vested and not yet been claimed. |
| `revoke` | grantor | On a revocable grant, returns the unvested remainder to the grantor. |
| `vested_amount` | anyone | Vested amount at a given ledger timestamp. |
| `claimable`, `remaining`, `schedule` | anyone | Current claimable amount, unclaimed balance, and checkpoints. |
| `bump`, `get` | anyone | Extends TTL; reads the full grant record. |

## Example flow

```text
1. client.create(&grantor, &beneficiary, &token, &total, &start,
                 &cliff, &duration, &revocable)
2. Before start + cliff: client.claim() returns VestingCliffNotReached.
3. After the cliff: client.claim() pays the vested-but-unclaimed amount.
4. If revocable: client.revoke() freezes vesting at the current timestamp,
   returns the unvested remainder to the grantor, and leaves the vested
   remainder claimable by the beneficiary.
5. client.vested_amount(&ts) and client.schedule() render the schedule.
```

Full CLI commands are in
[docs/CONTRACT_INVOKE.md](../../docs/CONTRACT_INVOKE.md), and the token movement
diagram is in [docs/fund-flows.md](../../docs/fund-flows.md).

## Errors

Failures use the shared `Error` enum in
[contracts/common/src/errors.rs](../common/src/errors.rs), re-exported as
`sororail_vesting::Error`. Vesting owns codes 60–79; the canonical table is
[docs/ERRORS.md](../../docs/ERRORS.md).

| Variant | Raised by |
| --- | --- |
| `AlreadyInitialized` | `create` on an initialized instance |
| `NotInitialized` | any entry point before `create` |
| `IdenticalParties` | `create` with `grantor == beneficiary` |
| `InvalidAmount` | `create` with a non-positive total |
| `InvalidDuration` | `create` with a zero duration |
| `Overflow` | proportional vesting math exceeding `i128` |
| `VestingCliffAfterEnd` | `create` with `cliff > duration` |
| `VestingCliffNotReached` | `claim` before `start + cliff` |
| `VestingNothingToClaim` | `claim` with nothing vested-but-unclaimed |
| `VestingNotRevocable` | `revoke` on a grant created with `revocable = false` |
| `VestingRevoked` | `revoke` on an already-revoked grant |
| `VestingScheduleOverflow` | `create` with `start + duration` beyond `u64::MAX` |
