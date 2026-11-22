// init

use super::*;

// ------------------------------------------------------------------ init

#[test]
fn init_stores_the_agreement() {
    let f = Fixture::new(true);
    let e = f.client.get();
    assert_eq!(e.depositor, f.depositor);
    assert_eq!(e.beneficiary, f.beneficiary);
    assert_eq!(e.arbiter, Some(f.arbiter.clone()));
    assert_eq!(e.amount, AMOUNT);
    assert_eq!(e.deadline, DEADLINE);
    assert_eq!(e.state, State::Created);
    // Nothing has moved yet.
    assert_eq!(f.escrow_balance(), 0);
}

#[test]
fn get_returns_all_agreement_fields() {
    let f = Fixture::new(true);
    let e = f.client.get();
    assert_eq!(e.depositor, f.depositor);
    assert_eq!(e.beneficiary, f.beneficiary);
    assert_eq!(e.arbiter, Some(f.arbiter.clone()));
    assert_eq!(e.token, f.token.address);
    assert_eq!(e.amount, AMOUNT);
    assert_eq!(e.deadline, DEADLINE);
    assert_eq!(e.state, State::Created);

    // Without an arbiter, `None` round-trips too.
    let g = Fixture::new(false);
    let eg = g.client.get();
    assert_eq!(eg.arbiter, None);
    assert_eq!(eg.token, g.token.address);
    assert_eq!(eg.amount, AMOUNT);
    assert_eq!(eg.deadline, DEADLINE);
}

#[test]
fn state_returns_each_variant_across_lifecycle() {
    // Created -> Funded -> Released.
    let f = Fixture::new(true);
    assert_eq!(f.client.state(), State::Created);
    f.client.fund();
    assert_eq!(f.client.state(), State::Funded);
    f.client.release(&f.depositor);
    assert_eq!(f.client.state(), State::Released);

    // Created -> Funded -> Refunded.
    let g = Fixture::new(true);
    assert_eq!(g.client.state(), State::Created);
    g.client.fund();
    assert_eq!(g.client.state(), State::Funded);
    g.advance_past_deadline();
    g.client.refund(&g.depositor);
    assert_eq!(g.client.state(), State::Refunded);

    // Created -> Funded -> Disputed -> Resolved.
    let h = Fixture::new(true);
    assert_eq!(h.client.state(), State::Created);
    h.client.fund();
    assert_eq!(h.client.state(), State::Funded);
    h.client.dispute(&h.depositor);
    assert_eq!(h.client.state(), State::Disputed);
    h.client.resolve(&5_000);
    assert_eq!(h.client.state(), State::Resolved);
}

#[test]
fn init_rejects_a_second_call() {
    let f = Fixture::new(true);
    let res = f.client.try_init(
        &f.depositor,
        &f.beneficiary,
        &None,
        &f.token.address,
        &AMOUNT,
        &DEADLINE,
    );
    assert_eq!(res, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn init_rejects_arbiter_equal_to_depositor_or_beneficiary() {
    let env = TestEnv::at(START_TS).env().clone();
    env.mock_all_auths();
    let depositor = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let neutral_arbiter = Address::generate(&env);
    let token = Address::generate(&env);

    // Each failure happens on a fresh contract so we can also prove an
    // invalid configuration does not consume the one-time init slot.
    for conflicted_arbiter in [depositor.clone(), beneficiary.clone()] {
        let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));
        assert_eq!(
            client.try_init(
                &depositor,
                &beneficiary,
                &Some(conflicted_arbiter),
                &token,
                &AMOUNT,
                &DEADLINE,
            ),
            Err(Ok(Error::EscrowArbiterNotNeutral))
        );
        assert_eq!(client.try_get(), Err(Ok(Error::NotInitialized)));

        // A corrected, independent arbiter can still initialize the same
        // contract; the failed attempt did not persist any agreement.
        client.init(
            &depositor,
            &beneficiary,
            &Some(neutral_arbiter.clone()),
            &token,
            &AMOUNT,
            &DEADLINE,
        );
        assert_eq!(client.get().arbiter, Some(neutral_arbiter.clone()));
    }
}

#[test]
fn init_allows_no_arbiter_even_when_depositor_equals_beneficiary() {
    let env = TestEnv::at(START_TS).env().clone();
    env.mock_all_auths();
    let party = Address::generate(&env);
    let token = Address::generate(&env);
    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));

    // Neutrality applies only when Some(arbiter) is requested. Existing
    // no-arbitration and same-party agreements remain backward-compatible.
    client.init(&party, &party, &None, &token, &AMOUNT, &DEADLINE);
    let agreement = client.get();
    assert_eq!(agreement.arbiter, None);
    assert_eq!(agreement.depositor, agreement.beneficiary);
}

#[test]
fn init_rejects_non_positive_amounts() {
    let env = TestEnv::at(START_TS).env().clone();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);
    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let t = Address::generate(&env);

    for bad in [0i128, -1, i128::MIN] {
        let res = client.try_init(&a, &b, &None, &t, &bad, &DEADLINE);
        assert_eq!(
            res,
            Err(Ok(Error::InvalidAmount)),
            "amount {bad} was accepted"
        );
    }
}

#[test]
fn init_rejects_a_deadline_that_is_not_in_the_future() {
    let env = TestEnv::at(START_TS).env().clone();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);
    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let t = Address::generate(&env);

    // Equal to now, and in the past.
    for bad in [START_TS, START_TS - 1, 0] {
        let res = client.try_init(&a, &b, &None, &t, &AMOUNT, &bad);
        assert_eq!(
            res,
            Err(Ok(Error::InvalidTimeRange)),
            "deadline {bad} was accepted"
        );
    }
}

#[test]
fn entry_points_error_before_init() {
    let env = TestEnv::new().env().clone();
    env.mock_all_auths();
    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));
    let who = Address::generate(&env);

    assert_eq!(client.try_fund(), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_cancel(), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_release(&who), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_refund(&who), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_dispute(&who), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_resolve(&5_000), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_state(), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_bump(), Err(Ok(Error::NotInitialized)));
    assert_eq!(client.try_get(), Err(Ok(Error::NotInitialized)));
}

#[test]
#[should_panic]
fn init_requires_the_depositors_authorization() {
    // No mock_all_auths: require_auth must actually bite.
    let env = Env::default();
    env.ledger().with_mut(|l| l.timestamp = START_TS);
    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));
    client.init(
        &Address::generate(&env),
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &AMOUNT,
        &DEADLINE,
    );
}

#[test]
fn init_emits_created_event_with_correct_topics_and_data() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = START_TS);

    let depositor = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token_address = env.register_stellar_asset_contract_v2(issuer).address();
    let client = EscrowContractClient::new(&env, &env.register(EscrowContract, ()));

    client.init(
        &depositor,
        &beneficiary,
        &Some(arbiter.clone()),
        &token_address,
        &AMOUNT,
        &DEADLINE,
    );

    let all = env.events().all();
    let events = all.events();
    assert_eq!(events.len(), 1);

    let created_event = &events[0];
    let expected = Created {
        depositor: depositor.clone(),
        beneficiary: beneficiary.clone(),
        token: token_address.clone(),
        amount: AMOUNT,
        deadline: DEADLINE,
    };
    assert_eq!(created_event, &expected.to_xdr(&env, &client.address));
}

#[test]
fn bump_restores_the_instance_ttl_after_ledger_advance() {
    let f = Fixture::new(false);
    f.env
        .ledger()
        .set_sequence_number(f.env.ledger().sequence() + INSTANCE_BUMP - 1);
    f.env.as_contract(&f.client.address, || {
        assert!(f.env.storage().instance().get_ttl() < INSTANCE_BUMP);
    });
    f.client.bump();
    f.env.as_contract(&f.client.address, || {
        assert_eq!(f.env.storage().instance().get_ttl(), INSTANCE_BUMP);
    });
}
