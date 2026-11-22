# Vesting Arithmetic Safe Range

`Grant::vested_amount` uses checked i128 arithmetic for the linear vesting step:

```text
total * elapsed / duration
```

The largest safe `total` before the multiplication overflows is:

```text
i128::MAX / elapsed
```

For a ten-year grant, the largest elapsed value before the end shortcut is roughly 315,359,999 seconds. That leaves a safe total of about `5.39e29` base units. A common launch-sized grant such as one billion 18-decimal tokens (`1e27` base units) is comfortably inside that range, and the vesting test suite now covers that scenario directly.

At or after the grant end timestamp, vesting returns the full grant amount before performing the multiplication, so the final timestamp does not create an additional overflow path.

## Schedule Checkpoints and Computation Complexity

The vesting schedule is defined by three parameters: `start`, `cliff`, and `duration`. The `schedule()` entry point returns these as corner points:

1. **Start point** (`at = start`): vested = 0
2. **Cliff point** (`at = start + cliff`, if cliff > 0): vested = proportional amount
3. **End point** (`at = start + duration`): vested = total

This creates a maximum of 3 checkpoints:
- If `cliff == 0`, the cliff point is omitted (linear from start)
- If `cliff == duration`, the end point is omitted (all-or-nothing at once)
- Otherwise, all three points appear

The `vested_amount()` computation does not loop. It directly computes vesting amount using linear interpolation, so its cost is O(1) regardless of schedule parameters. The maximum checkpoint count of 3 represents the inherent shape of the schedule (zero before cliff, linear after cliff, complete at end) and does not impose a computational burden.

## Cliff and Rounding Semantics

Vesting uses integer division (`total * elapsed / duration`), which rounds toward zero. The cliff timestamp is **inclusive**: at exactly `start + cliff`, the elapsed proportion vests in one step.

Consider a 1,000-unit grant vesting over 1,000 seconds with a 100-second cliff:

| Timestamp | Seconds Elapsed | Vested Amount | Notes |
|-----------|-----------------|---------------|-------|
| start (0s) | 0 | 0 | Nothing vests before the cliff |
| start + 99s | 99 | 0 | Still before the cliff (cliff is at 100s) |
| start + 100s | 100 | 100 | Cliff hits: 1000 × 100 / 1000 = 100 |
| start + 500s | 500 | 500 | Linear: 1000 × 500 / 1000 = 500 |
| start + 999s | 999 | 999 | 1000 × 999 / 1000 = 999 (before end) |
| start + 1000s | 1000 | 1000 | End: fully vested |
| start + 1001s | 1001 | 1000 | After end: remains at total |

**Rounding toward zero:** Division rounds down. For example, at 333 seconds of a 1000-second schedule with 1000 units:
- `1000 × 333 / 1000 = 333` (exact)

At 1 second:
- `1000 × 1 / 1000 = 1` (exact)

The "dust" (fractional units lost to rounding) stays in the contract until full vesting, when the entire total is released. This prevents any leakage: the beneficiary receives exactly `total` after `start + duration`, regardless of the schedule resolution.

**Cliff inclusivity:** The cliff timestamp is the first moment when vesting occurs. At `start + cliff - 1`, nothing has vested; at `start + cliff`, the elapsed proportion vests in one step.