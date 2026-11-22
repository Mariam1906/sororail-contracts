// fund

use super::*;

// ------------------------------------------------------------------ fund

#[test]
fn fund_moves_the_tokens_into_the_contract() {
    let f = Fixture::new(true);
    let before = f.token.balance(&f.depositor);
    f.client.fund();

    assert_eq!(f.escrow_balance(), AMOUNT);
    assert_eq!(f.token.balance(&f.depositor), before - AMOUNT);
    assert_eq!(f.client.state(), State::Funded);
}

#[test]
fn fund_rejects_a_second_call() {
    let f = Fixture::funded(true);
    assert_eq!(f.client.try_fund(), Err(Ok(Error::EscrowNotFundable)));
}

/// `fund` writes `State::Funded` before the token pull. If the depositor cannot
/// pay, the host reverts the invocation so a later `state()` is still `Created`.
#[test]
fn fund_reverts_cleanly_when_depositor_cannot_pay() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);

    let depositor = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let issuer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token_address = sac.address();
    // Deliberately mint nothing to the depositor.
    let token = TokenClient::new(&env, &token_address);

    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));
    client.init(
        &depositor,
        &beneficiary,
        &Some(arbiter),
        &token_address,
        &AMOUNT,
        &DEADLINE,
    );
    assert_eq!(client.state(), State::Created);

    assert!(client.try_fund().is_err());
    assert_eq!(
        client.state(),
        State::Created,
        "optimistic Funded write leaked"
    );
    assert_eq!(token.balance(&client.address), 0);
    assert_eq!(client.get().amount, AMOUNT);
}

#[test]
fn fund_rejects_a_closed_escrow() {
    let f = Fixture::funded(true);
    f.client.release(&f.depositor);
    assert_eq!(f.client.try_fund(), Err(Ok(Error::EscrowClosed)));
}

/// `fund` must reject when the deadline has already passed, even if the
/// escrow is still in `Created` state (#102).
#[test]
fn fund_rejects_after_the_deadline() {
    let f = Fixture::new(true);
    f.advance_past_deadline();
    assert_eq!(f.client.try_fund(), Err(Ok(Error::DeadlinePassed)));
    assert_eq!(f.escrow_balance(), 0);
    assert_eq!(f.client.state(), State::Created);
}

#[test]
fn state_is_terminal_for_each_lifecycle_variant() {
    assert!(!State::Created.is_terminal());
    assert!(!State::Funded.is_terminal());
    assert!(State::Released.is_terminal());
    assert!(State::Refunded.is_terminal());
    assert!(!State::Disputed.is_terminal());
    assert!(State::Resolved.is_terminal());
}

/// `fund` must reject at exactly the deadline timestamp (#102).
///
/// Boundary: matches `refund_at_exactly_the_deadline_is_allowed` -- the
/// instant the depositor's unilateral refund right begins is also the
/// instant funding stops being allowed.
#[test]
fn fund_rejects_at_exactly_the_deadline() {
    let f = Fixture::new(true);
    f.env.ledger().with_mut(|l| l.timestamp = DEADLINE);
    assert_eq!(f.client.try_fund(), Err(Ok(Error::DeadlinePassed)));
    assert_eq!(f.client.state(), State::Created);
}

#[test]
fn fund_just_before_the_deadline_succeeds() {
    let f = Fixture::new(true);
    f.env.ledger().with_mut(|l| l.timestamp = DEADLINE - 1);
    f.client.fund();
    assert_eq!(f.client.state(), State::Funded);
}

#[test]
fn fund_emits_funded_event_with_correct_topics_and_data() {
    let f = Fixture::new(true);
    f.client.fund();

    let all = f.env.events().all();
    let events = all.events();
    assert_eq!(events.len(), 2); // Created + Funded

    let funded_event = &events[1];
    let expected = Funded {
        depositor: f.depositor.clone(),
        amount: AMOUNT,
    };
    assert_eq!(funded_event, &expected.to_xdr(&f.env, &f.client.address));
}

#[test]
#[should_panic]
fn fund_requires_the_depositors_authorization() {
    let f = Fixture::new(true);
    // Clear auths so require_auth bites.
    f.env.set_auths(&[]);
    f.client.fund();
}
