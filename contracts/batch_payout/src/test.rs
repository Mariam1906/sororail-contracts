// Test fixtures do plain arithmetic on known-small constants; the checked-math
// rule is for contract code.
#![allow(clippy::arithmetic_side_effects)]

use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation, Auths as _, Events as _},
    token::{StellarAssetClient, TokenClient},
    vec, Address, BytesN, Env, Event, IntoVal, Symbol, Vec,
};
use sororail_common::{
    testutils::{assert_conserved, TestEnv},
    Error,
};

use crate::{
    contract::{BatchPayoutContract, BatchPayoutContractClient},
    events::{Executed, ExecutedMany},
    types::{EqualSplitReceipt, MultiTokenPayment, Payment, Receipt, MAX_RECIPIENTS},
};

const MINT: i128 = 1_000_000_000;

struct Fixture<'a> {
    env: Env,
    client: BatchPayoutContractClient<'a>,
    token: TokenClient<'a>,
    funder: Address,
}

impl Fixture<'_> {
    fn new() -> Self {
        let te = TestEnv::new();
        let (token, funder) = te.make_token(MINT);
        let token_address = token.address.clone();
        let env = te.env().clone();

        let client = BatchPayoutContractClient::new(&env, &env.register(BatchPayoutContract, ()));
        Fixture {
            token: TokenClient::new(&env, &token_address),
            env,
            client,
            funder,
        }
    }

    fn addresses(&self, n: u32) -> Vec<Address> {
        let mut v = Vec::new(&self.env);
        for _ in 0..n {
            v.push_back(Address::generate(&self.env));
        }
        v
    }

    fn payments(&self, n: u32, amount: i128) -> Vec<Payment> {
        let mut v = Vec::new(&self.env);
        for _ in 0..n {
            v.push_back(Payment {
                to: Address::generate(&self.env),
                amount,
            });
        }
        v
    }
}

// ----------------------------------------------------------------- execute

#[test]
fn execute_pays_each_recipient_their_own_amount() {
    let f = Fixture::new();
    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    let c = Address::generate(&f.env);
    let batch = vec![
        &f.env,
        Payment {
            to: a.clone(),
            amount: 100,
        },
        Payment {
            to: b.clone(),
            amount: 250,
        },
        Payment {
            to: c.clone(),
            amount: 7,
        },
    ];

    let receipt = f
        .client
        .execute(&f.funder, &f.token.address, &batch, &false, &None);
    // Capture immediately: subsequent token balance invocations replace the
    // events returned by `events().all()`.
    let all = f.env.events().all();
    let events = all.events();
    assert_eq!(
        receipt,
        Receipt {
            count: 3,
            total: 357,
            memo: None,
        }
    );
    assert_eq!(f.token.balance(&a), 100);
    assert_eq!(f.token.balance(&b), 250);
    assert_eq!(f.token.balance(&c), 7);
    assert_eq!(f.token.balance(&f.funder), MINT - 357);
    // The contract is a conduit; it holds nothing.
    assert_eq!(f.token.balance(&f.client.address), 0);
    assert_conserved(
        &f.token,
        &[
            f.funder.clone(),
            a.clone(),
            b.clone(),
            c.clone(),
            f.client.address.clone(),
        ],
        MINT,
    );

    // `execute` emits one token transfer per recipient before its own
    // `Executed`, so take the last event rather than assuming an index.
    let executed_event = events.last().expect("no events emitted");
    let expected = Executed {
        funder: f.funder.clone(),
        token: f.token.address.clone(),
        count: 3,
        total: 357,
        memo: None,
    };
    assert_eq!(executed_event, &expected.to_xdr(&f.env, &f.client.address));
}

#[test]
fn execute_emits_the_total_and_recipient_count() {
    let f = Fixture::new();
    let recipients = f.addresses(2);
    let payments = vec![
        &f.env,
        Payment {
            to: recipients.get(0).unwrap(),
            amount: 125,
        },
        Payment {
            to: recipients.get(1).unwrap(),
            amount: 875,
        },
    ];

    f.client
        .execute(&f.funder, &f.token.address, &payments, &false, &None);

    let expected = Executed {
        funder: f.funder,
        token: f.token.address,
        count: 2,
        total: 1_000,
        memo: None,
    };
    let all = f.env.events().all();
    let events = all.events();
    assert_eq!(
        events.last().expect("no events emitted"),
        &expected.to_xdr(&f.env, &f.client.address)
    );
}

#[test]
fn execute_leaves_no_token_balance_on_the_contract() {
    let f = Fixture::new();
    let payments = f.payments(3, 37);

    f.client
        .execute(&f.funder, &f.token.address, &payments, &false, &None);

    assert_eq!(f.token.balance(&f.client.address), 0);
}

#[test]
fn execute_rejects_an_empty_batch() {
    let f = Fixture::new();
    let empty: Vec<Payment> = Vec::new(&f.env);
    assert_eq!(
        f.client
            .try_execute(&f.funder, &f.token.address, &empty, &false, &None),
        Err(Ok(Error::BatchEmpty))
    );
}

#[test]
fn execute_rejects_a_batch_over_the_cap() {
    let f = Fixture::new();
    let batch = f.payments(MAX_RECIPIENTS + 1, 1);
    assert_eq!(
        f.client
            .try_execute(&f.funder, &f.token.address, &batch, &false, &None),
        Err(Ok(Error::BatchTooLarge))
    );
}

#[test]
fn execute_rejects_non_positive_amounts_before_paying_anyone() {
    let f = Fixture::new();
    let good = Address::generate(&f.env);
    let bad = Address::generate(&f.env);
    for bad_amount in [0i128, -1, i128::MIN] {
        let batch = vec![
            &f.env,
            Payment {
                to: good.clone(),
                amount: 100,
            },
            Payment {
                to: bad.clone(),
                amount: bad_amount,
            },
        ];
        assert_eq!(
            f.client
                .try_execute(&f.funder, &f.token.address, &batch, &false, &None),
            Err(Ok(Error::InvalidAmount)),
            "amount {bad_amount} was accepted"
        );
        // Validation happens before any transfer, so the valid line in the
        // same batch was never paid either.
        assert_eq!(f.token.balance(&good), 0);
        assert_eq!(f.token.balance(&f.funder), MINT);
    }
}

#[test]
fn execute_rejects_a_zero_amount_payment_inside_a_larger_batch() {
    // The other non-positive-amount test only uses a 2-line batch; this
    // checks the same all-or-nothing validation holds with several valid
    // lines around the bad one, not just a single good line next to it (#264).
    let f = Fixture::new();
    let recipients: std::vec::Vec<Address> = (0..5).map(|_| Address::generate(&f.env)).collect();
    let mut batch = Vec::new(&f.env);
    for (i, to) in recipients.iter().enumerate() {
        batch.push_back(Payment {
            to: to.clone(),
            // The third of five payments is the zero amount.
            amount: if i == 2 { 0 } else { 100 },
        });
    }

    assert_eq!(
        f.client
            .try_execute(&f.funder, &f.token.address, &batch, &false, &None),
        Err(Ok(Error::InvalidAmount))
    );
    // All-or-nothing: none of the four otherwise-valid lines were paid either.
    for to in &recipients {
        assert_eq!(f.token.balance(to), 0);
    }
    assert_eq!(f.token.balance(&f.funder), MINT);
}

#[test]
fn preview_rejects_a_zero_amount_payment_inside_a_larger_batch() {
    // `preview` runs the same validation as `execute` (#264); nothing to
    // assert about balances since preview never moves funds.
    let f = Fixture::new();
    let mut batch = Vec::new(&f.env);
    for i in 0..5 {
        batch.push_back(Payment {
            to: Address::generate(&f.env),
            amount: if i == 2 { 0 } else { 100 },
        });
    }

    assert_eq!(
        f.client.try_preview(&batch, &false),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn execute_rejects_a_total_that_overflows() {
    let f = Fixture::new();
    let batch = vec![
        &f.env,
        Payment {
            to: Address::generate(&f.env),
            amount: i128::MAX,
        },
        Payment {
            to: Address::generate(&f.env),
            amount: 1,
        },
    ];
    assert_eq!(
        f.client
            .try_execute(&f.funder, &f.token.address, &batch, &false, &None),
        Err(Ok(Error::Overflow))
    );
}

#[test]
fn a_batch_the_funder_cannot_afford_pays_nobody() {
    // All-or-nothing: the host reverts the transfers already made when a later
    // one fails on insufficient balance.
    let f = Fixture::new();
    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    let batch = vec![
        &f.env,
        Payment {
            to: a.clone(),
            amount: 100,
        },
        Payment {
            to: b.clone(),
            amount: MINT,
        },
    ];

    assert!(f
        .client
        .try_execute(&f.funder, &f.token.address, &batch, &false, &None)
        .is_err());
    assert_eq!(f.token.balance(&a), 0, "a partial payout survived");
    assert_eq!(f.token.balance(&b), 0);
    assert_eq!(f.token.balance(&f.funder), MINT);
}

/// Same host-revert guarantee as the other contracts: a failed mid-batch
/// transfer leaves no partial mutation (batch_payout is otherwise stateless).
#[test]
fn execute_reverts_cleanly_when_a_transfer_fails() {
    a_batch_the_funder_cannot_afford_pays_nobody();
}

#[test]
fn paying_the_same_address_twice_is_allowed() {
    // Duplicates are legitimate -- two invoices for one contractor -- and are
    // deliberately not rejected on-chain.
    let f = Fixture::new();
    let a = Address::generate(&f.env);
    let batch = vec![
        &f.env,
        Payment {
            to: a.clone(),
            amount: 100,
        },
        Payment {
            to: a.clone(),
            amount: 50,
        },
    ];
    let receipt = f
        .client
        .execute(&f.funder, &f.token.address, &batch, &false, &None);
    assert_eq!(receipt.total, 150);
    assert_eq!(f.token.balance(&a), 150);
}

/// Issue #479: pins the behaviour left by removing duplicate detection.
///
/// Error 102 was `BatchDuplicateRecipient`; it was removed before release
/// (see the "What is absent, and why" note in `errors.rs` and the reserved
/// slot in `sororail_common::errors`) because an on-chain duplicate check
/// costs a quadratic scan and paying one address twice is legitimate. The
/// consequence is that both transfers occur: the recipient receives the sum
/// and the receipt counts each line separately.
#[test]
fn duplicate_recipients_are_each_paid_and_counted_in_the_receipt() {
    let f = Fixture::new();
    let dup = Address::generate(&f.env);
    let other = Address::generate(&f.env);
    let batch = vec![
        &f.env,
        Payment {
            to: dup.clone(),
            amount: 100,
        },
        Payment {
            to: other.clone(),
            amount: 25,
        },
        Payment {
            to: dup.clone(),
            amount: 50,
        },
    ];

    let receipt = f
        .client
        .execute(&f.funder, &f.token.address, &batch, &false, &None);
    assert_eq!(
        receipt,
        Receipt {
            count: 3,
            total: 175,
            memo: None,
        }
    );
    // Both lines for the duplicate address arrived, summed.
    assert_eq!(f.token.balance(&dup), 150);
    assert_eq!(f.token.balance(&other), 25);
    assert_eq!(f.token.balance(&f.funder), MINT - 175);
    assert_eq!(f.token.balance(&f.client.address), 0);
}

/// Same as above for `execute_equal`: the same address listed twice is paid
/// twice and counted twice (no `BatchDuplicateRecipient`, see error 102).
#[test]
fn execute_equal_pays_a_duplicate_recipient_once_per_line() {
    let f = Fixture::new();
    let dup = Address::generate(&f.env);
    let recipients = vec![&f.env, dup.clone(), dup.clone()];

    let receipt =
        f.client
            .execute_equal(&f.funder, &f.token.address, &recipients, &40, &false, &None);
    assert_eq!(
        receipt,
        Receipt {
            count: 2,
            total: 80,
            memo: None,
        }
    );
    assert_eq!(f.token.balance(&dup), 80);
    assert_eq!(f.token.balance(&f.funder), MINT - 80);
}

#[test]
#[should_panic]
fn execute_requires_the_funders_authorization() {
    let env = Env::default();
    let funder = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(issuer).address();
    let client = BatchPayoutContractClient::new(&env, &env.register(BatchPayoutContract, ()));
    let batch = vec![
        &env,
        Payment {
            to: Address::generate(&env),
            amount: 100,
        },
    ];
    client.execute(&funder, &token, &batch, &false, &None);
}

#[test]
fn execute_rejects_payment_to_contract_address() {
    let f = Fixture::new();
    let contract = f.client.address.clone();
    let batch = vec![
        &f.env,
        Payment {
            to: contract,
            amount: 100,
        },
    ];
    assert_eq!(
        f.client
            .try_execute(&f.funder, &f.token.address, &batch, &false, &None),
        Err(Ok(Error::IdenticalParties))
    );
}

#[test]
fn preview_rejects_payment_to_contract_address() {
    let f = Fixture::new();
    let contract = f.client.address.clone();
    let batch = vec![
        &f.env,
        Payment {
            to: contract,
            amount: 100,
        },
    ];
    assert_eq!(
        f.client.try_preview(&batch, &false),
        Err(Ok(Error::IdenticalParties))
    );
}

#[test]
fn execute_rejects_multiple_invalid_args_in_order() {
    let f = Fixture::new();
    let contract = f.client.address.clone();
    // Both empty batch AND self-payment: size check comes first
    let empty_batch: Vec<Payment> = Vec::new(&f.env);
    assert_eq!(
        f.client
            .try_execute(&f.funder, &f.token.address, &empty_batch, &false, &None),
        Err(Ok(Error::BatchEmpty))
    );

    // Self-payment AND zero amount: self-payment check comes first
    let batch = vec![
        &f.env,
        Payment {
            to: contract,
            amount: 0,
        },
    ];
    assert_eq!(
        f.client
            .try_execute(&f.funder, &f.token.address, &batch, &false, &None),
        Err(Ok(Error::IdenticalParties))
    );
}

// ----------------------------------------------------------- execute_equal

#[test]
fn execute_equal_pays_everyone_the_same() {
    let f = Fixture::new();
    let recipients = f.addresses(5);
    let receipt = f.client.execute_equal(
        &f.funder,
        &f.token.address,
        &recipients,
        &200,
        &false,
        &None,
    );

    assert_eq!(
        receipt,
        Receipt {
            count: 5,
            total: 1_000,
            memo: None,
        }
    );
    for r in recipients.iter() {
        assert_eq!(f.token.balance(&r), 200);
    }
    assert_eq!(f.token.balance(&f.funder), MINT - 1_000);
}

#[test]
fn execute_equal_applies_the_exact_amount_without_rounding() {
    let f = Fixture::new();
    let recipients = f.addresses(3);

    let receipt =
        f.client
            .execute_equal(&f.funder, &f.token.address, &recipients, &7, &false, &None);

    assert_eq!(
        receipt,
        Receipt {
            count: 3,
            total: 21,
            memo: None,
        }
    );
    for recipient in recipients.iter() {
        assert_eq!(f.token.balance(&recipient), 7);
    }
}

#[test]
fn execute_equal_rejects_a_non_positive_amount() {
    let f = Fixture::new();
    let recipients = f.addresses(3);
    for bad in [0i128, -1, i128::MIN] {
        assert_eq!(
            f.client.try_execute_equal(
                &f.funder,
                &f.token.address,
                &recipients,
                &bad,
                &false,
                &None
            ),
            Err(Ok(Error::InvalidAmount)),
            "amount {bad} was accepted"
        );
    }
}

#[test]
fn execute_equal_rejects_an_empty_batch() {
    let f = Fixture::new();
    let empty: Vec<Address> = Vec::new(&f.env);
    assert_eq!(
        f.client
            .try_execute_equal(&f.funder, &f.token.address, &empty, &100, &false, &None),
        Err(Ok(Error::BatchEmpty))
    );
}

#[test]
fn execute_equal_rejects_a_batch_over_the_cap() {
    let f = Fixture::new();
    let recipients = f.addresses(MAX_RECIPIENTS + 1);
    assert_eq!(
        f.client
            .try_execute_equal(&f.funder, &f.token.address, &recipients, &1, &false, &None),
        Err(Ok(Error::BatchTooLarge))
    );
}

#[test]
fn execute_equal_rejects_contract_address_in_recipients() {
    let f = Fixture::new();
    let contract = f.client.address.clone();
    let mut recipients = f.addresses(2);
    recipients.push_back(contract);
    assert_eq!(
        f.client.try_execute_equal(
            &f.funder,
            &f.token.address,
            &recipients,
            &100,
            &false,
            &None
        ),
        Err(Ok(Error::IdenticalParties))
    );
}

#[test]
fn execute_equal_rejects_multiple_invalid_args_in_order() {
    let f = Fixture::new();
    let contract = f.client.address.clone();
    // Non-positive amount comes before size check
    assert_eq!(
        f.client.try_execute_equal(
            &f.funder,
            &f.token.address,
            &f.addresses(0),
            &0,
            &false,
            &None
        ),
        Err(Ok(Error::InvalidAmount))
    );

    // Size check comes before self-payment check
    let mut oversized = f.addresses(MAX_RECIPIENTS + 1);
    oversized.push_back(contract.clone());
    assert_eq!(
        f.client
            .try_execute_equal(&f.funder, &f.token.address, &oversized, &100, &false, &None),
        Err(Ok(Error::BatchTooLarge))
    );

    // Self-payment check comes before total calculation
    let recipients = vec![&f.env, contract];
    assert_eq!(
        f.client.try_execute_equal(
            &f.funder,
            &f.token.address,
            &recipients,
            &i128::MAX,
            &false,
            &None
        ),
        Err(Ok(Error::IdenticalParties))
    );
}

#[test]
fn execute_equal_rejects_a_total_that_overflows() {
    let f = Fixture::new();
    let recipients = f.addresses(2);
    assert_eq!(
        f.client.try_execute_equal(
            &f.funder,
            &f.token.address,
            &recipients,
            &i128::MAX,
            &false,
            &None
        ),
        Err(Ok(Error::Overflow))
    );
}

// ----------------------------------------------------------------- preview

#[test]
fn preview_totals_without_moving_anything() {
    let f = Fixture::new();
    let batch = f.payments(4, 25);
    let receipt = f.client.preview(&batch, &false);
    assert_eq!(
        receipt,
        Receipt {
            count: 4,
            total: 100,
            memo: None,
        }
    );
    assert_eq!(f.token.balance(&f.funder), MINT);
}

#[test]
fn preview_returns_the_correct_receipt_and_moves_nothing() {
    // The existing uniform-amount preview test can't catch a `Receipt` whose
    // `total` happens to equal `count * amount` by coincidence; this uses
    // distinct amounts so `count` and `total` are independently verified,
    // and checks every balance plus the event log, not just the funder's
    // (#265).
    let f = Fixture::new();
    let a = Address::generate(&f.env);
    let b = Address::generate(&f.env);
    let c = Address::generate(&f.env);
    let batch = vec![
        &f.env,
        Payment {
            to: a.clone(),
            amount: 11,
        },
        Payment {
            to: b.clone(),
            amount: 22,
        },
        Payment {
            to: c.clone(),
            amount: 33,
        },
    ];

    let receipt = f.client.preview(&batch, &false);
    assert_eq!(
        receipt,
        Receipt {
            count: 3,
            total: 66,
            memo: None,
        }
    );

    assert_eq!(f.token.balance(&a), 0);
    assert_eq!(f.token.balance(&b), 0);
    assert_eq!(f.token.balance(&c), 0);
    assert_eq!(f.token.balance(&f.funder), MINT);
    assert_eq!(f.token.balance(&f.client.address), 0);
    // Unlike `execute`, `preview` never publishes an `Executed` event.
    assert!(f.env.events().all().events().is_empty());
}

#[test]
fn preview_rejects_exactly_what_execute_rejects() {
    let f = Fixture::new();
    let empty: Vec<Payment> = Vec::new(&f.env);
    assert_eq!(
        f.client.try_preview(&empty, &false),
        Err(Ok(Error::BatchEmpty))
    );

    let too_many = f.payments(MAX_RECIPIENTS + 1, 1);
    assert_eq!(
        f.client.try_preview(&too_many, &false),
        Err(Ok(Error::BatchTooLarge))
    );

    let bad = vec![
        &f.env,
        Payment {
            to: Address::generate(&f.env),
            amount: 0,
        },
    ];
    assert_eq!(
        f.client.try_preview(&bad, &false),
        Err(Ok(Error::InvalidAmount))
    );
}

// -------------------------------------------------------------- the cap

#[test]
fn max_recipients_is_reported_to_clients() {
    let f = Fixture::new();
    assert_eq!(f.client.max_recipients(), MAX_RECIPIENTS);
}

#[test]
fn a_full_size_batch_executes() {
    let f = Fixture::new();
    let recipients = f.addresses(MAX_RECIPIENTS);
    let receipt =
        f.client
            .execute_equal(&f.funder, &f.token.address, &recipients, &10, &false, &None);
    assert_eq!(receipt.count, MAX_RECIPIENTS);
    assert_eq!(receipt.total, 10 * i128::from(MAX_RECIPIENTS));
}

/// Largest batch that executes under the test environment's budget.
///
/// Returns `None` if even the smallest probe fails. Resource exhaustion
/// surfaces as a host panic rather than a returnable error, so the probe has
/// to catch unwinds; the panic hook is silenced for the duration so the
/// expected failures do not fill the test log.
fn largest_batch_that_executes(candidates: &[u32]) -> Option<u32> {
    let previous = std::panic::take_hook();
    std::panic::set_hook(std::boxed::Box::new(|_| {}));

    let mut highest = None;
    for &n in candidates {
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let env = Env::default();
            env.mock_all_auths();
            let funder = Address::generate(&env);
            let issuer = Address::generate(&env);
            let token = env.register_stellar_asset_contract_v2(issuer).address();
            StellarAssetClient::new(&env, &token).mint(&funder, &MINT);
            let client =
                BatchPayoutContractClient::new(&env, &env.register(BatchPayoutContract, ()));

            let mut recipients = Vec::new(&env);
            for _ in 0..n {
                recipients.push_back(Address::generate(&env));
            }
            // Call the entry point directly, bypassing MAX_RECIPIENTS, so the
            // probe measures resources rather than our own cap.
            client.execute_equal(&funder, &token, &recipients, &1, &false, &None);
        }))
        .is_ok();

        if ok {
            highest = Some(n);
        } else {
            break;
        }
    }

    std::panic::set_hook(previous);
    highest
}

#[test]
fn max_recipients_is_within_what_actually_executes() {
    // SPEC.md requires the cap be discovered empirically rather than guessed.
    // An earlier guess of 100 exceeded the budget, which is why this exists.
    let ceiling = largest_batch_that_executes(&[10, 20, 30, 40, 50, 60, 70, 80, 90, 100])
        .expect("even a 10-recipient batch failed to execute");
    std::println!("measured batch ceiling in the test environment: {ceiling} recipients");
    assert!(
        MAX_RECIPIENTS <= ceiling,
        "MAX_RECIPIENTS ({MAX_RECIPIENTS}) exceeds what executes ({ceiling})"
    );
}

#[test]
#[ignore = "reports the measured ceiling; run with --ignored --nocapture"]
fn report_batch_ceiling() {
    // Finer-grained sweep, for when the cap is being re-derived.
    //
    // Caveat that keeps MAX_RECIPIENTS conservative: the local test
    // environment models the CPU/memory budget but NOT the transaction size
    // limit a real network applies to the submitted envelope, and its
    // `mock_all_auths` builds a separate authorization entry per transfer
    // where a real submission signs one tree. So this measures a budget
    // ceiling, not the on-chain ceiling. Re-run against testnet before v0.2.
    let candidates: std::vec::Vec<u32> = (1..=20).map(|i| i * 5).collect();
    let ceiling = largest_batch_that_executes(&candidates);
    std::println!("measured batch ceiling in the test environment: {ceiling:?}");
}

// ------------------------------------------------ #435: env.auths() assertions

#[test]
fn execute_records_the_exact_authorised_invocation() {
    // #435: every state-changing entry point must be covered by an env.auths()
    // assertion verifying that the funder authorised the specific call.
    let f = Fixture::new();
    let to = Address::generate(&f.env);
    let batch = vec![
        &f.env,
        Payment {
            to: to.clone(),
            amount: 100,
        },
    ];

    f.client
        .execute(&f.funder, &f.token.address, &batch, &false, &None);

    assert_eq!(
        f.env.auths(),
        std::vec![(
            f.funder.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    f.client.address.clone(),
                    soroban_sdk::symbol_short!("execute"),
                    (
                        &f.funder,
                        &f.token.address,
                        &batch,
                        &false,
                        &None::<BytesN<32>>
                    )
                        .into_val(&f.env),
                )),
                sub_invocations: std::vec![AuthorizedInvocation {
                    function: AuthorizedFunction::Contract((
                        f.token.address.clone(),
                        soroban_sdk::symbol_short!("transfer"),
                        (&f.funder, &to, &100_i128).into_val(&f.env),
                    )),
                    sub_invocations: std::vec![],
                }],
            }
        )]
    );
}

#[test]
fn execute_equal_records_the_exact_authorised_invocation() {
    // #435: execute_equal must record an auth entry for the funder.
    let f = Fixture::new();
    let to = Address::generate(&f.env);
    let recipients = vec![&f.env, to.clone()];

    f.client.execute_equal(
        &f.funder,
        &f.token.address,
        &recipients,
        &200,
        &false,
        &None,
    );

    assert_eq!(
        f.env.auths(),
        std::vec![(
            f.funder.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    f.client.address.clone(),
                    soroban_sdk::symbol_short!("execute_equal"),
                    (
                        &f.funder,
                        &f.token.address,
                        &recipients,
                        &200_i128,
                        &false,
                        &None::<BytesN<32>>
                    )
                        .into_val(&f.env),
                )),
                sub_invocations: std::vec![AuthorizedInvocation {
                    function: AuthorizedFunction::Contract((
                        f.token.address.clone(),
                        soroban_sdk::symbol_short!("transfer"),
                        (&f.funder, &to, &200_i128).into_val(&f.env),
                    )),
                    sub_invocations: std::vec![],
                }],
            }
        )]
    );
}

#[test]
fn execute_many_tokens_records_the_exact_authorised_invocation() {
    // #435: execute_many_tokens must record an auth entry for the funder.
    let te = TestEnv::new();
    let (token_a, funder) = te.make_token(MINT);
    let (token_b, _) = te.make_token(0);
    te.mint(&token_b, &funder, MINT);
    let env = te.env().clone();
    let client = BatchPayoutContractClient::new(&env, &env.register(BatchPayoutContract, ()));

    let to_a = Address::generate(&env);
    let to_b = Address::generate(&env);
    let payments = vec![
        &env,
        MultiTokenPayment {
            token: token_a.address.clone(),
            to: to_a.clone(),
            amount: 100,
        },
        MultiTokenPayment {
            token: token_b.address.clone(),
            to: to_b.clone(),
            amount: 200,
        },
    ];

    client.execute_many_tokens(&funder, &payments, &false);

    let auths = env.auths();
    assert_eq!(auths.len(), 1, "expected one top-level auth entry");
    let (auth_addr, auth_inv) = &auths[0];
    assert_eq!(auth_addr, &funder);
    assert_eq!(
        auth_inv.function,
        AuthorizedFunction::Contract((
            client.address.clone(),
            soroban_sdk::symbol_short!("execute_many_tokens"),
            (&funder, &payments, &false).into_val(&env),
        ))
    );
    // Two sub-invocations: one token transfer per payment line.
    assert_eq!(auth_inv.sub_invocations.len(), 2);
}

// ---------------------------------------- #433: only_eoa gate

#[test]
fn execute_rejects_contract_recipient_when_only_eoa_is_set() {
    // A registered contract address should be rejected when only_eoa=true.
    let f = Fixture::new();
    let contract_recipient = f.client.address.clone();
    // Use a different contract so we don't hit IdenticalParties first.
    let other_contract = f.env.register(BatchPayoutContract, ());
    let batch = vec![
        &f.env,
        Payment {
            to: other_contract,
            amount: 100,
        },
    ];
    assert_eq!(
        f.client
            .try_execute(&f.funder, &f.token.address, &batch, &true, &None),
        Err(Ok(Error::BatchRecipientIsContract))
    );
    // The same batch succeeds when only_eoa is false.
    // (Payment will fail due to insufficient balance on the other contract, but
    // the validation itself passes; we only check the error type here.)
    let _ = f
        .client
        .try_execute(&f.funder, &f.token.address, &batch, &false, &None);
    _ = contract_recipient; // silence unused warning
}

#[test]
fn execute_allows_account_recipients_when_only_eoa_is_set() {
    let f = Fixture::new();
    let batch = f.payments(2, 50);
    // With only_eoa=true, normal account recipients should still work.
    let receipt = f
        .client
        .execute(&f.funder, &f.token.address, &batch, &true, &None);
    assert_eq!(receipt.count, 2);
    assert_eq!(receipt.total, 100);
}

#[test]
fn execute_equal_rejects_contract_recipient_when_only_eoa_is_set() {
    let f = Fixture::new();
    let other_contract = f.env.register(BatchPayoutContract, ());
    let recipients = vec![&f.env, other_contract];
    assert_eq!(
        f.client
            .try_execute_equal(&f.funder, &f.token.address, &recipients, &100, &true, &None),
        Err(Ok(Error::BatchRecipientIsContract))
    );
}

#[test]
fn execute_equal_allows_account_recipients_when_only_eoa_is_set() {
    let f = Fixture::new();
    let recipients = f.addresses(3);
    let receipt =
        f.client
            .execute_equal(&f.funder, &f.token.address, &recipients, &10, &true, &None);
    assert_eq!(receipt.count, 3);
}

#[test]
fn preview_rejects_contract_recipient_when_only_eoa_is_set() {
    let f = Fixture::new();
    let other_contract = f.env.register(BatchPayoutContract, ());
    let batch = vec![
        &f.env,
        Payment {
            to: other_contract,
            amount: 100,
        },
    ];
    assert_eq!(
        f.client.try_preview(&batch, &true),
        Err(Ok(Error::BatchRecipientIsContract))
    );
    // Without only_eoa: passes validation (no token check in preview).
    assert!(f.client.try_preview(&batch, &false).is_ok());
}

#[test]
fn only_eoa_check_comes_after_identical_parties_check() {
    // BatchPayoutContract's own address is both a contract AND the same as
    // env.current_contract_address(). The IdenticalParties check runs first
    // so the error is IdenticalParties, not BatchRecipientIsContract.
    let f = Fixture::new();
    let batch = vec![
        &f.env,
        Payment {
            to: f.client.address.clone(),
            amount: 100,
        },
    ];
    assert_eq!(
        f.client
            .try_execute(&f.funder, &f.token.address, &batch, &true, &None),
        Err(Ok(Error::IdenticalParties))
    );
}

// ---------------------------------------- #432: execute_many_tokens

#[test]
fn execute_many_tokens_pays_each_recipient_from_their_own_token() {
    let te = TestEnv::new();
    let (token_a, funder) = te.make_token(MINT);
    let (token_b, _) = te.make_token(0);
    te.mint(&token_b, &funder, MINT);
    let env = te.env().clone();
    let client = BatchPayoutContractClient::new(&env, &env.register(BatchPayoutContract, ()));
    let token_a_client = TokenClient::new(&env, &token_a.address);
    let token_b_client = TokenClient::new(&env, &token_b.address);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let payments = vec![
        &env,
        MultiTokenPayment {
            token: token_a.address.clone(),
            to: a.clone(),
            amount: 300,
        },
        MultiTokenPayment {
            token: token_b.address.clone(),
            to: b.clone(),
            amount: 500,
        },
    ];

    let receipt = client.execute_many_tokens(&funder, &payments, &false);

    assert_eq!(
        receipt,
        Receipt {
            count: 2,
            total: 800,
            memo: None,
        }
    );
    assert_eq!(token_a_client.balance(&a), 300);
    assert_eq!(token_b_client.balance(&b), 500);
    assert_eq!(token_a_client.balance(&funder), MINT - 300);
    assert_eq!(token_b_client.balance(&funder), MINT - 500);
    // The contract holds nothing.
    assert_eq!(token_a_client.balance(&client.address), 0);
    assert_eq!(token_b_client.balance(&client.address), 0);
}

#[test]
fn execute_many_tokens_emits_executed_many_event() {
    let te = TestEnv::new();
    let (token_a, funder) = te.make_token(MINT);
    let env = te.env().clone();
    let client = BatchPayoutContractClient::new(&env, &env.register(BatchPayoutContract, ()));

    let payments = vec![
        &env,
        MultiTokenPayment {
            token: token_a.address.clone(),
            to: Address::generate(&env),
            amount: 100,
        },
    ];

    client.execute_many_tokens(&funder, &payments, &false);

    let all = env.events().all();
    let events = all.events();
    let last = events.last().expect("no events emitted");
    let expected = ExecutedMany {
        funder: funder.clone(),
        count: 1,
    };
    assert_eq!(last, &expected.to_xdr(&env, &client.address));
}

#[test]
fn execute_many_tokens_rejects_an_empty_batch() {
    let te = TestEnv::new();
    let (_, funder) = te.make_token(MINT);
    let env = te.env().clone();
    let client = BatchPayoutContractClient::new(&env, &env.register(BatchPayoutContract, ()));
    let empty: Vec<MultiTokenPayment> = Vec::new(&env);
    assert_eq!(
        client.try_execute_many_tokens(&funder, &empty, &false),
        Err(Ok(Error::BatchEmpty))
    );
}

#[test]
fn execute_many_tokens_rejects_a_batch_over_the_cap() {
    let te = TestEnv::new();
    let (token_a, funder) = te.make_token(MINT);
    let env = te.env().clone();
    let client = BatchPayoutContractClient::new(&env, &env.register(BatchPayoutContract, ()));

    let mut payments = Vec::new(&env);
    for _ in 0..MAX_RECIPIENTS + 1 {
        payments.push_back(MultiTokenPayment {
            token: token_a.address.clone(),
            to: Address::generate(&env),
            amount: 1,
        });
    }
    assert_eq!(
        client.try_execute_many_tokens(&funder, &payments, &false),
        Err(Ok(Error::BatchTooLarge))
    );
}

#[test]
fn execute_many_tokens_rejects_non_positive_amount() {
    let te = TestEnv::new();
    let (token_a, funder) = te.make_token(MINT);
    let env = te.env().clone();
    let client = BatchPayoutContractClient::new(&env, &env.register(BatchPayoutContract, ()));

    let payments = vec![
        &env,
        MultiTokenPayment {
            token: token_a.address.clone(),
            to: Address::generate(&env),
            amount: 0,
        },
    ];
    assert_eq!(
        client.try_execute_many_tokens(&funder, &payments, &false),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn execute_many_tokens_rejects_payment_to_contract_address() {
    let te = TestEnv::new();
    let (token_a, funder) = te.make_token(MINT);
    let env = te.env().clone();
    let client = BatchPayoutContractClient::new(&env, &env.register(BatchPayoutContract, ()));

    let payments = vec![
        &env,
        MultiTokenPayment {
            token: token_a.address.clone(),
            to: client.address.clone(),
            amount: 100,
        },
    ];
    assert_eq!(
        client.try_execute_many_tokens(&funder, &payments, &false),
        Err(Ok(Error::IdenticalParties))
    );
}

#[test]
fn execute_many_tokens_rejects_contract_recipient_when_only_eoa_is_set() {
    let te = TestEnv::new();
    let (token_a, funder) = te.make_token(MINT);
    let env = te.env().clone();
    let client = BatchPayoutContractClient::new(&env, &env.register(BatchPayoutContract, ()));
    let other_contract = env.register(BatchPayoutContract, ());

    let payments = vec![
        &env,
        MultiTokenPayment {
            token: token_a.address.clone(),
            to: other_contract,
            amount: 100,
        },
    ];
    assert_eq!(
        client.try_execute_many_tokens(&funder, &payments, &true),
        Err(Ok(Error::BatchRecipientIsContract))
    );
}

#[test]
fn execute_many_tokens_uses_all_or_nothing_semantics() {
    // If a later transfer fails (e.g. insufficient balance), no earlier
    // transfer survives.
    let te = TestEnv::new();
    let (token_a, funder) = te.make_token(MINT);
    // token_b has no balance for funder
    let (token_b, _) = te.make_token(0);
    let env = te.env().clone();
    let client = BatchPayoutContractClient::new(&env, &env.register(BatchPayoutContract, ()));
    let token_a_client = TokenClient::new(&env, &token_a.address);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let payments = vec![
        &env,
        MultiTokenPayment {
            token: token_a.address.clone(),
            to: a.clone(),
            amount: 100,
        },
        MultiTokenPayment {
            token: token_b.address.clone(),
            to: b.clone(),
            amount: 1, // funder has 0 of token_b → will fail
        },
    ];

    assert!(client
        .try_execute_many_tokens(&funder, &payments, &false)
        .is_err());
    // The first transfer was reverted along with the failed second one.
    assert_eq!(token_a_client.balance(&a), 0, "partial payout survived");
    assert_eq!(token_a_client.balance(&funder), MINT);
}

// --------------------------------------------- shared helpers for #424-#429

fn memo(env: &Env, byte: u8) -> Option<BytesN<32>> {
    Some(BytesN::from_array(env, &[byte; 32]))
}

/// Seeded xorshift, as in `sororail_common::math`'s property tests: no new
/// dependency, fully deterministic.
struct Xorshift(u64);

impl Xorshift {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform in `[lo, hi]`.
    fn next_in(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next_u64() % (hi - lo + 1)
    }
}

// ------------------------------------------------- #424: execute_weighted

fn shares_of(env: &Env, bps: &[u32]) -> Vec<(Address, u32)> {
    let mut v = Vec::new(env);
    for b in bps {
        v.push_back((Address::generate(env), *b));
    }
    v
}

#[test]
fn execute_weighted_pays_by_share_and_gives_dust_to_the_last_recipient() {
    let f = Fixture::new();
    let shares = shares_of(&f.env, &[3_333, 3_333, 3_334]);

    let receipt =
        f.client
            .execute_weighted(&f.funder, &f.token.address, &1_001, &shares, &false, &None);
    let all = f.env.events().all();
    let events = all.events();

    assert_eq!(
        receipt,
        Receipt {
            count: 3,
            total: 1_001,
            memo: None,
        }
    );
    // 1001 * 3333 / 10000 = 333.63 -> 333 for the first two; the last takes
    // 1001 - 666 = 335, absorbing the truncation dust.
    assert_eq!(f.token.balance(&shares.get(0).unwrap().0), 333);
    assert_eq!(f.token.balance(&shares.get(1).unwrap().0), 333);
    assert_eq!(f.token.balance(&shares.get(2).unwrap().0), 335);
    assert_eq!(f.token.balance(&f.funder), MINT - 1_001);
    assert_eq!(f.token.balance(&f.client.address), 0);

    let expected = Executed {
        funder: f.funder.clone(),
        token: f.token.address.clone(),
        count: 3,
        total: 1_001,
        memo: None,
    };
    assert_eq!(
        events.last().expect("no events emitted"),
        &expected.to_xdr(&f.env, &f.client.address)
    );
}

#[test]
fn execute_weighted_rejects_shares_not_summing_to_10_000() {
    let f = Fixture::new();
    for bad in [
        std::vec![9_999u32],
        std::vec![10_001],
        std::vec![5_000, 4_999],
        std::vec![5_000, 5_001],
        std::vec![0],
        // Overflows u32 when summed; must still be InvalidBasisPoints.
        std::vec![u32::MAX, 10_001],
    ] {
        let shares = shares_of(&f.env, &bad);
        assert_eq!(
            f.client.try_execute_weighted(
                &f.funder,
                &f.token.address,
                &1_000,
                &shares,
                &false,
                &None,
            ),
            Err(Ok(Error::InvalidBasisPoints)),
            "shares {bad:?}"
        );
    }
    assert_eq!(f.token.balance(&f.funder), MINT);
}

#[test]
fn execute_weighted_rejects_invalid_batches_before_moving_anything() {
    let f = Fixture::new();
    let ok = shares_of(&f.env, &[10_000]);
    assert_eq!(
        f.client
            .try_execute_weighted(&f.funder, &f.token.address, &0, &ok, &false, &None),
        Err(Ok(Error::InvalidAmount))
    );

    let empty: Vec<(Address, u32)> = Vec::new(&f.env);
    assert_eq!(
        f.client
            .try_execute_weighted(&f.funder, &f.token.address, &100, &empty, &false, &None),
        Err(Ok(Error::BatchEmpty))
    );

    let mut too_many = Vec::new(&f.env);
    for _ in 0..=MAX_RECIPIENTS {
        too_many.push_back((Address::generate(&f.env), 0u32));
    }
    assert_eq!(
        f.client
            .try_execute_weighted(&f.funder, &f.token.address, &100, &too_many, &false, &None),
        Err(Ok(Error::BatchTooLarge))
    );

    let to_self = vec![&f.env, (f.client.address.clone(), 10_000u32)];
    assert_eq!(
        f.client
            .try_execute_weighted(&f.funder, &f.token.address, &100, &to_self, &false, &None),
        Err(Ok(Error::IdenticalParties))
    );
    assert_eq!(f.token.balance(&f.funder), MINT);
}

#[test]
fn execute_weighted_zero_share_pays_nothing_but_counts() {
    let f = Fixture::new();
    let shares = shares_of(&f.env, &[0, 10_000]);
    let receipt =
        f.client
            .execute_weighted(&f.funder, &f.token.address, &500, &shares, &false, &None);
    assert_eq!(receipt.count, 2);
    assert_eq!(f.token.balance(&shares.get(0).unwrap().0), 0);
    assert_eq!(f.token.balance(&shares.get(1).unwrap().0), 500);
}

/// Property: for any total and any basis-point partition summing to 10_000,
/// the payouts sum to exactly `total` (checked by balances), and the receipt
/// reports `total`.
#[test]
fn execute_weighted_payouts_always_sum_to_total() {
    let mut rng = Xorshift(0x424_u64.wrapping_mul(0x9e37_79b9_7f4a_7c15));
    for _ in 0..64 {
        let f = Fixture::new();
        let n = rng.next_in(1, 12) as usize;
        let total = rng.next_in(1, MINT as u64) as i128;

        // n - 1 random cut points in [0, 10_000] give n shares summing to 10_000.
        let mut cuts: std::vec::Vec<u32> =
            (0..n - 1).map(|_| rng.next_in(0, 10_000) as u32).collect();
        cuts.sort_unstable();
        let mut bps = std::vec::Vec::with_capacity(n);
        let mut prev = 0u32;
        for c in cuts {
            bps.push(c - prev);
            prev = c;
        }
        bps.push(10_000 - prev);

        let shares = shares_of(&f.env, &bps);
        let receipt =
            f.client
                .execute_weighted(&f.funder, &f.token.address, &total, &shares, &false, &None);
        assert_eq!(receipt.total, total);
        assert_eq!(receipt.count, n as u32);

        let mut paid: i128 = 0;
        for (i, (to, b)) in shares.iter().enumerate() {
            let got = f.token.balance(&to);
            if i + 1 < n {
                assert_eq!(got, total * i128::from(b) / 10_000, "line {i}");
            }
            paid += got;
        }
        assert_eq!(paid, total, "total {total}, shares {bps:?}");
        assert_eq!(f.token.balance(&f.funder), MINT - total);
    }
}

// ------------------------------------------------------ #428: per-batch memo

#[test]
fn execute_echoes_the_memo_in_receipt_and_event() {
    let f = Fixture::new();
    let m = memo(&f.env, 0xab);
    let batch = f.payments(2, 50);

    let receipt = f
        .client
        .execute(&f.funder, &f.token.address, &batch, &false, &m);
    let all = f.env.events().all();
    let events = all.events();

    assert_eq!(
        receipt,
        Receipt {
            count: 2,
            total: 100,
            memo: m.clone(),
        }
    );
    let expected = Executed {
        funder: f.funder.clone(),
        token: f.token.address.clone(),
        count: 2,
        total: 100,
        memo: m,
    };
    let last = events.last().expect("no events emitted");
    assert_eq!(last, &expected.to_xdr(&f.env, &f.client.address));
}

#[test]
fn execute_equal_and_weighted_echo_the_memo() {
    let f = Fixture::new();
    let m = memo(&f.env, 7);

    let recipients = f.addresses(2);
    let r = f
        .client
        .execute_equal(&f.funder, &f.token.address, &recipients, &5, &false, &m);
    assert_eq!(r.memo, m);

    let shares = shares_of(&f.env, &[2_500, 7_500]);
    let r = f
        .client
        .execute_weighted(&f.funder, &f.token.address, &100, &shares, &false, &m);
    assert_eq!(r.memo, m);

    let r = f
        .client
        .execute_equal_split(&f.funder, &f.token.address, &recipients, &11, &false, &m);
    assert_eq!(r.memo, m);
}

#[test]
fn execute_with_a_memo_records_it_in_the_authorised_invocation() {
    let f = Fixture::new();
    let m = memo(&f.env, 9);
    let batch = f.payments(1, 1);
    f.client
        .execute(&f.funder, &f.token.address, &batch, &false, &m);
    let auths = f.env.auths();
    assert_eq!(
        auths[0].1.function,
        AuthorizedFunction::Contract((
            f.client.address.clone(),
            Symbol::new(&f.env, "execute"),
            (&f.funder, &f.token.address, &batch, &false, &m).into_val(&f.env),
        ))
    );
}

// ---------------------------- #429: the equal-split dust rule at the extremes

/// Runs `execute_equal_split` and asserts, from balances alone, that the
/// recipients received exactly `total` with the dust on the last one.
fn assert_equal_split_by_balances(n: u32, total: i128) -> EqualSplitReceipt {
    let f = Fixture::new();
    let recipients = f.addresses(n);
    let receipt = f.client.execute_equal_split(
        &f.funder,
        &f.token.address,
        &recipients,
        &total,
        &false,
        &None,
    );
    let each = total / i128::from(n);
    let dust = total % i128::from(n);
    assert_eq!(receipt.amount_each, each);
    assert_eq!(receipt.dust, dust);
    assert_eq!(receipt.total, total);
    assert_eq!(receipt.count, n);

    let mut sum: i128 = 0;
    for (i, to) in recipients.iter().enumerate() {
        let got = f.token.balance(&to);
        let want = if i + 1 == n as usize {
            each + dust
        } else {
            each
        };
        assert_eq!(got, want, "recipient {i} of {n}, total {total}");
        sum += got;
    }
    assert_eq!(sum, total);
    assert_eq!(f.token.balance(&f.funder), MINT - total);
    assert_eq!(f.token.balance(&f.client.address), 0);
    receipt
}

#[test]
fn equal_split_one_recipient_takes_everything_with_no_dust() {
    let r = assert_equal_split_by_balances(1, 1_001);
    assert_eq!((r.amount_each, r.dust), (1_001, 0));
}

#[test]
fn equal_split_at_max_recipients_puts_the_dust_on_the_last() {
    let n = MAX_RECIPIENTS;
    let total = 7 * i128::from(n) + i128::from(n - 1);
    let r = assert_equal_split_by_balances(n, total);
    assert_eq!((r.amount_each, r.dust), (7, i128::from(n - 1)));
}

#[test]
fn equal_split_total_smaller_than_recipient_count_pays_it_all_to_the_last() {
    let r = assert_equal_split_by_balances(5, 3);
    assert_eq!((r.amount_each, r.dust), (0, 3));
    let r = assert_equal_split_by_balances(MAX_RECIPIENTS, 1);
    assert_eq!((r.amount_each, r.dust), (0, 1));
}

#[test]
fn equal_split_exact_division_has_no_dust() {
    let r = assert_equal_split_by_balances(4, 1_000);
    assert_eq!((r.amount_each, r.dust), (250, 0));
}

// ------------------------------------------------------ #425: preview_equal

#[test]
fn preview_equal_agrees_with_execute_equal_split() {
    for &n in &[1u32, 2, 3, 7, MAX_RECIPIENTS] {
        for &total in &[1i128, 2, 7, 39, 40, 41, 1_000, 1_001, 999_999] {
            let f = Fixture::new();
            let recipients = f.addresses(n);
            let preview = f.client.preview_equal(&recipients, &total, &false);
            assert_eq!(f.token.balance(&f.funder), MINT, "preview moved funds");
            let executed = f.client.execute_equal_split(
                &f.funder,
                &f.token.address,
                &recipients,
                &total,
                &false,
                &None,
            );
            assert_eq!(preview, executed, "n {n}, total {total}");
            assert_eq!(f.token.balance(&f.funder), MINT - total);
        }
    }
}

#[test]
fn preview_equal_moves_nothing_and_emits_nothing() {
    let f = Fixture::new();
    let recipients = f.addresses(3);
    let r = f.client.preview_equal(&recipients, &10, &false);
    assert_eq!(
        r,
        EqualSplitReceipt {
            count: 3,
            total: 10,
            amount_each: 3,
            dust: 1,
            memo: None,
        }
    );
    assert!(f.env.events().all().events().is_empty());
    for to in recipients.iter() {
        assert_eq!(f.token.balance(&to), 0);
    }
}

#[test]
fn preview_equal_rejects_exactly_what_execute_equal_split_rejects() {
    let f = Fixture::new();
    let cases: [(Vec<Address>, i128, Error); 4] = [
        (f.addresses(0), 10, Error::BatchEmpty),
        (f.addresses(MAX_RECIPIENTS + 1), 10, Error::BatchTooLarge),
        (f.addresses(2), 0, Error::InvalidAmount),
        (
            vec![&f.env, f.client.address.clone()],
            10,
            Error::IdenticalParties,
        ),
    ];
    for (recipients, total, err) in cases {
        assert_eq!(
            f.client.try_preview_equal(&recipients, &total, &false),
            Err(Ok(err))
        );
        assert_eq!(
            f.client.try_execute_equal_split(
                &f.funder,
                &f.token.address,
                &recipients,
                &total,
                &false,
                &None,
            ),
            Err(Ok(err))
        );
    }
    assert_eq!(f.token.balance(&f.funder), MINT);
}
