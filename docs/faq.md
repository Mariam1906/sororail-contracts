# Frequently Asked Questions

Answers to questions that come up repeatedly in issues and discussions.

---

## 1. Why can't contracts be upgraded after deployment?

Soroban contracts are **immutable once deployed**. The network stores the
compiled WASM by its hash; there is no upgrade mechanism that replaces logic in
place. This is a Soroban platform constraint, not a SoroRail design choice.

When a bug fix or feature is needed, a new contract instance must be deployed
and existing positions (escrows, streams, grants, subscriptions) need to migrate
to it. The [README § Contract Immutability and Migration](../README.md#contract-immutability-and-migration)
describes the migration plan: drain old instances, direct new interactions to
the new address, and update the SDK or wrapper to point at the new deployment.

Deployment addresses are recorded in [DEPLOYMENTS.md](../DEPLOYMENTS.md).

---

## 2. Which tokens do these contracts support?

All five contracts interact with tokens through the
[SEP-41 token interface](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0041.md).
They assume **well-behaved, standard tokens** only. The following token types
are explicitly **out of scope**:

| Token type | Why it breaks |
|---|---|
| Fee-on-transfer | The contract holds less than it believes, breaking `withdrawn + refunded + remaining == deposited`. |
| Rebasing | Balances change outside of transfers; the contract cannot distribute a surplus or cover a deficit. |
| Callback / re-entrant | Re-entry during `transfer` has not been exhaustively tested across all entry points. |

For the full reasoning and the per-contract defences, see
[SECURITY.md § Supported token assumptions](../SECURITY.md#supported-token-assumptions).

---

## 3. What are token decimals and how do I pass amounts?

Stellar tokens can carry between 0 and 18 decimal places; the network uses
`i128` amounts in the **smallest indivisible unit**. SoroRail passes amounts
straight through to the SEP-41 `transfer` and `transfer_from` calls without any
decimal conversion — the contracts never multiply or divide by a decimal factor.

What this means in practice: if a token has 7 decimal places (the Stellar
native XLM convention), 1.0 token = `10_000_000` as an `i128`. Callers are
responsible for the conversion. The `sororail-sdk` frontend package provides
helpers for this.

All arithmetic inside the contracts goes through
[`sororail_common::math`](../contracts/common/src/math.rs), which returns an
error on overflow rather than producing a wrong-but-plausible balance.
`overflow-checks` is enabled in release builds.

---

## 4. Who pays the storage rent (TTL / ledger entry fees)?

The account that calls a mutating entry point pays the ledger write fee for
that transaction, which includes the initial TTL extension. After the initial
funding period, **anyone** can call the public `bump()` entry point on each
contract to extend its TTL — the caller pays that transaction fee.

Soroban archives ledger entries that are not extended. Any stream, grant,
escrow, or subscription that runs longer than roughly 30 days needs `bump()`
called before it expires. The full renewal schedule and operator responsibilities
are documented in [docs/TTL_STRATEGY.md](TTL_STRATEGY.md).

---

## 5. What happens if the escrow arbiter disappears?

If a dispute is raised and the arbiter's key is lost or they become
unresponsive, funds would otherwise be locked in the `Disputed` state forever.
To prevent that:

- Once the original `deadline` passes, a **7-day grace period** begins.
- After the grace period, **anyone** may call `refund()` to return the full
  amount to the original depositor.

This design prioritises recovery over absolute arbiter authority. See
[SECURITY.md § Escrow dispute timeout](../SECURITY.md#escrow-dispute-timeout)
for the rationale, and
[`contracts/escrow/src/contract.rs`](../contracts/escrow/src/contract.rs) for
the implementation.

---

## 6. Why are error codes integers instead of strings?

The `Error` enum in
[`contracts/common/src/errors.rs`](../contracts/common/src/errors.rs) assigns
each contract a numeric range, and clients decode failures **by number**. Using
integers means the ABI is stable across SDK versions and language bindings: a
JavaScript frontend parsing `110` will keep working even if the Rust enum
variant is renamed or its doc comment changes.

Consequence: a released variant is **never renumbered and never removed**. When
a variant is retired, its integer is "burned" (left as a gap). New variants
are always appended inside the documented range for that contract.

---

## 7. Why does the contract store state before calling `transfer`?

Every mutating entry point that calls the token (`fund`, `release`, `refund`,
`resolve`, `pay`, `pull`, etc.) **persists state before making the token call**.
This is an intentional re-entry defence: a callback token that re-enters the
contract via a different entry point will observe the already-committed
post-transition state rather than the pre-transition one, so it cannot
double-spend or trigger an illegal state transition.

Each `contract.rs` file documents this with an inline comment at the relevant
call sites. For example:

- [`contracts/escrow/src/contract.rs`](../contracts/escrow/src/contract.rs) —
  `fund`, `release`, `refund`, `resolve` all save before `transfer`.
- [`contracts/stream/src/contract.rs`](../contracts/stream/src/contract.rs) —
  `pay` / `pull` save before `transfer`.

---

## 8. Why do some entry points take a `caller: Address` argument while others do not?

The choice comes down to **how many parties may call the entry point**:

- **Exactly one fixed party** → no `caller` argument; the contract calls
  `require_auth` directly on the stored address (e.g. `stream::cancel`, which
  only the sender may call).
- **More than one permitted party** → takes `caller: Address`; a guard from
  [`sororail_common::auth`](../contracts/common/src/auth.rs) checks membership
  before prompting for a signature (e.g. `escrow::release`, which the depositor
  or arbiter may call).

Adding a `caller` argument to a single-party entry point just widens the ABI
and adds a check that can only ever compare an address with itself. The full
reasoning is in
[CONTRIBUTING.md § Choosing an authorization pattern](../CONTRIBUTING.md#choosing-an-authorization-pattern).

---

## 9. How does the streaming payment clock work?

`sororail-stream` does not use a wall-clock or an oracle. It reads
`env.ledger().timestamp()`, the UNIX timestamp embedded in the ledger by
validators. Time is advanced in tests by calling
`env.ledger().set_timestamp(...)` on the test ledger — no mocking is needed.

A stream accrues `rate_per_second * elapsed_seconds` to the recipient. The full
per-second accumulation and the conservation invariant
(`withdrawn + remaining + refunded == deposited`) are tested in
[`contracts/stream/src/test.rs`](../contracts/stream/src/test.rs).

---

## 10. Can I run the contracts locally without Stellar testnet?

Yes. The `soroban_sdk::testutils` environment runs entirely in-process without
any network connection. Run the full suite with:

```bash
make test
```

For an end-to-end smoke test that deploys real WASM to a local
`stellar/quickstart` container, trigger the **Quickstart smoke** workflow from
the Actions tab, or run:

```bash
./scripts/smoke_quickstart.sh
```

That script requires Docker and the `stellar` CLI (v27.1.0). It is **not** a
required PR check — it is optional validation before testnet deployments.
See [CONTRIBUTING.md § Setup](../CONTRIBUTING.md#setup) for the local dev setup.
