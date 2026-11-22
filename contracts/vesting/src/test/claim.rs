use super::*;

// claim

// ------------------------------------------------------------------- claim

#[test]
fn claim_is_blocked_before_the_cliff() {
    let f = Fixture::new(true);
    f.at(START + CLIFF - 1);
    assert_eq!(f.client.try_claim(), Err(Ok(Error::VestingCliffNotReached)));
    assert_eq!(f.held(), TOTAL);
}

#[test]
fn claim_exactly_at_and_one_second_before_cliff() {
    let env = Env::default();
    let g = bare(&env, CLIFF, DURATION);
    // One second before cliff_at(): nothing vested yet.
    assert_eq!(g.claimable_at(START + CLIFF - 1), Ok(0));
    // Exactly at cliff_at(): cliff vests its proportion in one step.
    assert_eq!(g.claimable_at(START + CLIFF), Ok(TOTAL / 10));
}

#[test]
fn claim_fails_one_second_before_cliff_but_succeeds_at_cliff() {
    let f = Fixture::new(true);
    f.at(START + CLIFF - 1);
    assert_eq!(f.client.try_claim(), Err(Ok(Error::VestingCliffNotReached)));
    f.at(START + CLIFF);
    assert_eq!(f.client.claim(), TOTAL / 10);
    assert_eq!(f.token.balance(&f.beneficiary), TOTAL / 10);
    f.assert_conserved();
}

#[test]
fn claim_pays_the_vested_portion() {
    let f = Fixture::new(true);
    f.at(START + 500);
    assert_eq!(f.client.claim(), TOTAL / 2);
    assert_eq!(f.token.balance(&f.beneficiary), TOTAL / 2);
    f.assert_conserved();
}

/// Pins the wire shape indexers decode (SPEC.md `indexed_events`). The
/// expected value is spelled out via `events::Claimed`, so renaming a topic
/// or a field fails here.
#[test]
fn claim_emits_the_claimed_event() {
    let f = Fixture::new(true);
    f.at(START + 500);
    f.client.claim();

    let expected = Claimed {
        beneficiary: f.beneficiary.clone(),
        amount: TOTAL / 2,
        total_claimed: TOTAL / 2,
    };
    // filter_by_contract returns ContractEvents; .events() gives &[ContractEvent].
    // The contract emits `created` at setup and `claimed` here -- take the last one.
    let all = f.env.events().all().filter_by_contract(&f.client.address);
    let events = all.events();
    let last = events.last().expect("no events emitted");
    assert_eq!(last, &expected.to_xdr(&f.env, &f.client.address));
}

#[test]
fn claim_twice_in_the_same_second_has_nothing_left() {
    let f = Fixture::new(true);
    f.at(START + 500);
    f.client.claim();
    assert_eq!(f.client.try_claim(), Err(Ok(Error::VestingNothingToClaim)));
    f.assert_conserved();
}

#[test]
fn incremental_claims_sum_to_the_total() {
    let f = Fixture::new(true);
    for step in 1..=10u64 {
        f.at(START + step * 100);
        f.client.claim();
        f.assert_conserved();
    }
    assert_eq!(f.token.balance(&f.beneficiary), TOTAL);
    assert_eq!(f.held(), 0);
}

#[test]
fn claim_after_the_end_takes_everything() {
    let f = Fixture::new(true);
    f.at(START + DURATION + 10_000);
    assert_eq!(f.client.claim(), TOTAL);
    assert_eq!(f.held(), 0);
    f.assert_conserved();
}

/// #216 – After the end date the full remaining amount is claimable and
/// `grant.claimed` reaches `grant.total`.
#[test]
fn claim_after_end_date_claimed_field_reaches_total() {
    let f = Fixture::new(true);
    f.at(START + DURATION + 1);
    let claimable = f.client.claimable();
    assert_eq!(claimable, TOTAL, "full amount must be claimable after end");
    let returned = f.client.claim();
    assert_eq!(returned, TOTAL, "claim must return the full total");
    let grant = f.client.get();
    assert_eq!(
        grant.claimed, grant.total,
        "claimed field must equal total after claiming everything"
    );
    f.assert_conserved();
}

#[test]
#[should_panic]
fn claim_requires_the_beneficiarys_authorization() {
    let f = Fixture::new(true);
    f.at(START + 500);
    f.env.set_auths(&[]);
    f.client.claim();
}

/// #218 – A non-beneficiary caller is rejected; only the beneficiary may claim.
///
/// Strips all authorisations so no address can satisfy `require_auth`.
/// Soroban panics on a failed `require_auth`, which `should_panic` catches.
#[test]
#[should_panic]
fn claim_is_callable_only_by_the_beneficiary() {
    let f = Fixture::new(true);
    f.at(START + 500);
    // Remove all auth so neither beneficiary nor anyone else is authorised.
    f.env.set_auths(&[]);
    f.client.claim();
}

// issue #224: claimable after partial claim

// ------------------------------------------------------- issue #224: claimable after partial claim

#[test]
fn claimable_equals_vested_minus_claimed_after_partial_claim() {
    let f = Fixture::new(true);
    // Advance to halfway through the schedule and make a first claim.
    f.at(START + 500);
    let first_claim = f.client.claim();
    assert_eq!(first_claim, TOTAL / 2);

    // Advance further; more has vested.
    f.at(START + 750);
    let claimable = f.client.claimable();
    let grant = f.client.get();
    // claimable must equal vested_at_now minus already_claimed.
    let vested_now = f.client.vested_amount(&(START + 750));
    assert_eq!(claimable, vested_now - grant.claimed);
    f.assert_conserved();
}
