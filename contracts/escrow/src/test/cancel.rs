// cancel

use super::*;

// --------------------------------------------------------------- cancel

#[test]
fn cancel_transitions_created_to_refunded() {
    let f = Fixture::new(true);
    f.client.cancel();
    assert_eq!(f.client.state(), State::Refunded);
}

// `cancel` takes no caller argument -- it authorizes internally via
// `escrow.depositor.require_auth()` -- so "wrong caller" isn't expressible
// as a parameter under this fixture's blanket `mock_all_auths()`; that would
// need per-call `set_auths` to simulate a missing depositor signature, which
// is out of scope here. A prior version of this test called
// `try_cancel(&some_address)` against a signature `cancel()` no longer has.

#[test]
fn cancel_rejects_a_funded_escrow() {
    let f = Fixture::funded(true);
    assert_eq!(f.client.try_cancel(), Err(Ok(Error::EscrowNotCancellable)));
}

#[test]
fn cancel_emits_cancelled_event() {
    let f = Fixture::new(true);
    f.client.cancel();

    // `events().all()` only returns events from the *last* invocation, so
    // only `cancel`'s own `Cancelled` is visible here -- `init`'s `Created`
    // (from an earlier, separate call) already scrolled out of view.
    let all = f.env.events().all();
    let events = all.events();
    assert_eq!(events.len(), 1);
    let expected = Cancelled {
        depositor: f.depositor.clone(),
    };
    assert_eq!(&events[0], &expected.to_xdr(&f.env, &f.client.address));
}

#[test]
fn cancel_does_not_move_any_tokens() {
    let f = Fixture::new(true);
    let depositor_before = f.token.balance(&f.depositor);
    f.client.cancel();

    assert_eq!(f.token.balance(&f.depositor), depositor_before);
    assert_eq!(f.escrow_balance(), 0);
}
