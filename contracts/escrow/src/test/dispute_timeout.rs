// dispute timeout

use super::*;

// ------------------------------------------------------- dispute timeout

const TIMEOUT: u64 = 1_000;
/// Ledger time at which the dispute is raised. Well after `START_TS` and well
/// before `DEADLINE`, so the timeout is measured from the dispute and not from
/// `init` or the deadline.
const DISPUTED_AT: u64 = START_TS + 5_000;

impl Fixture<'_> {
    /// Funded, then disputed at `DISPUTED_AT`, with `TIMEOUT` as the dispute
    /// timeout.
    fn disputed() -> Self {
        let f = Fixture::with_dispute_timeout(TIMEOUT);
        f.client.fund();
        f.env.ledger().with_mut(|l| l.timestamp = DISPUTED_AT);
        f.client.dispute(&f.depositor);
        f
    }
}

#[test]
fn init_stores_the_dispute_timeout_and_leaves_disputed_at_unset() {
    let f = Fixture::with_dispute_timeout(TIMEOUT);
    let e = f.client.get();
    assert_eq!(e.dispute_timeout, Some(TIMEOUT));
    assert_eq!(e.disputed_at, None);
}

#[test]
fn init_rejects_a_zero_dispute_timeout() {
    let env = TestEnv::at(START_TS).env().clone();
    env.mock_all_auths();
    let depositor = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token = Address::generate(&env);
    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));

    assert_eq!(
        client.try_init(
            &depositor,
            &beneficiary,
            &Some(arbiter),
            &token,
            &AMOUNT,
            &DEADLINE,
            &Some(0),
        ),
        Err(Ok(Error::InvalidDuration))
    );
    // The rejected attempt does not consume the one-time init slot.
    assert_eq!(client.try_get(), Err(Ok(Error::NotInitialized)));
}

#[test]
fn init_rejects_a_dispute_timeout_without_an_arbiter() {
    let env = TestEnv::at(START_TS).env().clone();
    env.mock_all_auths();
    let depositor = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let token = Address::generate(&env);
    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));

    assert_eq!(
        client.try_init(
            &depositor,
            &beneficiary,
            &None,
            &token,
            &AMOUNT,
            &DEADLINE,
            &Some(TIMEOUT),
        ),
        Err(Ok(Error::EscrowNoArbiter))
    );
    assert_eq!(client.try_get(), Err(Ok(Error::NotInitialized)));
}

#[test]
fn dispute_records_when_the_dispute_started() {
    let f = Fixture::disputed();
    assert_eq!(f.client.get().disputed_at, Some(DISPUTED_AT));
}

#[test]
fn resolve_timeout_requires_a_disputed_escrow() {
    // Created: nothing has been funded or disputed.
    let created = Fixture::with_dispute_timeout(TIMEOUT);
    assert_eq!(
        created.client.try_resolve_timeout(&created.depositor),
        Err(Ok(Error::EscrowNotDisputed))
    );

    // Funded but not disputed, even well past any timeout.
    let funded = Fixture::with_dispute_timeout(TIMEOUT);
    funded.client.fund();
    funded
        .env
        .ledger()
        .with_mut(|l| l.timestamp = DISPUTED_AT + TIMEOUT);
    assert_eq!(
        funded.client.try_resolve_timeout(&funded.depositor),
        Err(Ok(Error::EscrowNotDisputed))
    );
}

#[test]
fn resolve_timeout_is_rejected_when_no_timeout_was_configured() {
    let f = Fixture::funded(true);
    f.client.dispute(&f.depositor);
    f.advance_past_deadline();
    assert_eq!(
        f.client.try_resolve_timeout(&f.depositor),
        Err(Ok(Error::EscrowNoDisputeTimeout))
    );
    assert_eq!(f.client.state(), State::Disputed);
    assert_eq!(f.escrow_balance(), AMOUNT);
}

#[test]
fn resolve_timeout_is_rejected_one_second_before_the_timeout() {
    let f = Fixture::disputed();
    f.env
        .ledger()
        .with_mut(|l| l.timestamp = DISPUTED_AT + TIMEOUT - 1);
    assert_eq!(
        f.client.try_resolve_timeout(&f.depositor),
        Err(Ok(Error::EscrowDisputeTimeoutNotReached))
    );
    assert_eq!(f.client.state(), State::Disputed);
    assert_eq!(f.escrow_balance(), AMOUNT);
}

#[test]
fn resolve_timeout_refunds_the_depositor_at_exactly_the_timeout() {
    // Boundary: the timeout is inclusive, so the call succeeds at
    // `disputed_at + dispute_timeout` and not one second earlier.
    let f = Fixture::disputed();
    let depositor_before = f.token.balance(&f.depositor);
    f.env
        .ledger()
        .with_mut(|l| l.timestamp = DISPUTED_AT + TIMEOUT);

    f.client.resolve_timeout(&f.beneficiary);

    assert_eq!(f.client.state(), State::Refunded);
    assert_eq!(f.token.balance(&f.depositor), depositor_before + AMOUNT);
    assert_eq!(f.escrow_balance(), 0);
}

#[test]
fn resolve_timeout_is_callable_by_either_party_but_not_by_others() {
    for caller_is_depositor in [true, false] {
        let f = Fixture::disputed();
        f.env
            .ledger()
            .with_mut(|l| l.timestamp = DISPUTED_AT + TIMEOUT);
        let caller = if caller_is_depositor {
            f.depositor.clone()
        } else {
            f.beneficiary.clone()
        };
        f.client.resolve_timeout(&caller);
        assert_eq!(f.client.state(), State::Refunded);
    }

    let f = Fixture::disputed();
    f.env
        .ledger()
        .with_mut(|l| l.timestamp = DISPUTED_AT + TIMEOUT);
    assert_eq!(
        f.client.try_resolve_timeout(&f.arbiter),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        f.client.try_resolve_timeout(&f.outsider),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(f.client.state(), State::Disputed);
}

#[test]
fn arbiter_resolving_just_before_the_timeout_wins() {
    let f = Fixture::disputed();
    f.env
        .ledger()
        .with_mut(|l| l.timestamp = DISPUTED_AT + TIMEOUT - 1);
    f.client.resolve(&5_000);
    assert_eq!(f.client.state(), State::Resolved);

    // Once the arbiter has decided, the timeout path is closed, even after
    // the timeout would have elapsed.
    f.env
        .ledger()
        .with_mut(|l| l.timestamp = DISPUTED_AT + TIMEOUT);
    assert_eq!(
        f.client.try_resolve_timeout(&f.depositor),
        Err(Ok(Error::EscrowNotDisputed))
    );
    assert_eq!(f.escrow_balance(), 0);
}

#[test]
fn resolve_timeout_emits_refunded_event() {
    let f = Fixture::disputed();
    f.env
        .ledger()
        .with_mut(|l| l.timestamp = DISPUTED_AT + TIMEOUT);
    f.client.resolve_timeout(&f.beneficiary);

    let all = f.env.events().all();
    let events = all.events();
    let last = events.last().expect("no events emitted");
    let expected = Refunded {
        depositor: f.depositor.clone(),
        amount: AMOUNT,
        refunded_by: f.beneficiary.clone(),
    };
    assert_eq!(last, &expected.to_xdr(&f.env, &f.client.address));
}
