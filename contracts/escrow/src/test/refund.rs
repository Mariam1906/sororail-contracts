// refund

use super::*;

// ---------------------------------------------------------------- refund

#[test]
fn refund_by_depositor_is_blocked_before_the_deadline() {
    let f = Fixture::funded(true);
    assert_eq!(
        f.client.try_refund(&f.depositor),
        Err(Ok(Error::DeadlineNotReached))
    );
    // Funds stay put.
    assert_eq!(f.escrow_balance(), AMOUNT);
}

#[test]
fn refund_by_depositor_succeeds_after_the_deadline() {
    let f = Fixture::funded(true);
    let before = f.token.balance(&f.depositor);
    f.advance_past_deadline();
    f.client.refund(&f.depositor);

    assert_eq!(f.token.balance(&f.depositor), before + AMOUNT);
    assert_eq!(f.escrow_balance(), 0);
    assert_eq!(f.client.state(), State::Refunded);
}

#[test]
fn refund_emits_refunded_event_with_correct_topics_and_data() {
    let f = Fixture::funded(true);
    f.advance_past_deadline();
    f.client.refund(&f.depositor);

    // `refund` emits the token's own transfer event before its own
    // `Refunded`, so take the last event rather than assuming an index.
    let all = f.env.events().all();
    let events = all.events();
    let last = events.last().expect("no events emitted");
    let expected = Refunded {
        depositor: f.depositor.clone(),
        amount: AMOUNT,
        refunded_by: f.depositor.clone(),
    };
    assert_eq!(last, &expected.to_xdr(&f.env, &f.client.address));
}

#[test]
fn refund_by_arbiter_is_allowed_before_the_deadline() {
    let f = Fixture::funded(true);
    f.client.refund(&f.arbiter);
    assert_eq!(f.escrow_balance(), 0);
    assert_eq!(f.client.state(), State::Refunded);
}

#[test]
fn refund_moves_the_funded_tokens_back_to_the_depositor() {
    // The other refund tests check `escrow_balance()` and `state()` but
    // never that the depositor's own balance actually went back up -- a
    // regression that zeroed the escrow without paying anyone would still
    // pass those (#256). Covers both callers the funds can move under:
    // the depositor themselves (after the deadline) and the arbiter (at
    // any time).
    let f = Fixture::funded(true);
    let depositor_before = f.token.balance(&f.depositor);
    f.advance_past_deadline();
    f.client.refund(&f.depositor);
    assert_eq!(f.token.balance(&f.depositor), depositor_before + AMOUNT);
    assert_eq!(f.escrow_balance(), 0);

    let g = Fixture::funded(true);
    let depositor_before = g.token.balance(&g.depositor);
    g.client.refund(&g.arbiter);
    assert_eq!(g.token.balance(&g.depositor), depositor_before + AMOUNT);
    assert_eq!(g.escrow_balance(), 0);
}

#[test]
fn refund_is_not_callable_by_the_beneficiary_or_an_outsider() {
    let f = Fixture::funded(true);
    f.advance_past_deadline();
    assert_eq!(
        f.client.try_refund(&f.beneficiary),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        f.client.try_refund(&f.outsider),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn refund_at_exactly_the_deadline_is_allowed() {
    // Boundary: the deadline is inclusive of the refund right.
    let f = Fixture::funded(true);
    f.env.ledger().with_mut(|l| l.timestamp = DEADLINE);
    f.client.refund(&f.depositor);
    assert_eq!(f.client.state(), State::Refunded);
}
